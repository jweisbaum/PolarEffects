# MCP Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An MCP server inside PolarExplorer, off until switched on in Settings, through which an AI client drives the application with the interface's own commands.

**Architecture:** A port of VectorEffects' `crates/ve-app/src/mcp/` into `crates/pe-app/src/mcp/`: `rmcp` Streamable HTTP on a hand-written `hyper` accept loop bound to `127.0.0.1`, a bearer token checked before `rmcp` sees a request, one handler per client session whose tools call the `#[tauri::command]` functions with `app.state::<AppState>()`, and Tauri events that make the open views follow. The whole module is generic over `R: tauri::Runtime` so the integration tests drive a `tauri::test` mock application over real HTTP.

**Tech Stack:** Rust (`rmcp` 3.4, `hyper` 1, `tokio`, `schemars` 1, `toml_edit`, `zip`), React + TypeScript (Settings section, event listeners), Node built-ins only for the Claude Desktop stdio bridge.

**Spec:** `docs/superpowers/specs/2026-10-01-mcp-service-design.md`. The reference implementation is `/Users/jon/VectorEffects/crates/ve-app/src/mcp/` and its tests `/Users/jon/VectorEffects/crates/ve-app/tests/mcp.rs`; where a task says *port*, copy that file and apply the renames in Global Constraints, then the task's listed differences.

## Global Constraints

- Bind `127.0.0.1` only, IPv4. Default port **47392**. Path `/mcp`; anything else 404.
- Token: 32 random bytes, base64url without padding, 43 characters; `Authorization: Bearer <token>`, constant-time comparison, `401` before any MCP handling; an empty token matches nothing.
- **While the setting is off the module creates no socket, thread or task.**
- Always compiled in; no cargo feature. No new `-sys` crate, no `aws-lc-rs`, no OpenSSL (`cargo tree -p pe-app -e normal | grep -E '\-sys|aws-lc|openssl'` shows nothing new). Dev-dependency `reqwest` has `default-features = false`.
- `tests/webdriver_optional.rs` must still pass unchanged in what it asserts about the WebDriver dependency.
- Units across the boundary are the domain's: knots, degrees, wind and waves "from", current "toward", UTC epoch seconds, longitude in [−180, 180).
- Server name `polarexplorer` in every client registration; display name `PolarExplorer`; skill name `polarexplorer`; bundle `PolarExplorer.mcpb`; skill zip `PolarExplorer-skill.zip`; bridge environment variable `PE_SETTINGS`; stand-in tool `polarexplorer_status`; guide tool `polarexplorer_guide`.
- Renames when porting: `VectorEffects` → `PolarExplorer` (the handler struct too), `vectoreffects` → `polarexplorer`, `ve_app` → `pe_app`, `VE_SETTINGS` → `PE_SETTINGS`, `.veproj` → `.wpsproj`, `tracing::{info,warn,error,debug}!` → nothing (this crate has no `tracing`; a failure that matters is returned or stored in `bind_error`), invariant "5" → "4".
- No `unwrap`/`expect` outside tests; `thiserror`/`AppError` as elsewhere in `pe-app`; doc comments say why.
- Every interface string through `t()`/`msg()` with French and German; every control a `data-feature` id registered in `ui/src/help/features/settings.ts`.
- No test in the default suite uses the network.
- The `invoke` table excludes: `mcp_status`, `mcp_set`, `mcp_rotate_token`, `mcp_register_client`, `deliver_capture`, `refuse_capture`, `set_language`, `quit_app`, every `database::*` command, and the binary-packet commands (`basemap`, `map_tracks`, `polar_scene`, `polar_plot_dots`, `compare_polars`).

## Review Focus

1. A client sends a structured parameter (filters, blend settings) as a JSON **string** rather than an object: it is parsed, not refused (`typed`).
2. The port is already taken when the setting is turned on: the error is shown in Settings, the setting stays on, nothing panics.
3. A tool is called with **no project open**: every project tool refuses with the interface's own message; `screenshot` refuses before asking the frontend.
4. A tool names a **boat or source id that does not exist**: the command's own refusal reaches the client as a tool result with `is_error`, not a protocol error.
5. The client disconnects **mid-call** on a long tool (`weather_fetch`): the progress relay unlistens and the session count returns to what it was.

Each is pinned by a test in the task that owns the code (Tasks 2, 1, 2, 3, 4).

## File Structure

```
crates/pe-app/src/mcp/
  mod.rs        McpService: listener state, bind error, activity, captures
  server.rs     bind, accept loop, auth, hand-off to rmcp           (port)
  token.rs      fresh(), matches()                                   (port)
  events.rs     document://changed, view://*, mcp://activity
  capture.rs    the screenshot round trip                            (port)
  clients.rs    Add to Claude Code, Add to Codex                     (port)
  desktop.rs    the .mcpb bundle and skill zip                       (port)
  bridge.js     stdio bridge inside the bundle                       (port)
  skill.rs      SKILL.md text and install                            (port)
  invoke.rs     the escape hatch's command table
  tools/mod.rs  PolarExplorer<R> handler, run/write/typed/json, relay
  tools/{guide,project,boats,sources,tracks,weather,blend,export,view,history,escape}.rs
crates/pe-app/src/settings.rs   McpSettings, McpStatus, mcp_* commands
crates/pe-app/src/lib.rs        manage McpService, apply at setup, register commands
crates/pe-app/tests/mcp.rs      the integration suite
ui/src/settings/McpSection.tsx (+ .test.tsx)
ui/src/mcp/follow.ts (+ .test.ts)   applyDocumentChanged
ui/src/App.tsx                  listeners, status-bar badge
```

Tool results are JSON text plus `structuredContent` built by one helper, `json(&value)`, from any `Serialize` value wrapped in an object, so the interface's IPC types need no `JsonSchema` derive. Tool **parameters** are small structs defined in each tool file that derive `Deserialize` and `JsonSchema`; a structured parameter that is an interface type (`TrackFilters`, `BlendSettingsInput`, `OrcFilters`, `ExportAxes`, `CompareOperand`, `EditOp`) is declared as `serde_json::Value` with its shape in the description and read with `typed::<T>()`.

---

### Task 1: Settings, token, listener, lifecycle

**Files:**
- Modify: `crates/pe-app/Cargo.toml`, `crates/pe-app/src/settings.rs`, `crates/pe-app/src/lib.rs`
- Create: `crates/pe-app/src/mcp/{mod,server,token,events}.rs`, `crates/pe-app/src/mcp/tools/{mod,guide}.rs`
- Test: `crates/pe-app/tests/mcp.rs`, unit tests in `token.rs` and `settings.rs`

**Interfaces:**
- Produces: `settings::McpSettings { enabled: bool, port: u16, token: String }` (default `false, 47392, ""`), `settings::DEFAULT_MCP_PORT`, `settings::McpStatus { enabled, port, token, bound_port: Option<u16>, bind_error: Option<String>, sessions: u32, last_tool: Option<String>, clients: Vec<McpClientInfo> }`; commands `mcp_status`, `mcp_set(enabled, port)`, `mcp_rotate_token` and their handle-free twins `mcp_status_of`, `mcp_set_on<R>(&AppHandle<R>, enabled, port)`, `mcp_rotate_on<R>(&AppHandle<R>)`; `mcp::McpService` with `apply`, `adopt`, `running_port`, `status`, `note_tool`, `session_delta`; `mcp::server::{start, Running, PATH}`; `mcp::token::{fresh, matches}`; `mcp::tools::PolarExplorer<R>` with `new(app)`, `run`, `write`, `typed`, `json`, `ToolError`.

- [ ] **Step 1: Add the dependencies** (versions as VectorEffects' `crates/ve-app/Cargo.toml` lines 50–66 and 94–98; `zip` moves from dev-dependencies to dependencies).
- [ ] **Step 2: Write the failing tests** in `tests/mcp.rs` with the harness ported from VectorEffects' (`tauri::test::mock_app`, a real `AppState` in a `TempRoot`, `server::start(handle, 0, token)` then `McpService::adopt`, an `rmcp` client over reqwest): `a_request_without_the_token_is_refused_before_mcp` (401), `a_wrong_token_is_refused` (401), `a_wrong_host_is_refused`, `a_wrong_origin_is_refused` (non-2xx with the right token), `the_right_token_initialises_and_lists_tools` (server name `PolarExplorer`, the tool `polarexplorer_guide` listed), `off_means_no_socket_and_stop_releases_the_port` (`running_port()` is `None` before and after; the port rebinds), `mcp_set_issues_a_token_on_enable_and_clears_it_on_disable` (43 characters, `bound_port` set, then empty), `a_taken_port_is_reported_and_the_setting_stays_on` (bind a std listener first; `status.bind_error` is `Some`, `enabled` true), `port_zero_is_refused_by_the_command`. In `settings.rs`: `a_settings_file_without_mcp_loads_with_it_off`.
- [ ] **Step 3: Run them** — `cargo test -p pe-app --test mcp`: fails to compile (no `mcp` module).
- [ ] **Step 4: Implement** — port `token.rs`, `server.rs`, `mod.rs`; `events.rs` with `CHANGED`, `ACTIVITY`, `STAGE = "view://stage"`, `BOAT = "view://boat"`, `SELECTION = "view://selection"`, `DocumentChanged { project, opened }`, `McpActivity { sessions, last_tool }` and `changed(app, opened)`; `tools/mod.rs` (handler, `ToolError`, `run`, `write`, `typed`, `json`, `get_info` with instructions under 2,048 characters, session counting in `on_initialized`/`Drop`); `tools/guide.rs` with `polarexplorer_guide`. `McpSettings` read field by field in `settings::load`; `lib.rs` manages `McpService` and calls `apply` at the end of `setup`.
- [ ] **Step 5: Run** `cargo test -p pe-app --test mcp` and `cargo test -p pe-app --lib settings token` — all pass. `cargo tree` check from Global Constraints.

### Task 2: Project, boats and history tools

**Files:** Create `tools/{project,boats,history}.rs`; modify `tools/mod.rs`; test `tests/mcp.rs`.

**Interfaces:**
- Consumes: `run`, `write`, `json`, `ToolError`.
- Produces tools → commands: `project_status` → `projects::project_summary(state, None)`; `project_new {name, boat_name?, boat_notes?, discard_unsaved}` → `projects::new_project`; `project_open {path, discard_unsaved}` → `open_project`; `project_save {path?}` → `save_project` / `save_project_as`; `project_close {discard_unsaved}` → `close_project`; `recent_projects` → `recent_projects`; `boats_list` → `boats::boat_tabs`; `boat_add {name}` → `add_boat`; `boat_rename {boat, name}` → `rename_boat`; `boat_remove {boat}` → `delete_boat(project_id, boat_id)`; `boat_restore {boat}` → `restore_boat`; `undo {boat?}` / `redo {boat?}` → `edit::undo` / `edit::redo`. `tools::absolute(path)` resolves `~/` and refuses a relative path.

- [ ] **Step 1: Failing tests**: `a_client_creates_a_project_and_the_app_sees_it` (the tool's summary equals `projects::summary`), `unsaved_work_is_refused_without_discard` (tool result `is_error`, text contains the interface's message), `a_project_tool_with_nothing_open_is_refused_in_words` (Review Focus 3), `a_project_saves_and_reopens_through_the_tools`, `boats_are_added_renamed_removed_and_restored`, `undo_through_a_tool_restores_the_document_byte_for_byte` (compare `pe_core::io::to_bytes` before the edit and after `undo`), `a_write_tool_emits_document_changed_once_and_a_read_tool_emits_nothing`, `a_relative_path_is_refused`.
- [ ] **Step 2: Run** — fail (unknown tool).
- [ ] **Step 3: Implement** the three files and add their routers to `PolarExplorer::new`.
- [ ] **Step 4: Run** — pass.

### Task 3: Source tools

**Files:** Create `tools/sources.rs`; test `tests/mcp.rs`.

**Interfaces — tools → commands:** `sources_list {boat?}` → `project_summary(state, boat)`'s `sources` and `blend`; `source_set {boat?, source, label?, colour?, visible?, weight?}` → `edit::set_source_label` / `set_source_colour` / `set_source_visible` / `set_source_weight(.., None)`, one command per field present, in that order; `source_move {boat?, source, to}` → `move_source`; `source_remove {boat?, source}` → `remove_source`; `orc_search {boat?, query, filters?, limit?, offset?}` → `orc::orc_search` (limit default 20, at most 100); `orc_add {boat?, id, allow_duplicate?}` → `orc::orc_add`; `orr_search`, `orr_add` likewise; `polar_files_import {boat?, paths}` → `polar_files::import_polar_files`.

- [ ] **Step 1: Failing tests**: `sources_round_trip_through_the_interface_reads` (import `polar_examples` file by `polar_files_import`, list, set weight 0.4 and colour, read back through `projects::summary`), `a_weight_outside_zero_to_one_is_refused` (1.5 → `is_error`), `source_set_applies_each_field_as_its_own_undo_entry`, `an_unknown_source_is_refused_in_words` (Review Focus 4), `orc_search_finds_and_adds_a_certificate` (the embedded catalogue; a query known to `crates/pe-app/tests/orc.rs`).
- [ ] **Steps 2–4:** run (fail), implement, run (pass).

### Task 4: Track and weather tools

**Files:** Create `tools/{tracks,weather}.rs`; modify `crates/pe-app/src/{env.rs,trackers.rs}` (the commands that take `tauri::AppHandle` become generic over `R: tauri::Runtime`); test `tests/mcp.rs`.

**Interfaces — tools → commands:** `track_files_inspect {paths}` → `tracks::inspect_track_files`; `track_files_import {boat?, files}` → `import_track_files` (`files` read with `typed::<Vec<TrackFileRequest>>`); `tracker_event {tracker, url, refresh?}` → `trackers::tracker_event`; `tracker_import {boat?, tracker, key, boats}` → `import_tracker_boats`; `track_set {boat?, source, filters?, max_gap_s?, prefer?, downloaded_wind_only?}` → `set_track_filters` / `set_track_derivation` / `set_track_wind`; `sample_get {boat?, source, sample}` → `sample_details`; `samples_exclude {boat?, samples, excluded}` → `polar3d::set_excluded(nodes: [], samples, excluded)`; `weather_estimate {boat?, sources, restart?}` → `env::env_estimate`; `weather_fetch {boat?, sources, interval?, restart?}` → `env::start_env_fetch`, then polls `env::env_jobs` once a second, forwarding each change as an MCP progress notification, until no job for those sources is queued or running, and returns the final status; `weather_cancel {boat?, sources?}` → `cancel_env_fetch`; `weather_jobs {boat?}` → `env_jobs`.

- [ ] **Step 1: Failing tests** (fixtures only: `tools/webdriver/fixtures/supplied-wind.csv`, and the recorded-archive harness of `tests/env_jobs.rs`): `a_track_file_is_inspected_imported_and_filtered`, `a_filter_sent_as_a_string_is_read` (Review Focus 1), `samples_are_excluded_and_included_through_the_tools`, `weather_estimate_answers_without_fetching`, `a_dropped_weather_call_leaves_no_listener_and_no_session` (Review Focus 5: drop the client mid-call; `status.sessions` returns to 0).
- [ ] **Steps 2–4:** run (fail), implement, run (pass). `cargo test -p pe-app` whole crate still passes after the generic change.

### Task 5: Blend, compare and export tools

**Files:** Create `tools/{blend,export}.rs`; test `tests/mcp.rs`.

**Interfaces — tools → commands:** `blend_status {boat?}` → `project_summary`'s `blend`; `blend_set {boat?, settings}` → `blend::set_blend_settings` (`typed::<BlendSettingsInput>`); `blend_filters_set {boat?, global?, wave_ranges?, priority_groups?, priority_minimum?}` → `set_global_filters` / `set_wave_ranges` / `set_priority_filters`; `polar_read {boat?, source?}` → `polar_edit::polar_edit_surface(source or 0 for the blend)`; `blend_cell {boat?, twa, tws}` (values, not indices: the tool finds the output-grid cell and refuses a value not on the grid, naming the axis) → `blend::blend_cell`; `polar_edit {boat?, source?, op, cells, gesture?}` → `polar_edit::edit_polar`; `compare {boat?, a, b, threshold_kn?}` → `compare::compare_in` through the session, returned as JSON (axes, A, B, Δ kn, Δ %, classes, regions, summary); `export_preview {boat?, format, axes?}` → `blend::export_preview`; `export_polar {boat?, path, format, axes?}` → `export_polar`; `export_all {directory, format}` → `boats::export_all_polars`.

- [ ] **Step 1: Failing tests**: `blend_cell_names_its_sources_through_the_tool` (the fixture of `tests/blend_cell.rs`: 7.524 kn, shares 0.6 and 0.4), `a_blend_cell_off_the_grid_is_refused_naming_the_axis`, `blend_set_changes_the_interpolation_and_undoes`, `polar_read_gives_the_blend_and_a_source`, `compare_reports_the_difference_as_json`, `export_writes_the_bytes_the_interface_writes` (the tool's file equals `blend::export_polar`'s for the same project), `an_export_needs_an_absolute_path`.
- [ ] **Steps 2–4:** run (fail), implement, run (pass).

### Task 6: View tools, screenshot, and the view following

**Files:** Create `tools/view.rs`, `mcp/capture.rs` (port); modify `lib.rs` (`deliver_capture`, `refuse_capture` commands); create `ui/src/mcp/follow.ts` (+ test); modify `ui/src/App.tsx`, `ui/src/ipc.ts`, the stage components' capture hooks; test `tests/mcp.rs`.

**Interfaces:** tools `screenshot` (emits `view://capture {id}`; waits 35 s for `deliver_capture(id, bytes)` / `refuse_capture(id, reason)`; refuses at once with no project), `view_stage {stage: "3d" | "map" | "compare" | "plot"}` (emits `view://stage`), `view_boat {boat}` (`view://boat`), `selection_set {samples}` (`view://selection`). Frontend: `applyDocumentChanged(payload, setters)`; `App.tsx` listens for `document://changed`, `view://stage`, `view://boat`, `view://selection`, `view://capture`, `mcp://activity`.

- [ ] **Step 1: Failing tests** (Rust): `view_tools_emit_events_for_the_frontend`, `a_screenshot_is_delivered_by_the_frontend`, `a_screenshot_with_no_project_is_refused_before_asking`, `a_declined_screenshot_returns_the_reason`. (TS) `follow.test.ts`: an edit replaces the project only; a new project resets stage and selection; `null` shows the start screen.
- [ ] **Steps 2–4:** run (fail), implement, run (pass); `npm run ui:test`.

### Task 7: The `invoke` escape hatch

**Files:** Create `mcp/invoke.rs`, `tools/escape.rs`; test in `invoke.rs` and `tests/mcp.rs`.

**Interfaces:** `invoke::call<R>(app, name, args: Value) -> Result<Value>` through `TABLE: &[(&str, Handler)]` with `deny_unknown_fields` argument structs; `invoke::EXCLUDED: &[&str]`; `invoke::REGISTERED: &[&str]`, the names in `lib.rs`'s `generate_handler!`, kept beside it.

- [ ] **Step 1: Failing tests**: `the_table_covers_every_registered_command` (`TABLE ∪ EXCLUDED == REGISTERED`, and `REGISTERED` equals the names parsed from `lib.rs`'s `generate_handler!` source), `invoke_reaches_a_command`, `an_unknown_argument_is_refused`, `empty_arguments_are_accepted`, `an_excluded_command_is_refused_by_name`.
- [ ] **Steps 2–4:** run (fail), implement, run (pass).

### Task 8: Client registration

**Files:** Create `mcp/{clients,desktop,skill}.rs`, `mcp/bridge.js` (ports); modify `settings.rs` (`mcp_register_client`); tests in each file and `tests/mcp.rs`.

**Interfaces:** `clients::McpClient { ClaudeCode, Codex, ClaudeDesktop }` with `available()`; `mcp_register_client(client) -> McpRegistered { skill: Option<String> }`; `skill::{NAME, text(), install(dir)}`; `desktop::{BUNDLE_NAME, write_bundle(dir, settings_file)}`.

- [ ] **Step 1: Failing tests** (ports of VectorEffects' with the renames): Codex config created, preserved, idempotent, refused when invalid or when the folder is missing, 0600 with no staging file; a fake `claude` script records the remove and add arguments at user scope; a refusal is reported without the token; skill front matter valid and body equal to the guide; `the_desktop_bundle_is_a_valid_extension_with_no_secret_in_it`; `the_bridge_follows_the_application_through_a_restart_and_off_and_on` (skipped without `node`).
- [ ] **Steps 2–4:** run (fail), implement, run (pass).

### Task 9: Settings section, status badge, help and translations

**Files:** Create `ui/src/settings/McpSection.tsx` (+ `.test.tsx`, ported); modify `ui/src/settings/SettingsDialog.tsx`, `ui/src/ipc.ts`, `ui/src/help/features/settings.ts`, `ui/src/help/topics.ts` and `locales/{fr,de}.ts`, `ui/src/i18n/locales/{fr,de}/settings.ts` and `shell.ts`; `npm run bindings`.

- [ ] **Step 1: Failing tests** (`McpSection.test.tsx`): no URL while off; status fetched once; rotate; no client buttons while off; add, then update after the token changes; Desktop says "Opened", never update; platform-filtered clients; a failure is not claimed as added; the line about ChatGPT is shown.
- [ ] **Steps 2–4:** run (fail), implement (controls and `data-feature` ids as the spec's §5 table), run `npm run ui:typecheck && npm run ui:test`.

### Task 10: Rules, documents and the end-to-end check

**Files:** Modify `CLAUDE.md` (invariant 4's inbound exception, the recipe *Adding an MCP tool*, the `bridge.js` gotcha), `spec.md` (§1.5, §3.4, new §3.7), `plan.md` (M21 complete, validation numbers), `docs/USER-GUIDE.md`, `tools/check-offline.sh` (a comment: the listener is enforced by `off_means_no_socket_and_stop_releases_the_port`, not here); create `tools/webdriver/ux/16-mcp-follow.test.mjs`.

- [ ] **Step 1:** The UX test: start the app with an isolated data root, turn the service on in Settings, read the URL and token from the section, connect with `@modelcontextprotocol/sdk`'s Streamable HTTP client (a dev dependency already), create a project and import a polar file through tools, set the source's weight to 0.4, and assert the slider in the interface reads 0.40 and the status bar shows the badge; screenshots of the Settings section and the followed view.
- [ ] **Step 2:** Run every check in CLAUDE.md's *Commands* (fmt, clippy, `cargo test --workspace`, `ui:typecheck`, `ui:test`, `ui:perf`, `check:offline`, `tools:test`) and `npm run ux -- mcp`; open the screenshots.
- [ ] **Step 3:** Update the documents with the real numbers.
