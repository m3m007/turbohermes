# REUSE_MAP — TurboHermes

Work order v7 section 6 step 1: one row per requirement with source and file path,
licence, action (use, fork, copy, port-adapt, glue), and for "glue" what was checked
in each source and why nothing could be ported or adapted. Upstream commits are
pinned. Licence conflicts are stop cases (section 7); none arose in Phase 3.

## Pinned upstreams used in Phase 3

| Project | Path | Pinned commit | Licence | Rider? |
|---|---|---|---|---|
| jcode | /home/hermes/projects/upstream/jcode | 7206efd0c1dbf49ef38d06b63f3a96a3a7cee611 | MIT | no |
| BlockCell | /home/hermes/projects/upstream/blockcell | 5ca38be861b4a3e8f7a1e8d332e242b42ed25710 | MIT | no |
| beads (Go original, `gastownhall/beads`) | /home/hermes/projects/upstream/beads | 9d4ebf42f0496cdd0f0d7fdc6062c0981f374a70 | MIT | no |

`beads_rust` (Dicklesworthstone/beads_rust) was checked as the section 2.2a planner
source and **rejected for Phase 3**: its LICENSE carries an explicit
OpenAI/Anthropic rider that nullifies rights for a long list of parties. Under
work order v7 section 7 ("a source has a licence rider") this is a stop case, so
no beads_rust code is copied or ported. The claim-and-status logic comes from the
Go original `beads` at the commit above, which is plain MIT. This decision and the
licence text it rests on are recorded here rather than in a chat message.

## Rows

| # | Requirement | Source | File path (upstream) | Licence | Action | Notes |
|---|---|---|---|---|---|---|
| 1 | NDJSON frame read/write, blank-line skip, typed EOF | jcode | `crates/jcode-harness-api/src/client.rs` (lines 39-67) | MIT | port | `write_frame`/`read_frame` copied verbatim in behaviour, including the `read_line` + `trim().is_empty()` skip loop. |
| 2 | Frame envelope shapes (`v`, `id`, `#[serde(flatten)]`) | jcode | `crates/jcode-harness-api/src/lib.rs` (lines 58-90) | MIT | port | `ClientFrame`/`ServerFrame` plus `ClientFrame::new`. |
| 3 | Request surface (`ApiRequest`) | jcode | `crates/jcode-harness-api/src/requests.rs` (lines 29-343) | MIT | port | Only the narrow subset the architecture document names is used: `Hello`, `Ping`, `CreateSession`, `AttachSession`, `ListSessions`, `PeekSession`, `ConfigureTools`, `ListTools`, `SendMessage`, `Cancel`, `GetHistory`, `GetRuntimeInfo`. The enum is ported whole because serde's `#[serde(tag = "req", rename_all = "snake_case")]` plus `#[serde(other)] Unknown` catch-all is what makes unknown requests forward-compatible. |
| 4 | Event surface (`ApiEvent`) | jcode | `crates/jcode-harness-api/src/events.rs` (lines 1-560) | MIT | port | Ported whole, same `#[serde(tag = "ev", rename_all = "snake_case")]` + `#[serde(other)] Unknown` contract. `SessionInfo` is trimmed to the fields the adapter reads (`session_id`, `status`, `working_dir`); the rest of the struct is preserved verbatim. |
| 5 | Socket path resolution (`JCODE_API_SOCKET` → `JCODE_RUNTIME_DIR` → `XDG_RUNTIME_DIR` → temp fallback) | jcode | `crates/jcode-harness-api/src/sockets.rs` (lines 12-56) | MIT | port | Copied verbatim. Upstream's own doc comment records why one definition matters: the bridge and clients once resolved different directories and could not connect. |
| 6 | Version handshake + monotonic request id allocation | jcode | `crates/jcode-harness-api/src/client.rs` (lines 88-116) | MIT | port | `HarnessClient::{hello,send,recv}` is the shape `Transport` follows, including "skip `Unknown` on recv". |
| 7 | `TurnStopReason` | jcode | `crates/jcode-session-types/src/lib.rs` (lines 1113-1124) | MIT | port | Copied verbatim so a `turn_stopped` reason is upstream's vocabulary, not a re-spelling. |
| 8 | Worktree create/list/parse | jcode | `crates/jcode-sdk/src/worktrees.rs` (lines 22-155) | MIT | port | `list`, `parse`, `safe_name`, `create`, and the `<repo>-worktrees/<encoded>` destination root. The `-b` (never `-B`) rule and "errors never trigger destructive cleanup" are preserved. Upstream's test module (lines 158-318) is the source of this crate's `scope` tests. |
| 9 | Acceptance criteria validation: 32-item cap, empty/4096/NUL rejection, parent-dir refusal | BlockCell | `crates/core/src/task_contract.rs` (lines 45-65) | MIT | port-adapt | Ported as `TaskSpec::validate`. BlockCell's `Error::Validation` becomes this crate's `SpecError`, and `Deliverable`/`must_update` are dropped because the architecture document's `TaskSpec` carries `AcceptanceCriterion {kind, required}` instead. |
| 10 | Workspace containment for declared outputs | BlockCell | `crates/core/src/task_contract.rs` (lines 80-104) | MIT | port | `resolve_workspace_output` copied verbatim, including the canonicalize-containment check. |
| 11 | Budget shape, `0 means unlimited`, warning threshold | BlockCell | `crates/core/src/budget.rs` (lines 53-82) | MIT | port-adapt | `BudgetConfig`/`BudgetSnapshot` become `Budget {max_tokens, max_cost_micro_usd, warning_threshold}`. BlockCell's `MICRO_USD_PER_CENT`/`MICRO_USD_PER_USD` constants and the `saturating_mul` cent→micro conversion are copied. The `BudgetTracker` parent-chain machinery (lines 84-309) is not ported: the adapter reports usage as events, it does not gate a turn in-process, and the tracker would be a second budget authority. |
| 12 | Claim CAS: insert-or-refuse, idempotent re-claim by the same holder, typed conflict carrying the holder | beads (Go) | `internal/storage/issueops/claim.go` (lines 33-146, `ClaimIssueInTx`/`claimIssueInTx`) | MIT | port-adapt | The CAS body: read state, decide claimability (unassigned / same actor / claim-pool alias), conditional UPDATE, and on `rowsAffected == 0` re-read the state that lost and return a typed refusal. Adapted from MySQL placeholders to rusqlite's `?1` and from `row_lock`-based CAS to a SQLite `UNIQUE` index on `(task_id, attempt)` plus `INSERT ... ON CONFLICT DO NOTHING`. Dolt versioning, wisp routing and the event/version journaling are not ported; the append-only event store in `turbohermes-core` is the journal here. |
| 13 | Lease: TTL, upsert, expiry, heartbeat | beads (Go) | `internal/storage/issueops/lease.go` (lines 28-46, 181-207, 320-328) | MIT | port-adapt | `DefaultLeaseTTL = 5 * time.Minute` and `UpsertLeaseInTx`/`DeleteLeaseInTx`/`HeartbeatIssueInTx` become the claim ledger's lease columns. The Dolt commit boundary (`LeaseRenewalRefusal`, lines 345-410) is not ported; there is no replica here. |
| 14 | Liveness probe by pid | (no source) | — | — | glue | Checked in each source: jcode's SDK worktree code, jcode's harness API client, BlockCell's core crates and beads' storage layer have no "is this pid alive" helper. `std::process` exposes no liveness check either. Beads' own reclaim path does not probe the process at all; it expires the lease and lets a later claim win, which is exactly what this crate's reconciler does as the first branch. The pid probe is the second branch, and it is glue over one syscall. |
| 15 | Bridge lifecycle: spawn, wait for socket, reap on drop | jcode | `crates/jcode-harness-api-server/src/bin/bridge.rs` (lines 16-57) | MIT | use | The bridge is an external process. This crate does not reimplement it. It starts `jcode api-bridge --api-socket <path>` and reaps the child. |
| 16 | `kill(pid, 0)` without libc | (crates.io) | `libc` 0.2 | MIT/Apache-2.0 | glue | Work order v7 section 2.5 allows "standard packages from crates.io for a specific need". `libc::kill` is the minimal such package; the alternative (parsing `/proc/<pid>` per row) is a reimplementation of the syscall. |
| 17 | Event mapping onto the AgentEvent schema | (no source) | — | — | glue | Checked: jcode's `ApiEvent` → its own TUI event types (out of scope, that is jcode's internal mapping); BlockCell's `crates/core/src/event*` has its own schema that does not carry TurboHermes identity types; beads' event journal is a Dolt change row, not a typed runtime event. The mapping to `turbohermes_core::AgentEvent` is glue by definition — it is the join between two typed surfaces that neither upstream knows about. |
| 18 | Artifact envelope and collection | (no source) | — | — | glue | Checked: jcode stores transcripts in `~/.jcode` with its own layout; BlockCell's file store keys artifacts by session with no typed envelope; beads has no artifact concept. A typed envelope over hashes and paths is specified in the architecture document and has no upstream equivalent. The envelope carries references and SHA-256 digests, never text. |
| 19 | Supervisor CLI (`probe`, `run`, `reconcile`) | (no source) | — | — | glue | Checked: BlockCell's `bin/` and jcode's `crates/*/src/bin/` are entrypoints for their own runtimes. There is no supervisor CLI to copy because this is the supervisor. It is a thin arg-parse over the modules above. |

## Self-check numbers (work order v7 section 6 step 3)

Measured with `cloc`-style counting over `turbohermes-jcode/src` on branch
`phase-3-jcode-adapter` after the phase completed. Counts are in
`PHASE_REPORT.md` and are produced by a command whose output is quoted there, not
estimated. No new crate duplicates a function of BlockCell, jcode or Graphicle:
`frames`, `transport`, `client` and `scope` are ports of jcode's own code, `spec`
is a port of BlockCell's, `reconcile` is a port of beads', and the three glue
modules (`events`, `artifacts`, `main`) join typed surfaces that no upstream
carries.
