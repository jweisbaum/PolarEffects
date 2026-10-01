# MCP service design

**Date:** 2026-10-01. **Status:** design approved in discussion, awaiting
spec review. **Companion to** `spec.md` (invariant 4, §3.4) and `plan.md` §5.
**Follows** VectorEffects' MCP service
(`/Users/jon/VectorEffects/docs/superpowers/specs/2026-09-16-mcp-service-design.md`,
its `crates/ve-app/src/mcp/`), including the deviations that document
records in its §9. Where this text is silent, VectorEffects' answer applies.

## 1. What this is

An MCP (Model Context Protocol) server built into PolarExplorer, so an AI
client can open a project, add ORC and ORR certificates, import polar files
and tracks, fetch weather, filter, weight, edit, blend, compare, export and
look at the result, through the same commands the interface uses. It is off
unless the person switches it on in Settings, and while a client is working
the application says so.

Decisions fixed in discussion:

1. **Transport:** a loopback HTTP listener inside the shipped application,
   gated by the setting. This amends invariant 4 (§7).
2. **Surface:** curated task-level tools plus one raw `invoke` escape hatch.
3. **Consent:** a connected client may do everything the interface can, with
   a visible indicator and no extra confirmation dialogs. That includes the
   fetches the interface can start (tracker downloads, weather, the ORR
   catalogue scrape): a tool call is the person's agent asking, on the
   allow-listed hosts only, through `pe-trackers` and `pe-env` as today.
4. **Clients:** buttons for Claude Code, Codex and Claude Desktop. **No
   ChatGPT button** (settled with the user 2026-10-01): ChatGPT reaches MCP
   servers only over public HTTPS or OpenAI's Secure MCP Tunnel, never a
   loopback address, and the application neither exposes itself publicly nor
   runs a tunnel. The Settings section says so in one line.

## 2. Transport and security

- **Server:** `rmcp` (the official Rust SDK) serving Streamable HTTP through
  its tower service on a hand-written `hyper` http1 accept loop, on Tauri's
  tokio runtime. Pure Rust: `cargo tree` must show no new `-sys` crate and
  no `aws-lc-rs` (CLAUDE.md, environment gotchas). The one path is `/mcp`;
  anything else is 404. SSE keep-alive every 15 s.
- **Bind:** `127.0.0.1` only, IPv4, on the port in settings, default
  **47392** (VectorEffects holds 47391, so both applications can run at
  once). The bind is synchronous, so a taken port is an error the Settings
  section shows; the setting stays on and the next launch retries. Port 0 is
  refused by the command (a port that moves every launch is no use to a
  client configuration); only tests bind port 0, calling `server::start`
  directly.
- **Token:** 32 random bytes (`getrandom`), base64url without padding (43
  characters), issued when the setting is turned on or *Rotate token* is
  pressed, stored in `settings.json` beside the port. Every request must
  carry `Authorization: Bearer <token>`, compared in constant time; anything
  else is `401` before any MCP handling. An empty token matches nothing and
  the server refuses to start with one. Turning the setting off drops the
  listener and clears the token from the file. The settings file is already
  written owner-only (0600) on Unix.
- **Rebinding guard:** `rmcp`'s `allowed_hosts` is `127.0.0.1:<port>` and
  `localhost:<port>`, and `allowed_origins` is `http://127.0.0.1:<port>` and
  `http://localhost:<port>`, spelled literally so `check-offline.sh`'s
  loopback pattern can see they are local.
- **Lifecycle:** `McpService` (managed Tauri state) owns the running
  listener. `McpService::apply(settings)` is the only caller of start and
  stop: it always stops what is running, then starts again if `enabled`
  (the port and token are baked into a listener). It is called by
  `settings::mcp_set`, `settings::mcp_rotate_token`, and once from `setup`
  with the saved settings. Stop cancels a `CancellationToken` and waits up
  to 2 s for the listener to drop, so the port is free when it returns.
  **While the setting is off the module creates no socket, thread or task.**
- **Module:** `crates/pe-app/src/mcp/`, always compiled in (no cargo
  feature):

  | File | Holds |
  |---|---|
  | `mod.rs` | `McpService`: listener state, bind error, sessions, last tool, pending screenshots |
  | `server.rs` | bind, accept loop, auth check, hand-off to rmcp |
  | `token.rs` | token generation and constant-time comparison |
  | `events.rs` | the events sent to the frontend (§4) |
  | `capture.rs` | the screenshot round trip |
  | `clients.rs` | Add to Claude Code, Add to Codex |
  | `desktop.rs`, `bridge.js` | the Claude Desktop `.mcpb` bundle and its stdio bridge |
  | `skill.rs` | the `SKILL.md` given to clients |
  | `invoke.rs` | the `invoke` table |
  | `tools/{mod,guide,project,boats,sources,tracks,weather,blend,export,view,history,escape}.rs` | the tools, one file per group |

  The whole module is generic over `R: tauri::Runtime` so the integration
  tests can drive a `tauri::test` mock application.

## 3. Tool surface

Every tool calls the `#[tauri::command]` function the interface calls, with
`app.state::<AppState>()`. **No tool duplicates command logic**, so an
agent's edit is validated, undoable and autosaved exactly as a person's is,
and sources stay immutable behind overlays (invariant 1) with no new code
path to keep honest.

Tools that change the document return the new `ProjectSummary`. A command's
`AppError` reaches the client as a tool result with `is_error: true` and
the text the interface shows; only a panic or a malformed request is a
protocol error. Parameter structs derive `schemars::JsonSchema`; structured
parameters are also accepted as a JSON string, since some clients send
them that way. Names are `snake_case`, noun first.

**Units across this boundary are the domain's own** (CLAUDE.md,
conventions): speeds in knots, angles in degrees, wind and waves "from",
current "toward", time as UTC epoch seconds, longitude in [−180, 180). The
person's display units (Settings) are not applied; the guide says so.

| Group | Tools |
|---|---|
| Guide | `polarexplorer_guide` (start here: today's date and the full workflow guide) |
| Project | `project_status`, `project_new`, `project_open` (`discard_unsaved`), `project_save` (`path?` for Save As), `project_close` (`discard_unsaved`), `recent_projects` |
| Boats | `boats_list`, `boat_add`, `boat_rename`, `boat_remove`, `boat_restore` |
| Sources | `sources_list` (id, kind, label, colour, visible, weight, counts, edited), `source_set` (label, colour, visible, weight in [0, 1]; one undo entry per field), `source_move`, `source_remove`, `orc_search`, `orc_add`, `orr_search`, `orr_add`, `polar_files_import` |
| Tracks | `track_files_inspect`, `track_files_import`, `tracker_event` (a tracker address to its boat list), `tracker_import` (chosen boats), `track_set` (filters, heading and speed derivation, supplied wind), `sample_get`, `samples_exclude` |
| Weather | `weather_estimate`, `weather_fetch` (with progress), `weather_cancel`, `weather_jobs` |
| Blend | `blend_status` (settings, coverage), `blend_set` (grid, interpolation `linear` or `monotone_spline`, smoothing, `n_full`, asymmetric), `blend_filters_set` (global filters, wave ranges, priority groups), `polar_read` (any source, a track's segment, or the blend, as a TWA × TWS table), `blend_cell` (one output cell: value, direct or filled, each contributing source with its value and share of the weight), `polar_edit` (cell overrides on one source or the blend), `compare` (A against B: Δ and Δ % per cell, regions) |
| Export | `export_preview`, `export_polar` (Expedition, Adrena, CSV; absolute path), `export_all` (every boat) |
| View | `screenshot` (a PNG of the current stage, via the frontend), `view_stage` (map, 3D, compare, full-size plot), `view_boat` (the boat tab shown), `selection_set` |
| History | `undo`, `redo` |
| Escape hatch | `invoke` (command name, JSON arguments) |

Notes on particular tools:

- **Which boat.** A project holds independent boat tabs, and the
  interface's commands take an optional `boat_context` id. Every tool that
  reads or edits a boat takes the same optional `boat` id (from
  `boats_list`) and passes it through; without it the tool acts on the
  project's current boat, as the command does. Which tab the person sees is
  frontend state, so `view_boat` only asks the view to show one (§4); no
  tool depends on it.
- **`polar_read`** and **`blend_cell`** are read-only and derive from the
  cached blend (invariant 2): nothing they return is stored. `blend_cell`
  is the command the interface's blend tooltip uses.
- **`weather_fetch`**, **`tracker_import`** and the long imports report
  through MCP progress notifications on the request's progress token, fed
  by the same job progress the interface reads, and stop when the client
  cancels the request (invariant 6).
- **`export_polar`** always recomputes (invariant 2) and writes the same
  bytes the interface's export writes (invariant 5).
- **`invoke`** dispatches by name through a table kept beside
  `generate_handler!`, with `deny_unknown_fields` on each entry's
  arguments. An `EXCLUDED` list holds what must not be reachable: the
  `mcp_*` commands themselves, `deliver_capture`/`refuse_capture`,
  `set_language`, `quit_app`, and every `database::*` command (the
  PostgreSQL library holds saved credentials and runs `pg_dump`; it stays a
  thing only the person at the keyboard starts). A unit test holds
  `TABLE ∪ EXCLUDED == generate_handler!`, so a command added later must be
  placed on one side the day it lands.
- **Server instructions** (`get_info`) open with today's date and stay
  under 2,048 characters, because Claude Code truncates longer ones; the
  full text is the `polarexplorer_guide` tool's. A test checks that the
  instructions and the guide name only tools that exist.

MCP resources and prompts are out of scope: tools only, as in VectorEffects.

## 4. The view follows

Driving the back end alone would leave the interface behind. These events
carry the service's effects to the frontend, **emitted only by the
service** (commands invoked by the interface keep returning summaries;
emitting from there too would refresh every panel twice per edit):

- **`document://changed`** with `{project: ProjectSummary | null, opened}`
  after every tool that wrote to the document or opened, saved or closed a
  project, whether the closure returned Ok or Err (a partial write is
  always reported). `opened` is true only when a different project really
  opened; then the frontend also resets selection, stage and boat tab. A
  `null` project shows the start screen. Panels already refresh by
  `revision`.
- **`view://stage`**, **`view://boat`** and **`view://selection`**, applied
  through the same state setters the interface's own controls use.
- **`mcp://activity`** with the tool name, which drives the status-bar
  badge `MCP: <tool>` while a client session is open. Pushed, never
  polled, so it costs nothing when idle.
- **`view://capture`** with a request id for `screenshot`: the current
  stage's canvas (map, 3D, compare or plot) answers through
  `deliver_capture(id, bytes)` or `refuse_capture(id, reason)`; the tool
  waits 35 s, refuses at once with no project open, and writes nothing to
  disk. The pending capture is a guard that cleans up when a cancelled
  tool future is dropped.

## 5. Settings and client registration

- `Settings` gains `mcp: McpSettings { enabled: bool, port: u16, token:
  String }`, default `{false, 47392, ""}`, read field by field like every
  other setting, so an older `settings.json` loads with the service off.
- Commands: `mcp_status` → `McpStatus { enabled, port, token, bound_port,
  bind_error, sessions, last_tool, clients }`; `mcp_set(enabled, port)`;
  `mcp_rotate_token` (refused while off); `mcp_register_client(client)`.
- **Settings dialog**, a new *MCP service* section (`settings:mcp`) after
  *Network*:

  | Control | `data-feature` |
  |---|---|
  | "Enable the MCP service on this computer" | `settings:mcp-enable` |
  | Port (1–65535, committed on blur) | `settings:mcp-port` |
  | URL and token, shown only while on | — |
  | Rotate token | `settings:mcp-token` |
  | Connected clients and the last tool | — |
  | Add to Claude Code / Codex / Claude Desktop | `settings:mcp-clients` |
  | "Configuration for other clients": three copyable snippets | `settings:mcp-snippets` |
  | The line on ChatGPT (§1) | — |

  Every string in French and German, every control in the help search
  (invariant 7), and the Settings help topic describes the section.
- **Add to Claude Code:** runs `claude mcp remove --scope user
  polarexplorer` (failure ignored) then `claude mcp add --scope user
  --transport http polarexplorer <url> --header "Authorization: Bearer
  <token>"`, finding `claude` in the usual install folders before `PATH`;
  then writes `~/.claude/skills/polarexplorer/SKILL.md` (or under
  `$CLAUDE_CONFIG_DIR`). An error repeats what `claude` printed, never the
  token.
- **Add to Codex:** edits `$CODEX_HOME/config.toml` (else
  `~/.codex/config.toml`) in place with `toml_edit`, setting
  `[mcp_servers.polarexplorer]` `url` and `http_headers`, keeping every
  other line; written to a staging file, made 0600, renamed over the
  original. Refused, creating nothing, when the folder does not exist.
- **Add to Claude Desktop** (macOS and Windows only): writes
  `PolarExplorer.mcpb` beside the settings file (MCPB manifest 0.3, a Node
  stdio bridge using only `node:` built-ins, an icon) and
  `PolarExplorer-skill.zip`, then opens the bundle in Claude. **The bundle
  holds no secret:** the bridge re-reads the port and token from the
  settings file on every request, offers one stand-in tool
  (`polarexplorer_status`) while the application is not running, and
  follows it through a restart. The button says "Opened in", never "Added".
- The server registers as `polarexplorer` everywhere. The settings folder
  keeps its existing name (the rename is of the visible name only), which
  the bridge's `PE_SETTINGS` path carries.

## 6. Dependencies

Added to `pe-app`, all normal dependencies and none optional: `rmcp`
(`server`, `macros`, `transport-streamable-http-server`), `schemars`,
`hyper` (`server`, `http1`), `hyper-util`, `http-body-util`, `http`,
`bytes`, `tokio` (`net`, `sync`, `time`, `macros`, `rt`), `tokio-util`,
`getrandom`, `base64`, `toml_edit`. `zip` is already there. Dev-only:
`rmcp` with its client and reqwest transport, `tauri`'s `test` feature,
`tempfile`.

`reqwest` in the dev-dependency must keep the workspace's rule: no default
features, `rustls-no-provider` with `ring`. `tests/webdriver_optional.rs`
is unaffected: it holds the WebDriver dependency optional, and this service
does not touch it.

## 7. Spec and rules

- **Invariant 4** gains one recorded inbound exception, as VectorEffects'
  invariant 5 did: an inbound loopback endpoint that exists only while the
  person has switched it on, answers only to the token that switch issued,
  answers only loopback names, and drives the domain through the same
  commands the interface uses. The WebDriver rule (D25) is unchanged: that
  endpoint is unauthenticated, drives the interface rather than the domain,
  has no switch, and stays compiled out of every shipped build. A second
  inbound socket would need the same four properties and its own decision.
- **The "two crates may use the network" rule is unchanged.** The service
  accepts loopback connections and makes no outbound request of its own;
  the fetches a tool starts run in `pe-trackers` and `pe-env` as today.
  Registration spawns the person's own `claude` or opens Claude Desktop; it
  contacts no host.
- `plan.md`: decision **D29** with the reasoning above, and milestone
  **M20**. `spec.md`: invariant 4's summary, §3.4 (the settings), a new
  §3.7 "MCP service" (the tool list, registration, the skill), §15.
  `CLAUDE.md`: the matching paragraph under invariant 4, a recipe *Adding
  an MCP tool*, and the gotcha that `bridge.js` may use only `node:`
  built-ins and writes only JSON-RPC to stdout. `docs/USER-GUIDE.md`
  describes the section.
- `npm run check:offline` cannot see a listener; the *off means off* test
  (§8) is the enforcement, and a comment in `tools/check-offline.sh` says
  so. Its dependency layer changes (§10).

## 8. Testing

- **Rust integration (`crates/pe-app/tests/mcp.rs`):** a `tauri::test`
  mock application over a real `AppState` in a temporary directory, the
  server on port 0, driven by an `rmcp` client over real HTTP. Every
  curated tool is exercised, asserting the document through the returned
  summary and, where the tool has an interface twin, through the twin's own
  read, so a tool that diverged from its command fails. No test in the
  default suite uses the network: tracker and weather tools are driven
  against the recorded fixtures the existing tests use.
- **Refusals:** no token and a wrong token → 401 before MCP; a wrong
  `Host` or `Origin` → refused even with the right token; `project_open`
  on unsaved work without `discard_unsaved` → the interface's own error; a
  weight outside [0, 1] → the command's refusal.
- **Off means off:** with the setting off there is no running port before
  or after, and after on-then-off the port can be bound again.
  `mcp_set` issues a 43-character token on enable and clears it on disable.
- **Undo:** an edit made through a tool is undone by `undo` and by the
  interface's own undo, restoring the document byte for byte.
- **Events:** a write tool emits `document://changed` once; a read tool
  emits nothing.
- **Text and schema:** descriptions fit clients' limits; every tool schema
  is one a strict client accepts; the instructions and guide name only
  what exists; `invoke`'s table covers every registered command.
- **Clients:** Codex config created, preserved, idempotent, refused when
  invalid or when the folder is missing, 0600 with no staging file left; a
  fake `claude` script records the remove and add arguments; a refusal is
  reported without the token; the Desktop bundle is a valid extension with
  no secret in it; the bridge follows the application through a restart
  (skipped without `node`).
- **Frontend (`vitest`):** the Settings section (no URL while off, rotate,
  add then update after the token changes, Desktop says "Opened",
  platform-filtered clients, a failure is not claimed as added) and the
  `document://changed` handler (an edit replaces the project only, a new
  project resets state, `null` shows the start screen).
- **End to end:** a driver test in `tools/webdriver/ux/` starts the
  application with an isolated data root and the setting on, an MCP client
  changes a source's weight, and a screenshot shows the slider moved.

## 9. Out of scope

- Remote access, TLS, a tunnel, or any bind other than loopback; therefore
  ChatGPT (§1).
- Per-tool permissions or a read-only mode.
- MCP resources and prompts.
- The PostgreSQL track library through MCP (§3, `invoke`'s exclusions).
- Exposing the block cache, packets or other rendering internals.

## 10. Deviations from this design

Recorded as the service was built (2026-10-01); `spec.md` §3.7 is the
description of what exists.

- **`check:offline` did change.** Unlike VectorEffects' script, this one
  refuses any HTTP crate named in a manifest outside `pe-env` and
  `pe-trackers`. It now admits, in `pe-app` only: `hyper` with features from
  {server, http1}, `hyper-util` with {tokio}, and `rmcp` without default
  features and with features from {server, macros,
  transport-streamable-http-server}, each on one line in `[dependencies]`;
  and `reqwest` in `[dev-dependencies]` without default features. Any other
  feature, a feature list continued on another line, or a
  `[dependencies.<crate>]` table fails it.
- **Results are JSON, not typed.** Tools answer with one `json()` helper
  (text plus `structuredContent`) rather than `Json<T>` with an output
  schema, so the interface's IPC types need no `JsonSchema` derive.
- **Structured parameters are patches.** `track_set`, `blend_set` and
  `blend_filters_set` take only the fields to change, written over what
  the boat has. `clear_global` removes the global filters. A key that is
  not a field is refused with the fields listed (final review): dropped in
  silence, it left an agent believing a filter was on.
- **`track_samples` was added**: without it no tool gives the sample ids
  that `sample_get` and `samples_exclude` take. `boat_select` became
  `view_boat` plus an optional `boat` on every tool (§3).
- **Cells by value.** `blend_cell` and `polar_edit` take TWA and TWS
  values, refused with the axis listed when off the grid.
- **`invoke` excludes every settings command**, not only the language: each
  answers the whole settings file (the PostgreSQL password, the YellowBrick
  keys, the service's own token). The ORR scrape, which §1 names among
  what a client may start, is the tool `orr_refresh` (final review; first
  left out): the command takes the application handle, which `invoke`'s
  table cannot give.
- **A file already there is not replaced without `overwrite: true`**
  (final review): `project_save` to a new path and `export_polar`. §1's "no
  extra confirmation dialogs" stands; this is the caller saying it in the
  call, as `discard_unsaved` already was. `save_project_as` and
  `export_polar` are not reachable through `invoke`, which would be the way
  around it. `export_all` writes into a folder as the interface's own does.
- **Reading files is not narrowed.** `track_files_inspect` answers the
  header and first rows of any text file it is pointed at, as the
  interface's dialog (which offers "All files") can open any. Left for the
  user to decide whether tools should take only track and polar extensions.
- **A long tool watches the request's token.** `rmcp` cancels
  `ctx.ct`; it does not drop the tool's future. `weather_fetch` returns on
  it, and a guard cancels the fetch.
- **A session is uncounted only if it was counted**, where VectorEffects
  decrements on every drop. **A client that keeps no session is counted
  from its calls** (final review): `rmcp` answers a 2026-07-28 request
  without `initialize` or a session, so the indicator §1's consent rests on
  would never have shown for such a client. Stopping the listener resets
  the count at once, whatever idle sessions `rmcp` still holds.
- **`view://stage` names its boat** (final review), set on that boat's own
  view rather than through the help search's reveal steps, which only the
  boat on show hears. Without a boat the tool means the first, as every
  tool does, and shows its tab.
- **`tracker_event` does not watch the request's token yet**: an abandoned
  call's download runs to its end (bounded by the network timeout) and the
  event is kept for the session. Deferred.
- **No `tracing`.** A bind failure is stored for Settings to show; a failed
  accept is dropped.
- **The screenshot is the stage's canvas alone**, the largest one on show,
  without the panels or the labels drawn over it.
- **A contradictory settings file** (on without a token, a token while off)
  loads as off.
- **The end-to-end check** connects with the MCP SDK's own client from the
  driver suite (`tools/webdriver/ux/16-mcp-follow.test.mjs`).
