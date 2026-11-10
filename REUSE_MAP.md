# REUSE_MAP — one row per requirement

TurboHermes implementation order, section 4 step 1. Authority: the owner's
`TurboHermes_Implementation_Order_v2.md`. Rule: "Reuse before write."

For each row: source project and file path, action (use / fork / copy / new), and
for "new" the reason existing code does not fit. Nothing below waits for approval;
the four stop cases in section 5 of the order have not triggered.

## Pinned upstream commits and licences

| Project | Commit | Licence | Role |
|---|---|---|---|
| blockcell-labs/blockcell | `5ca38be861b4a3e8f7a1e8d332e242b42ed25710` | MIT | **FORKED** into this repo |
| jcode | `47e426e5ffe77913e3bf42276876a30aef018c79` | MIT | coding subsystem, launched and supervised, never merged |
| graphicle | `ddf11b1dac674b4a854356bda8ab67c0d5ced239` | Apache-2.0 | code intelligence, consumed as a release binary, never built from source |

Third-party notice obligation: MIT requires the copyright and permission notice be
included in all copies. Apache-2.0 requires preservation of licence, notice and
attribution files, and stating changes. Both are satisfied by keeping the upstream
`LICENSE` and any `NOTICE` files in place and adding a `THIRD-PARTY.md` recording
what was copied, from where, and at which commit. No copyleft dependency is
present in any of the three.

## Phase 1 — Fork BlockCell and establish TurboHermes identity

| # | Requirement (document's words) | Source | Action | Note |
|---|---|---|---|---|
| 1.1 | "Fork `blockcell-labs/blockcell`" | `m3m007/turbohermes`, parent verified | **DONE** | fork exists, pinned commit, compiles in 5m37s |
| 1.2 | "Rename package, binary, config paths and user-facing branding systematically" | in-repo | **fork + modify** | 11 `blockcell*` package names, ~5,100 mentions across tracked files. Mechanical only; no behaviour change rides along |
| 1.3 | "Keep upstream history and define a regular upstream merge/rebase process" | in-repo docs | **fork + modify** | `docs/architecture/UPSTREAM-SYNC.md`, commands and cadence. No rebase, no squash: rename commits on top of the fork point |
| 1.4 | "Preserve existing BlockCell functionality initially" | constraint on 1.2 | **new: test** | baseline test count is the pass mark. Not yet captured; first action in Phase 1 |
| 1.5 | "Add TurboHermes-specific integration crates only where needed" | — | **new, only if needed** | default answer is none. A capability BlockCell already implements gets no crate |
| 1.6 | "Add CI for formatting, clippy, unit tests, integration tests and licence checks" | standard + existing config | **copy in** | `cargo-deny` config already exists at `/home/hermes/projects/turbohermes/deny.toml`; copy it rather than write a new one |

## Phase 2 — Add jcode as a supervised coding subsystem

| # | Requirement | Source | Action | Note |
|---|---|---|---|---|
| 2.1 | "Implement the jcode adapter using the narrowest stable interface" | — | **new** | Listed in section 3 as a thin adapter. Reason new: no upstream project exposes jcode's process surface as a library |
| 2.2 | "Launch jcode in a controlled repository/worktree scope" | jcode worktree support | **use as is** | jcode already has optional worktrees and file-touch notifications; call them, do not rebuild them |
| 2.3 | "Pass task IDs, acceptance criteria, budgets and allowed operations" | BlockCell `crates/core/src/{budget,task_contract,scope}.rs` | **copy in** | BlockCell already models budgets, task contracts and scope. Wrap those types |
| 2.4 | "Capture worker and swarm lifecycle events" | jcode event surface | **use as is** | read jcode's lifecycle events; add only the trace identifier |
| 2.5 | "Collect typed task artifacts and evidence" | — | **new** | Telemetry, listed in section 3 |
| 2.6 | "Reconcile status after timeouts, crashes and supervisor restarts" | — | **new** | part of the adapter in 2.1 |
| 2.7 | "Add integration tests for cancellation, restart and duplicate dispatch" | — | **new** | integration tests are explicitly required, and are not upstream tests |

## Phase 3 — Add Graphicle as code intelligence

| # | Requirement | Source | Action | Note |
|---|---|---|---|---|
| 3.1 | "Start Graphicle for a repository/workspace" | graphicle release binary | **use as is** | `graphicle-<ver>-x86_64-unknown-linux-gnu.tar.gz`, checksum-verified. Never built from source |
| 3.2 | "Run code-only indexing first" | graphicle's own indexer | **use as is** | |
| 3.3 | "Expose its MCP tools to jcode and TurboHermes" | graphicle `serve.rs`, 10 tools | **use as is** | `query_graph`, `get_node`, `get_neighbors`, `get_community`, `god_nodes`, `graph_stats`, `shortest_path`, `list_prs`, `get_pr_impact`, `triage_prs` |
| 3.4 | "Build a typed adapter for code queries and cited context" | — | **new** | Listed in section 3 as a thin adapter |
| 3.5 | "Preserve file/line/symbol references in task artifacts" | — | **new** | Telemetry, section 3 |
| 3.6 | "Test incremental sync after edits, branch changes and merges" | — | **new** | integration tests |
| 3.7 | "Add staleness detection for memories linked to changed symbols" | graphicle `_built_at_commit` + `build_commit_line()` | **copy in** | graphicle already solves this: it records the build commit and compares to git HEAD because an MCP client cannot stat graph.json. Copy the pattern |

## Phase 4+ components that section 3 makes new

| Component | Source primitives to check first | Action |
|---|---|---|
| Memory service | SQLite + FTS5, rusqlite, tree-sitter, jcode's memory code, BlockCell `crates/storage` and `ghost_ledger.rs`, existing BM25 crates | **new** with bridging |
| Telemetry (event log, trace ids, outcome collector) | BlockCell hash-chain audit log in `crates/core` | **new** |
| DecisionProvider (Jev, Clef, Clef-Flash, deterministic) | Clef on Cloudflare Workers AI, LiteLLM `/v1/decisions` and `/v1/systemone` | **new** interface, **use as is** backends |
| TrajectoryCritic + OpenHands Critic adapter | OpenHands SDK critic on :8079 | **new** interface |
| Evaluation corpus tooling | — | **new** |

## What is deliberately NOT built

- No reimplementation of BlockCell's tool registry, policy surface, skills layer,
  storage or gateway. Forked and wrapped instead.
- No reimplementation of jcode's swarm, task DAG or self-development. Supervised
  instead.
- No reimplementation of graphicle's code graph. "Do not duplicate Graphicle's
  code graph in the TurboHermes database."
- No new secret scanner. `gitleaks`, `ripsecrets` and `kingfisher` are installed
  and working.
- No new decision engine. Clef is hosted and reachable on the verified account.
- No local embeddings. API-only decision layer, per the owner's standing ruling.
