# TASK: build `turbohermes-jcode` — the jcode adapter

## Authority

Work order v7 section 9: "Phase 3 jcode integration." Kanban ticket
`t_33ad8c8c`. The architecture document, section 3: "It delegates coding
execution to jcode through a narrow adapter: start, resume, inspect and stop
sessions; create a swarm from a task-DAG node; pass scoped task specification,
repository path, branch or worktree, budget and permitted tools; inject context;
collect typed artifacts; map jcode identities to TurboHermes identities;
reconcile after process failure. Process isolation protects release cadence and
makes upstream rebasing easier."

Reuse before write (work order v7 section 1). Every module names the upstream
file it was copied or ported from in `../../REUSE_MAP.md` and in its own module
doc comment.

## What the narrowest stable interface actually is (verified live 2026-11-10)

Not `jcode run` (a CLI wrapper that streams text and exits). The stable
interface is the **harness API bridge**: `jcode api-bridge --api-socket <path>`
serves NDJSON frames on a Unix socket, with a versioned handshake and a typed
request/event surface. Verified on this host:

- `hello` (min_version=1, max_version=1) replies `hello_ok` with 20 capability
  strings, `server: jcode-harness-api-bridge/0.1.0`.
- `create_session {working_dir}` replies `attached {session: {session_id, status}}`
  and moves the socket's cwd into `~/.jcode/scratch/<name>`.
- `attach_session` replies `attached`, or `error unknown_session` with
  "Failed to restore session" when the session was never persisted.
- `configure_tools {enabled:[...]}` replies `ok`; `list_tools` then returns
  exactly the allow-list (measured: 27 tools unrestricted -> 6 with the
  narrow allow-list).
- `send_message` does **not** reply with a frame. It emits the streaming events
  `message_accepted`, `connection_phase`, `reasoning_delta`, `reasoning_done`,
  `tool_start`, `tool_input_delta`, `tool_exec`, `tool_done`, `text_delta`,
  `text_done`, `token_usage`, `runtime_info`, then `turn_done`. A client that
  waits for `reply_to` on `send_message` blocks forever.
- `cancel` replies `ok` and then emits `turn_stopped {reason: "interrupted"}`.
- `get_history` replies `History {messages}` with roles `user`/`assistant`.
- `peek_session` replies `History` for any session, attached or not.
- `runtime_command {command:"info"}` replies `runtime_command_result` with
  structured data including `context_window`.
- Two `send_message` calls in one turn: the second returns
  `error` (a turn is already active) and the first completes normally. This is
  the duplicate-dispatch boundary the integration test must pin.

The bridge bridges to the internal daemon socket (`/run/user/1001/jcode.sock`).
It dies when its parent shell dies, so the adapter owns its lifecycle: it starts
the bridge, waits for the socket to appear, and reaps it.

## Goal

A Rust crate `turbohermes-jcode` providing the jcode side of the adapter:

1. **Transport and frames**: NDJSON over a Unix socket, version handshake,
   request id allocation, event stream demultiplexing. Ported from jcode's own
   client so the framing is upstream's, not ours.
2. **`JcodeClient`**: the narrow typed surface — `create`, `attach`, `list`,
   `peek`, `configure_tools`, `send`, `cancel`, `history`, `runtime_info`,
   `ping`. Nothing else. A request that is not in the architecture document's
   list is not in the client.
3. **`TaskSpec`**: task id, acceptance criteria, budget, allowed operations,
   repository path, branch/worktree, context to inject. Ported from BlockCell's
   `TaskContract` + `BudgetConfig`, not re-invented.
4. **`Scope`**: controlled repository/worktree scope. A worktree is created from
   the repo HEAD with `git worktree add -b`, and removed afterwards. Ported from
   jcode's own `worktrees.rs`.
5. **Lifecycle capture**: every `ApiEvent` observed during a turn is mapped onto
   a `turbohermes_core::AgentEvent` and appended to the event store. jcode
   identities (`session_id`, `tool call_id`) are carried in the event's
   `session_id` and extension map, so jcode identities map to TurboHermes
   identities without either runtime changing.
6. **Typed artifacts**: the completion report, tool outputs (hashed), token
   usage, and the final history, collected as one typed envelope per task.
7. **Reconciliation**: a claim ledger in SQLite. A dispatch writes a claim
   (task id, session id, pid, lease expiry). On timeout, crash or restart the
   reconciler decides: still running, orphaned (reap), or finished (collect).
   Claim-and-status logic ported from beads' `claim.go`, not rewritten.

## Constraints

- Rust edition 2021, MIT OR Apache-2.0. Dependencies: serde, serde_json,
  rusqlite (bundled), uuid, chrono, thiserror, and the path dependency
  `turbohermes-core`. Cargo may fetch crates from crates.io; nothing else is
  installed or downloaded.
- No network calls. The transport is a Unix socket to a bridge started on this
  host. No LLM calls from this crate: the model runs inside the jcode daemon
  the adapter supervises.
- The crate must not depend on jcode as a library. jcode is an external
  process; the adapter speaks its wire protocol. That is the "keep jcode and
  Graphicle as external processes initially" rule from the architecture
  document.
- Append-only for the event store: this crate appends `AgentEvent`s through
  `turbohermes_core::EventStore`, which already has no mutation path.
- Secrets are never stored. Tool outputs and messages are SHA-256 hashed by
  `turbohermes_core`; the artifact envelope carries references, not text.

## Module ownership (so parallel workers do not collide)

| File | Owner |
|---|---|
| `src/lib.rs`, `src/spec.rs`, `src/events.rs`, `Cargo.toml`, `TASK.md` | coordinator |
| `src/frames.rs`, `src/transport.rs`, `src/client.rs` | lane `jcode transport` |
| `src/scope.rs` | lane `jcode worktree scope` |
| `src/reconcile.rs`, `src/artifacts.rs` | lane `jcode reconcile` |
| `tests/*.rs` | coordinator (the done-check) |

## The contract each module must satisfy

### `src/frames.rs` (port)
- `write_frame(w, frame)` and `read_frame(r) -> Result<T, FrameError>` with
  `FrameError::{Io, Json, Eof}`, ported verbatim in behaviour from
  `jcode/crates/jcode-harness-api/src/client.rs`.
- `ClientFrame {v, id, request}` and `ServerFrame {reply_to, event}` with
  `#[serde(flatten)]`, ported from
  `jcode/crates/jcode-harness-api/src/lib.rs`.
- Tests: a frame round-trips through an in-memory pipe; a blank line is skipped;
  EOF is a typed error; an unknown event kind deserialises to `Unknown` rather
  than failing.

### `src/transport.rs` (port + glue)
- `Transport::connect(path)` over `std::os::unix::net::UnixStream`, with a
  connect timeout. `send(&ApiRequest) -> u64` allocates ids monotonically.
  `recv() -> ServerFrame` skips `Unknown`. `hello(client)` performs the
  handshake and returns the capability list, refusing a version it cannot
  speak.
- Socket path resolution ported from
  `jcode/crates/jcode-harness-api/src/sockets.rs`: `JCODE_API_SOCKET`, then
  `JCODE_RUNTIME_DIR`, then `XDG_RUNTIME_DIR`, then a temp fallback.
- `Bridge::start(socket_path) -> Bridge` spawns `jcode api-bridge --api-socket
  <path>` with `start_new_session(true)` (so the bridge outlives the shell that
  started it), waits for the socket file to exist and accept a connection, and
  reaps the child on drop. If the socket does not appear within a stated
  timeout, the error says so with the bridge's stderr tail.
- Tests: against a `TcpListener`/pipe stand-in where possible, and one live
  test that starts a real bridge and completes a `hello` + `ping` round trip.
  The live test is skipped when no `jcode` binary is on PATH.

### `src/client.rs` (glue over transport)
- `JcodeClient::connect(socket) -> Result<Self>`: hello, then a `Ping`.
- `create(working_dir) -> SessionId`, `attach(id)`, `list(limit)`,
  `peek(id, limit)`, `configure_tools(id, allow_list)`, `send(id, content)`,
  `cancel(id)`, `history(id)`, `runtime_info(id)`, `ping()`.
- `send` returns `Ok(())` immediately: the reply is the event stream, not a
  frame. The doc comment states this and cites the measured behaviour.
- `next_event(timeout) -> Option<ApiEvent>`: the event pump. The caller owns the
  loop; the client never blocks a turn on a reply that will not come.
- Tests: `send_does_not_wait_for_a_reply_frame` pins the measured behaviour
  against the live bridge when available.

### `src/spec.rs` (port-adapt)
- `TaskSpec { task_id: TaskId, trace_id: TraceId, prompt, acceptance: Vec<
  AcceptanceCriterion>, budget: Budget, allowed_ops: AllowedOps, repo_path,
  branch, worktree: bool, context: Vec<String> }`.
- `AcceptanceCriterion { kind: Command{cmd} | FileExists{path} | Artifact{name},
  required: bool }` and `validate()` — ported from BlockCell's
  `crates/core/src/task_contract.rs` (`Deliverable`, `TaskContract::validate`,
  `resolve_workspace_output`, the 32-item cap, the parent-dir refusal, the
  canonicalize containment check).
- `Budget { max_tokens: u64, max_cost_micro_usd: u64, warning_threshold: f64 }`
  with `0 means unlimited`, ported from BlockCell's
  `crates/core/src/budget.rs` (`BudgetConfig`, `BudgetSnapshot`).
- `AllowedOps { tools: Vec<String>, writes: Vec<String> }`: the tool allow-list
  that becomes `configure_tools {enabled}`, and the write scope. Empty tools
  means the unrestricted default is refused, not silently allowed.
- Tests: validation rejects a parent-dir deliverable, an over-long path, a NUL
  byte, and more than 32 criteria; `resolve_workspace_output` refuses a path
  outside the workspace; a zero budget means unlimited.

### `src/scope.rs` (port)
- `Scope::create(repo, branch) -> Scope` and `Scope::remove()`, ported from
  `jcode/crates/jcode-sdk/src/worktrees.rs`: `git worktree list --porcelain -z`,
  the `<repo>-worktrees/<encoded-branch>` destination root, `safe_name`
  percent-encoding, `git check-ref-format --branch` with the literal-name check,
  `git worktree add -b branch -- path HEAD` (never `-B`), and "errors never
  trigger destructive cleanup".
- `Scope::path()` and `Scope::branch()`.
- Tests, ported from the upstream test module: parsing NUL-separated records
  with embedded newlines, the bare and detached entries, a non-UTF-8 path
  erroring, and creation isolating staged/unstaged/untracked changes.

### `src/events.rs` (glue)
- `EventMapper { trace_id, task_id, agent_id, session_id }` with
- `map(&ApiEvent) -> Vec<AgentEvent>`: `message_accepted` -> `TaskUpdated`,
  `tool_start`/`tool_exec` -> `ToolCalled`, `tool_done` -> `ToolResult` or
  `ToolError`, `token_usage` -> `ResourceUsage`, `reasoning_done` ->
  `AgentStarted`-class progress, `text_done` -> `TaskUpdated`,
  `turn_done` -> `TaskCompleted`, `turn_stopped` -> `TaskCancelled`,
  `error` -> `TaskFailed`. Every mapped event carries the jcode `session_id`
  and, for tool events, the `call_id`, in the event's extension map. Tool
  inputs and outputs are never in the event: `input_hash`/`output_hash` only.
- Tests: each mapped kind produces the right `EventType`, and a tool event
  carries `call_id` while carrying no tool text.

### `src/artifacts.rs` (glue)
- `Artifact { task_id, session_id, kind, reference, sha256, collected_at }` with
  `ArtifactKind::{CompletionReport, History, TokenUsage, ToolOutput,
  RuntimeInfo}`. The report text is stored through the hygiene gate and kept
  only as a hash plus a path under the task's artifact directory; the envelope
  itself is what the supervisor reads.
- `ArtifactCollector::collect(client, session_id, out_dir) -> Vec<Artifact>`.
- Tests: collecting from a stub client yields one artifact per kind, each with
  a sha256 and no secret text.

### `src/reconcile.rs` (port-adapt)
- SQLite claim ledger, append-only in spirit (claims are rows with a lease;
  resolution is a new row, not an edit):
  `claims(task_id, session_id, agent_pid, bridge_pid, lease_expires_at,
  state, attempt)` and `resolutions(task_id, decided_at, verdict, reason)`.
- `ClaimLedger::claim(spec, session_id, pids, lease)` inserts a row; a second
  claim for the same `task_id` + `attempt` is refused (that is the
  duplicate-dispatch guard).
- `reconcile(now) -> Vec<Reconciliation>`: for each unresolved claim, compare
  the lease against `now`, then probe liveness: is the bridge pid alive, does
  `list_sessions` still contain the session, is the session's status terminal?
  Verdicts: `StillRunning`, `Orphaned{reap}`, `Finished{collect}`,
  `Lost{no evidence}`.
- Liveness probing uses `kill(pid, 0)` on the recorded pids, never a
  pattern-based `pgrep` (three recorded self-kill incidents on this host from
  pattern matching, see DECISIONS D-102).
- The claim-and-status logic is ported from beads `claim.go`; the lease and
  heartbeat shape follows the architecture document's reconciliation
  requirement.
- Tests: an expired lease with a dead pid resolves `Orphaned`; a live pid with
  an active session resolves `StillRunning`; a duplicate claim is refused; a
  restart (new ledger handle over the same file) re-reads unresolved claims and
  resolves them.

## Done-check

One command, must exit 0:

```
cargo test --manifest-path turbohermes-jcode/Cargo.toml
```

plus the CLI round-trips, also must exit 0:

```
turbohermes-jcode probe --socket <path>     # hello + ping + list_sessions, prints JSON
turbohermes-jcode run --repo <path> --task <id> --prompt <text>   # full supervised turn
turbohermes-jcode reconcile --db <path>     # prints the verdict for every open claim
```

Required integration tests (name them):
- `integration_cancelling_a_live_turn_stops_the_session_and_records_the_cancel`
- `integration_restarting_the_supervisor_reconciles_the_open_claim`
- `integration_duplicate_dispatch_of_the_same_task_is_refused`
- `integration_a_worktree_scoped_task_writes_only_inside_the_worktree`
- `integration_lifecycle_events_land_in_the_event_store_in_order`

## Reporting discipline

Report only what you ran and observed. If the live bridge is unavailable, the
live tests must skip with a stated reason rather than fail, and the report must
say which tests skipped and why. Do not claim tests pass that you did not run.
