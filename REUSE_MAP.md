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
| 2.1 | "Implement the jcode adapter using the narrowest stable interface" | jcode process surface | **glue** | the adapter is glue per section 4; checked jcode and BlockCell, neither exposes jcode's process surface as a library, so the thin wrapper is glue joining two runtimes |
| 2.2 | "Launch jcode in a controlled repository/worktree scope" | jcode worktree support | **use as is** | jcode already has optional worktrees and file-touch notifications; call them, do not rebuild them |
| 2.3 | "Pass task IDs, acceptance criteria, budgets and allowed operations" | BlockCell `crates/core/src/{budget,task_contract,scope}.rs` | **copy in** | BlockCell already models budgets, task contracts and scope. Wrap those types |
| 2.4 | "Capture worker and swarm lifecycle events" | jcode event surface | **use as is** | read jcode's lifecycle events; add only the trace identifier |
| 2.5 | "Collect typed task artifacts and evidence" | BlockCell audit log, `crates/core` | **copy** + **glue** | the append-only log is BlockCell's; the typed artifact envelope is glue |
| 2.6 | "Reconcile status after timeouts, crashes and supervisor restarts" | beads `claim.go` | **port-adapt** | Go to Rust; the claim and status-reconciliation logic is ported, not rewritten |
| 2.7 | "Add integration tests for cancellation, restart and duplicate dispatch" | — | **glue** | integration tests are explicitly required by the document and are not upstream tests, so they are glue |

## Phase 3 — Add Graphicle as code intelligence

| # | Requirement | Source | Action | Note |
|---|---|---|---|---|
| 3.1 | "Start Graphicle for a repository/workspace" | graphicle release binary | **use as is** | `graphicle-<ver>-x86_64-unknown-linux-gnu.tar.gz`, checksum-verified. Never built from source |
| 3.2 | "Run code-only indexing first" | graphicle's own indexer | **use as is** | |
| 3.3 | "Expose its MCP tools to jcode and TurboHermes" | graphicle `serve.rs`, 10 tools | **use as is** | `query_graph`, `get_node`, `get_neighbors`, `get_community`, `god_nodes`, `graph_stats`, `shortest_path`, `list_prs`, `get_pr_impact`, `triage_prs` |
| 3.4 | "Build a typed adapter for code queries and cited context" | graphicle MCP `serve.rs` | **glue** | the adapter is glue; the 10 MCP tools and 6 resources are used as is |
| 3.5 | "Preserve file/line/symbol references in task artifacts" | graphicle `_built_at_commit` pattern | **copy** + **glue** | the reference shape is graphicle's; the artifact field is glue |
| 3.6 | "Test incremental sync after edits, branch changes and merges" | — | **glue** | integration tests, glue |
| 3.7 | "Add staleness detection for memories linked to changed symbols" | graphicle `_built_at_commit` + `build_commit_line()` | **copy in** | graphicle already solves this: it records the build commit and compares to git HEAD because an MCP client cannot stat graph.json. Copy the pattern |

## Phase 4+ components that section 3 makes new

| Component | Source primitives to check first | Action |
|---|---|---|
| Memory service | jcode `memory.rs`/`memory_jev.rs`, moltis `crates/memory`, BlockCell `crates/storage` and `ghost_ledger.rs` | **copy** + **port-adapt** | |
| Telemetry (event log, trace ids, outcome collector) | BlockCell hash-chain audit log in `crates/core` | **copy** + **glue** | |
| DecisionProvider (Jev, Clef, Clef-Flash, deterministic) | jcode `jev.rs`; Clef on Cloudflare Workers AI; LiteLLM `/v1/decisions` and `/v1/systemone` | **glue** interface, **use as is** backends | |
| TrajectoryCritic + OpenHands Critic adapter | OpenHands SDK critic on :8079 | **glue** |
| Evaluation corpus tooling | — | **glue** |

## Approved sources added by implementation order v3, ladder updated by v5

Order v3 added four approved copy sources, overriding the architecture document's
wording that they are "design comparison, not proposed dependencies". Order v5
supersedes v3 and changes the action vocabulary: the ladder now has five steps, and
step 4 is "Port, adapt and merge", which is what unlocks the Go and TypeScript
sources. All four inspected at HEAD on 2026-11-10.

The five-step ladder, quoted from order v5 section 1:

1. **Use as is.** Run the upstream project unmodified and talk to it through its
   interface.
2. **Fork and modify.** Fork the repository and change only what is needed.
3. **Copy in.** Copy specific files or modules from a source in section 2, keep the
   licence, and note the source path and commit.
4. **Port, adapt and merge.** Take working code from a source in section 2 and move it
   into the target: change the language binding, the types, the storage layer or the
   interface so it fits, and keep its logic and algorithms. Merge code from several
   sources where the design needs it.
5. **Write glue only.** Original code is allowed only to connect the pieces above, and
   only when steps 1 to 4 do not fit.

Per section 4: "An algorithm, data structure, storage layer, retrieval method or
protocol that exists in a source in section 2 is ported or adapted from that source. It
is never written again from the description."

| Source | Commit | Licence | Language | What it holds that we need |
|---|---|---|---|---|
| `moltis-org/moltis` | cloned `--depth 1` | MIT | **Rust** | `crates/memory` (FTS5 chunk store, embeddings, reranking, search, watcher, writer, splitter), `crates/memory-zvec`, `crates/code-index`, `crates/skills`, `crates/sessions`, `crates/observability` |
| `steveyegge/beads` | cloned `--depth 1` | MIT | Go | Task and state tracking with a claim mechanism. Status machine: `open`, `in_progress`, `blocked`, `closed`. `claim.go` is the interesting file |
| `vectorize-io/hindsight` | cloned `--depth 1` | MIT | TypeScript | Memory conflict resolution by temporal evolution. `hindsight-docs/blog/2026-02-09-resolving-memory-conflicts.md` is the design doc for supersession |
| `sanonone/kektordb` | cloned `--depth 1` | Apache-2.0 | Go | Vector and retrieval storage. Note: the URL in order v5 (`kektordb/kektor`) does not exist; the real repository is `sanonone/kektordb` |

**Licence check, per the order's rule:** all four are permissive (three MIT, one
Apache-2.0). No copyleft, no conflict with the MIT/ Apache-2.0 targets. Apache-2.0
requires stating changes; MIT requires the notice. Both satisfied by `THIRD-PARTY.md`
plus keeping upstream `LICENSE` files.

**Language binding is not a blocker.** Steps 3 and 4 both apply: moltis is Rust so its
memory crate is a direct copy candidate; beads, hindsight and kektordb are Go and
TypeScript, so step 4, port and adapt, is the action, keeping the logic and algorithms
and changing the binding and types.

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

### Rows updated by these sources, using the v5 action vocabulary

Order v5 section 5 requires the action to be one of `use / fork / copy / port-adapt /
glue`, and every "glue" row must list what was checked in each source and why nothing
could be ported or adapted.

| # | Requirement | Source | Action | Note |
|---|---|---|---|---|
| M1 | Memory service: records, scope, provenance, status lifecycle | jcode `memory.rs` + `memory_jev.rs`; moltis `crates/memory` | **copy** | jcode already models trust, sources, superseded and inactive. moltis gives the FTS5 chunk store and query sanitisation |
| M2 | Memory service: context compiler | jcode `memory_agent.rs` + `memory/pending.rs` | **copy** | bounded context, two-minute expiry, per-session dedup, scope re-validation |
| M3 | Retrieval: relevance without embeddings | jcode `memory_jev.rs` | **copy** | decision-based, already no-embedding |
| M4 | Retrieval: FTS5/BM25 keyword path | moltis `crates/memory/src/store_sqlite.rs` | **copy** | includes FTS5 `MATCH` sanitisation, a real trap: dots in a query cause `fts5: syntax error` |
| M5 | Memory: conflict and supersession | hindsight `2026-02-09-resolving-memory-conflicts.md` | **port-adapt** | TypeScript to Rust. Keep the temporal-evolution algorithm, change the binding and types |
| M6 | Task and state tracking | beads `beads.go`, `claim.go` | **port-adapt** | Go to Rust. Keep the status machine and claim logic; the four states are `open`, `in_progress`, `blocked`, `closed` |
| M7 | Telemetry: append-only event log | BlockCell hash-chain audit log, `crates/core` | **copy** | already in the fork |
| M8 | DecisionProvider interface | jcode `jev.rs`; Clef on Workers AI | **glue** | checked: jcode `jev.rs` is the closest and its HTTP bounds are copied, not rewritten. The trait itself is glue because section 4 names it and no source defines it. Transport is ported, not written |
| M9 | Vector and retrieval storage | kektordb | **port-adapt, evaluate first** | Go, Apache-2.0. jcode has removed vector recall and the document's retrieval design is a hybrid cascade, so this row is evaluated before it is adopted. Recorded so it is not silently skipped |
| M10 | Telemetry: trace identifiers, outcome collector | BlockCell audit log; OpenHands learning-to-verify for outcome signals | **copy** + **glue** | the log is BlockCell's; the outcome-signal vocabulary comes from the OpenHands learning-to-verify design |
| M11 | TrajectoryCritic + OpenHands Critic adapter | OpenHands SDK critic on :8079 | **glue** | the adapter is glue; the critic itself is an external service we talk to, never reimplemented |
| M12 | Evaluation corpus tooling | — | **glue** | checked all section 2 sources; none holds an evaluation-corpus tool. The corpus format follows the document's acceptance tests in section 13 |

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
