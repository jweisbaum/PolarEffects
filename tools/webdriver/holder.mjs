/**
 * One held application, for the processes that keep one open across calls:
 * the MCP server (`mcp.mjs`) and the CLI's `serve`.
 *
 * The application runs in a detached process group, so a start that is lost
 * track of is an orphan — npm, Vite, cargo and the app, holding a port and a
 * temporary data root until someone finds them by hand. Three ways that
 * happened before this existed, each covered here and in `mcp.test.mjs`:
 *
 * - `ready()` failing after the launch succeeded left the started app
 *   running with nobody holding it: now it is closed before the error goes on.
 * - `stop` during a start answered "nothing was running" while the start
 *   carried on: now it cancels the start (the launch's `signal`), waits for
 *   it to wind down, and closes whatever it had already started.
 * - The client going away (stdin closing) during a cold start exited without
 *   stopping it: the shutdown path is `stop()`, so the same applies.
 */

import { connect as connectTo, launch as launchApp } from "./client.mjs";

export class AppHolder {
  /**
   * `launch({ signal })` starts an application and resolves with its
   * `Driver`; `connect(port)` attaches to one somebody else runs. Both are
   * parameters so the lifecycle can be tested without starting anything.
   */
  constructor({ launch = launchApp, connect = connectTo, port = null } = {}) {
    this.launch = launch;
    this.connect = connect;
    this.port = port;
    this.driver = null;
    this.starting = null;
    this.abort = null;
  }

  /** The application, started (once, however many callers ask) if need be. */
  async get() {
    if (this.driver) return this.driver;
    if (this.port) {
      this.driver = this.connect(this.port);
      return this.driver;
    }
    if (!this.starting) {
      const abort = new AbortController();
      this.abort = abort;
      this.starting = this.#start(abort.signal).finally(() => {
        this.starting = null;
        if (this.abort === abort) this.abort = null;
      });
    }
    return this.starting;
  }

  async #start(signal) {
    // `launch` stops its own tree when it fails or is cancelled.
    const started = await this.launch({ signal });
    try {
      await started.ready({ signal });
      signal.throwIfAborted();
    } catch (error) {
      await started.close();
      throw error;
    }
    this.driver = started;
    return started;
  }

  /**
   * Stops what this holder started, including a start still in flight.
   * Resolves with what it did, for the MCP tool's answer.
   */
  async stop() {
    if (this.starting) {
      const pending = this.starting;
      this.abort?.abort();
      await pending.catch(() => {});
      // A start that won the race with the cancel has set `driver`.
      if (this.driver) {
        await this.driver.close();
        this.driver = null;
      }
      return "stopped while starting";
    }
    if (!this.driver) return "nothing was running";
    const owned = Boolean(this.driver.child);
    await this.driver.close();
    this.driver = null;
    return owned ? "stopped" : "released (that application belongs to whoever started it)";
  }
}
