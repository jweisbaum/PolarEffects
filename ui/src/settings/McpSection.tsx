/**
 * Settings → MCP service (spec.md 3.7): the switch, the port, a button that
 * writes the service into Claude Code's or Codex's own configuration (and
 * the skill that says when to use it, where the client has skills) or opens
 * the Claude Desktop extension, and the same configuration as text for
 * every other client.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import type { McpClient } from "../generated/McpClient";
import type { McpStatus } from "../generated/McpStatus";
import { t, useT } from "../i18n";
import { api } from "../ipc";
import IntegerField from "./IntegerField";

/** The Claude Code command, a generic HTTP client entry, and a stdio bridge for clients that speak only stdio. */
export function clientSnippets(status: McpStatus): { claudeCode: string; json: string; bridge: string } {
  const url = `http://127.0.0.1:${status.port}/mcp`;
  const auth = `Bearer ${status.token}`;
  return {
    claudeCode: `claude mcp add --scope user --transport http polarexplorer ${url} --header "Authorization: ${auth}"`,
    json: JSON.stringify({ mcpServers: { polarexplorer: { type: "http", url, headers: { Authorization: auth } } } }, null, 2),
    bridge: `npx -y mcp-remote ${url} --header "Authorization:${auth}"`,
  };
}

/** Product names, the same in every language. */
const CLIENT_NAMES: Readonly<Record<McpClient, string>> = {
  claude_code: "Claude Code",
  codex: "Codex",
  claude_desktop: "Claude Desktop",
};

/**
 * Claude Desktop is not given a configuration but an extension, which reads
 * the port and the token from the settings file as it goes. Nothing it holds
 * goes stale, and the installing is Claude Desktop's own dialog to finish: the
 * button can say it was opened there, never that it was added.
 */
const INSTALLS_ITSELF: ReadonlySet<McpClient> = new Set<McpClient>(["claude_desktop"]);

/**
 * What a client was last given, so the button can say when that has gone
 * stale: a rotated token or a changed port leaves the client holding a
 * configuration the listener no longer answers.
 */
function registrationKey(status: McpStatus): string {
  return `${status.port} ${status.token}`;
}

/** The button's words: not yet added, added as it stands, or added and since changed. */
export function registerLabel(client: McpClient, given: string | undefined, status: McpStatus): string {
  const name = CLIENT_NAMES[client];
  if (given === undefined) return t("Add to {client}", { client: name });
  if (INSTALLS_ITSELF.has(client)) return t("Opened in {client}", { client: name });
  return given === registrationKey(status)
    ? t("Added to {client}", { client: name })
    : t("Update in {client}", { client: name });
}

/** How many clients are connected, and the last tool one called. */
function connectedLabel(sessions: number, lastTool: string | null): string {
  if (sessions === 0) return t("No client connected");
  if (lastTool) {
    return sessions === 1
      ? t("1 client connected, last: {tool}", { tool: lastTool })
      : t("{count} clients connected, last: {tool}", { count: sessions, tool: lastTool });
  }
  return sessions === 1 ? t("1 client connected") : t("{count} clients connected", { count: sessions });
}

export default function McpSection({ onError }: { onError: (err: unknown) => void }) {
  const t = useT();
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [copied, setCopied] = useState(false);
  // Remembered for as long as the dialog is open and no longer. The clients'
  // files are theirs and are not read back to find out: a reopened dialog
  // offers "Add" again, and adding twice is harmless.
  const [given, setGiven] = useState<Partial<Record<McpClient, string>>>({});
  const [adding, setAdding] = useState<McpClient | null>(null);
  // Where each client's skill was written. Claude Code reads its own; Claude
  // Desktop's is a zip only the person can upload, so the path is shown.
  const [skills, setSkills] = useState<Partial<Record<McpClient, string>>>({});

  // `onError` may be a new function on every re-render of the dialog. Read
  // through a ref, never a dependency, so the fetch-once effect below does
  // not run again on someone else's state change.
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  useEffect(() => {
    void api.mcpStatus().then(setStatus).catch((err) => onErrorRef.current(err));
  }, []);

  const apply = useCallback((promise: Promise<McpStatus>) => {
    void promise.then(setStatus).catch((err) => onErrorRef.current(err));
  }, []);

  const copy = useCallback((text: string) => {
    void navigator.clipboard?.writeText(text).then(() => setCopied(true)).catch(() => setCopied(false));
  }, []);

  const register = useCallback((client: McpClient, current: McpStatus) => {
    setAdding(client);
    void api.registerMcpClient(client)
      .then((registered) => {
        setGiven((held) => ({ ...held, [client]: registrationKey(current) }));
        const skill = registered.skill;
        if (skill) setSkills((held) => ({ ...held, [client]: skill }));
      })
      .catch((err) => onErrorRef.current(err))
      .finally(() => setAdding(null));
  }, []);

  // Said whether the service is on or off: someone looking for the button
  // should find the reason there is none.
  const chatgpt = (
    <p className="muted mcp-chatgpt">
      {t("ChatGPT is not offered: it reaches MCP servers only over the public internet, and this service answers only on this computer.")}
    </p>
  );

  if (status === null) {
    return (
      <section className="mcp-settings" data-section="settings:mcp">
        <h3>{t("MCP service")}</h3>
        <span>…</span>
      </section>
    );
  }
  const snippets = clientSnippets(status);
  const url = `http://127.0.0.1:${status.port}/mcp`;

  return (
    <section className="mcp-settings" data-section="settings:mcp">
      <h3>{t("MCP service")}</h3>
      <p className="muted">
        {t("Lets an AI client on this computer drive PolarExplorer: open projects, add certificates, polar files and tracks, fetch weather, filter, weight, edit, compare and export, and look at the result. Every change it makes can be undone. Nothing outside this computer can reach it, and nothing can reach it while it is off.")}
      </p>
      <label className="settings-field settings-check">
        <input type="checkbox" data-feature="settings:mcp-enable" checked={status.enabled}
          title={t("Turning it on issues a new token; turning it off closes the port and forgets the token")}
          onChange={(event) => apply(api.setMcp(event.target.checked, status.port))} />
        {t("Enable the MCP service on this computer")}
      </label>
      <label className="settings-field">
        {t("Port")}
        <IntegerField data-feature="settings:mcp-port" aria-label={t("Port")}
          title={t("The port on this computer the service listens on (1–65535)")}
          value={status.port} min={1} max={65535}
          onCommit={(port) => apply(api.setMcp(status.enabled, port))} />
      </label>
      {/* The system's own (English) reason is the tooltip, as every error's is. */}
      {status.bind_error && <p className="modal-error" role="alert" title={status.bind_error}>
        {t("The service could not start listening on port {port}. Another application may be using that port: choose another.", { port: status.port })}
      </p>}
      {status.enabled && (
        <>
          <div className="settings-field mcp-value">
            {/* i18n-ignore: an abbreviation, the same in every language */}
            <span>URL</span>
            <code>{url}</code>
          </div>
          <div className="settings-field mcp-value">
            <span>{t("Token")}</span>
            <code>{status.token}</code>
            <button className="small" data-feature="settings:mcp-token"
              title={t("Issue a new token; clients added before must be added again")}
              onClick={() => apply(api.rotateMcpToken())}>
              {t("Rotate token")}
            </button>
          </div>
          <p className="mcp-connected" role="status">{connectedLabel(status.sessions, status.last_tool)}</p>
          <div className="settings-buttons">
            {status.clients.map((client) => (
              <button key={client} data-feature="settings:mcp-add" disabled={adding !== null}
                onClick={() => register(client, status)}>
                {registerLabel(client, given[client], status)}
              </button>
            ))}
          </div>
          {status.clients.some((client) => !INSTALLS_ITSELF.has(client) && given[client] === registrationKey(status)) && (
            <p className="muted">{t("Added. A session that is already running picks it up when it is restarted.")}</p>
          )}
          {skills.claude_code !== undefined && (
            <p className="muted">{t("Claude Code was also given a skill that says when to use PolarExplorer: {path}", { path: skills.claude_code })}</p>
          )}
          {given.claude_desktop !== undefined && (
            <p className="muted">
              {t("Claude Desktop is asking whether to install the PolarExplorer extension; confirm it there. It needs installing once: a new token or port reaches it without another visit here.")}
            </p>
          )}
          {skills.claude_desktop !== undefined && (
            <p className="muted">
              {t("A skill that says when to use PolarExplorer was written to {path}. An extension cannot carry one, so add it yourself: in Claude Desktop’s settings, under Skills, upload that file.", { path: skills.claude_desktop })}
            </p>
          )}
          {chatgpt}
          <details>
            <summary data-feature="settings:mcp-snippets">{t("Configuration for other clients")}</summary>
            <pre className="settings-snippet">{snippets.claudeCode}</pre>
            <button className="small" data-feature="settings:mcp-copy" onClick={() => copy(snippets.claudeCode)}>{t("Copy Claude Code command")}</button>
            <pre className="settings-snippet">{snippets.json}</pre>
            <button className="small" data-feature="settings:mcp-copy" onClick={() => copy(snippets.json)}>{t("Copy HTTP client JSON")}</button>
            <pre className="settings-snippet">{snippets.bridge}</pre>
            <button className="small" data-feature="settings:mcp-copy" onClick={() => copy(snippets.bridge)}>{t("Copy stdio bridge command")}</button>
            {copied && <span className="muted"> {t("Copied.")}</span>}
          </details>
        </>
      )}
      {!status.enabled && chatgpt}
    </section>
  );
}
