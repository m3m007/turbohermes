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

## Approved sources added by implementation order v3

Order v3 supersedes v2 and adds four approved copy sources, overriding the
architecture document's wording that they are "design comparison, not proposed
dependencies". All four inspected at HEAD on 2026-11-10.

| Source | Commit | Licence | Language | What it holds that we need |
|---|---|---|---|---|
| `moltis-org/moltis` | cloned `--depth 1` | MIT | **Rust** | `crates/memory` (FTS5 chunk store, embeddings, reranking, search, watcher, writer, splitter), `crates/memory-zvec`, `crates/code-index`, `crates/skills`, `crates/sessions`, `crates/observability` |
| `steveyegge/beads` | cloned `--depth 1` | MIT | Go | Task and state tracking with a claim mechanism. Status machine: `open`, `in_progress`, `blocked`, `closed`. `claim.go` is the interesting file |
| `vectorize-io/hindsight` | cloned `--depth 1` | MIT | TypeScript | Memory conflict resolution by temporal evolution. `hindsight-docs/blog/2026-02-09-resolving-memory-conflicts.md` is the design doc for supersession |
| `sanonone/kektordb` | cloned `--depth 1` | Apache-2.0 | Go | Vector and retrieval storage. Note: the URL in order v3 (`kektordb/kektor`) does not exist; the real repository is `sanonone/kektordb` |

**Licence check, per the order's rule:** all four are permissive (three MIT, one
Apache-2.0). No copyleft, no conflict with the MIT/ Apache-2.0 targets. Apache-2.0
requires stating changes; MIT requires the notice. Both satisfied by `THIRD-PARTY.md`
plus keeping upstream `LICENSE` files.

### The most important find: jcode's memory architecture already matches the owner's rulings

`/home/hermes/projects/upstream/jcode/docs/MEMORY_ARCHITECTURE.md`, and the code it
maps. This is the proven design for the Memory service and the DecisionProvider, and
it already implements the constraints the owner set:

- **No embeddings, no BM25 prefilter, no generative reranker, no sidecar fallback in
  recall.** "Default builds no longer include the ONNX/tokenizer embedding stack. The
  `embeddings` Cargo feature remains opt-in for historical benchmarks/debugging."
  This is the owner's "I do not want local embeddings" and "no local models" rulings
  already implemented and shipped upstream.
- **Recall is a typed decision call.** Jev returns a probability per candidate. Bounded
  batches of at most 24 memories, 64 KiB encoded request budget, sequential calls,
  60-second whole-selection deadline, automatic context UTF-8-safely bounded to 8 KiB.
- **An oversize memory is skipped whole, never scored as a prefix and injected with an
  unseen suffix.** A content-free log records the number skipped.
- **Any failed batch invalidates the entire selection.** No partial-result,
  stale-verdict, embedding or conventional-LLM fallback. This is the exact opposite of
  the 451 retry spiral that cost us today.
- **Redirects are refused so bearer credentials cannot be redirected. Errors do not
  echo provider response bodies.**
- **Untrusted-data fencing:** "Stored contents and conversation context are untrusted
  data in `state`, not instructions."
- **Pending results expire after two minutes. Injection IDs deduplicated per session.
  Scope-bound consumption re-checks that selected memories still exist, are active and
  have unchanged content**, so switching projects cannot inject a stale payload.
- **Deduplication does not collapse code spelling, punctuation or case.** Exact
  duplicate content in the same category and scope reinforces the existing memory.
- **Threshold must be finite and in 0.8..=1.0.**
- **Storage keeps tags, relationships, categories, trust, sources and
  superseded/inactive entries.** That is the provenance and status lifecycle the
  architecture document asks for, already modelled.

**Implementation map, the files to take:**

| File | What it gives us |
|---|---|
| `crates/jcode-base/src/jev.rs` | provider credentials and bounded HTTP for decision endpoints |
| `crates/jcode-base/src/memory_jev.rs` | direct batched relevance selection |
| `crates/jcode-base/src/memory_agent.rs` | asynchronous per-session coordinator |
| `crates/jcode-base/src/memory.rs` | local storage and public compatibility APIs |
| `crates/jcode-base/src/memory/pending.rs` | scope-bound pending injection |
| `crates/jcode-app-core/src/tool/memory.rs` | public memory tool surface |

### Rows updated by these sources

| # | Requirement | Source | Action | Note |
|---|---|---|---|---|
| M1 | Memory service: records, scope, provenance, status lifecycle | jcode `memory.rs` + `memory_jev.rs`; moltis `crates/memory` | **copy in** | jcode already models trust, sources, superseded and inactive. moltis gives the FTS5 chunk store and query sanitisation |
| M2 | Memory service: context compiler | jcode `memory_agent.rs` + `memory/pending.rs` | **copy in** | bounded context, two-minute expiry, per-session dedup, scope re-validation |
| M3 | Retrieval: relevance without embeddings | jcode `memory_jev.rs` | **copy in** | decision-based, already no-embedding |
| M4 | Retrieval: FTS5/BM25 keyword path | moltis `crates/memory/src/store_sqlite.rs` | **copy in** | includes FTS5 `MATCH` sanitisation, a real trap: dots in a query cause `fts5: syntax error` |
| M5 | Memory: conflict and supersession | hindsight `2026-02-09-resolving-memory-conflicts.md` | **copy in** | temporal evolution for contradictions |
| M6 | Task and state tracking | beads `beads.go`, `claim.go` | **copy in** | status machine plus claim, for the jcode adapter's task IDs and status reconciliation |
| M7 | Telemetry: append-only event log | BlockCell hash-chain audit log, `crates/core` | **copy in** | |
| M8 | DecisionProvider interface | jcode `jev.rs`; Clef on Workers AI | **new** interface, **copy in** the HTTP bounds | the interface is new because section 4 names it; the transport is jcode's |
| M9 | Vector and retrieval storage | kektordb | **evaluate, do not adopt** | Go, Apache-2.0. The document's own retrieval design is a hybrid cascade, and jcode has removed vector recall. Recorded so the row is not silently skipped |

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
