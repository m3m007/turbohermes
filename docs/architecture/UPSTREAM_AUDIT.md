# UPSTREAM_AUDIT — TurboHermes Phase 0 deliverable

**Read the document's opening line before reading this file.** The owner's
architecture document states its decision in the first sentence: "Build TurboHermes as a
fork of **BlockCell**, but do not merge jcode or Graphicle into the BlockCell runtime."

So there is **ONE fork, not three**. BlockCell is forked and becomes the TurboHermes
supervisor. jcode and graphicle are NOT forks and NOT merged: section 11 places them under
`integrations/` as external processes, and says "Keep jcode and Graphicle as external
processes initially." An earlier version of this file framed all three as "upstreams",
which is wrong and was corrected on the owner's instruction.

Owner architecture document, Phase 0: "Before changing code: pin exact commits for
BlockCell, jcode and Graphicle; build and test all three from clean checkouts; map
BlockCell's tool execution path, policy enforcement, hooks, snapshots, Ghost
learning and storage; map jcode's CLI/API, swarm task-DAG status, MCP
configuration, memory configuration and session lifecycle; map Graphicle's MCP
schema, code graph query tools, sync lifecycle, document ingestion and session
capture; review licences and preserve third-party notices; create a compatibility
matrix and a list of interfaces that must be wrapped rather than copied."

Status of this document: PARTIAL. Sections marked VERIFIED were measured on this
host. Sections marked NOT DONE were not performed. Nothing below is inferred where
it is not labelled.

## 1. Pinned commits — VERIFIED

| Project | Path | Pinned commit | Date |
|---|---|---|---|
| BlockCell | /home/hermes/projects/upstream/blockcell | 5ca38be861b4a3e8f7a1e8d332e242b42ed25710 | 2026-11-08 |
| jcode | /home/hermes/projects/upstream/jcode | 47e426e5ffe77913e3bf42276876a30aef018c79 | 2026-11-09 |
| Graphicle | /home/hermes/projects/upstream/graphicle | not pinned this pass | — |

## 2. Build and test from clean checkouts

| Project | Built | Tests | Evidence |
|---|---|---|---|
| BlockCell | YES, 5m09s dev profile | **2,654 passed, 0 failed** | measured 2026-11-09 |
| jcode | YES, 8m03s dev profile | **453 passed, 0 failed** | measured 2026-11-09 |
| Graphicle | NOT DONE | — | — |

jcode builds with 29 clippy warnings across `jcode-app-core` (4) and `jcode-tui` (25),
3 auto-fixable. Not a gate; recorded so a later run can separate new warnings from
pre-existing ones.

## 3. BlockCell execution path — VERIFIED from source

Crate layout: `agent, channels, core, providers, scheduler, skills, storage, tools,
updater`. Workspace members list `core, agent, tools, skills, scheduler, channels`
and more in `Cargo.toml` (resolver 2).

- **Tool execution**: `crates/tools/src/lib.rs` exposes `ToolRegistry`,
  `ScopedTool`, `ToolCapabilityOrigin`, `ToolScopeView`; the trait carries
  `execute_capability(&self, id, input)` at line 338 and `execute(&self, ctx,
  params)` at line 755.
- **Policy enforcement**: `crates/core/src/` contains `tool_policy.rs`,
  `path_policy.rs`, `capability.rs`, `evolution_boundary.rs`, `agent_identity.rs`,
  `budget.rs`, `scope.rs`, `task_contract.rs`, `session_key.rs`, `file_store.rs`.
  This is a substantially richer policy surface than anything in the greenfield
  crates.
- **Hooks**: no `fn`/`trait` matches for "hook" in `crates/core/src/*.rs` this
  pass. NOT MAPPED.
- **Snapshots, Ghost learning, storage**: NOT MAPPED this pass.

## 4. Skills audit — VERIFIED, AND IT REFUTES AN EARLIER CLAIM

`crates/skills/src/audit.rs` contains `fn tokenize_python` (line 193) which skips
whitespace, strips comments, and emits identifier tokens; `is_dangerous_python_target`
then matches the lowercased token exactly against a list including `os.system`,
`os.popen`, `shutil.rmtree`, `subprocess.run`, `eval`, `exec`, `__import__`.

Upstream test `test_python_dangerous_call_variants_are_rejected` asserts rejection
of `"import os\nos.system ('echo pwned')"` — the exact form previously claimed to
bypass the audit — plus `import os as operating_system` + `operating_system.popen`,
`from subprocess import check_output as capture`, and `getattr(os, 'system')('id')`.
A complementary test asserts dangerous names inside comments and string literals are
ALLOWED, so the tokeniser avoids false positives as well as bypasses.

**Correction recorded in D-083**: the earlier claim that BlockCell's audit is a
substring scan bypassed by `os.system (` with a space is FALSE. That claim was
load-bearing: it was the stated reason to hesitate on the fork. It came from a
GitHub issue and was never checked against source.

## 5. jcode and Graphicle — jcode DONE, Graphicle PARTIAL

### jcode — VERIFIED
Pinned 47e426e5ffe77913e3bf42276876a30aef018c79. Built clean, **453 tests passing, 0
failed**, 29 pre-existing clippy warnings (4 in jcode-app-core, 25 in jcode-tui).
Its enforcement surface was already mapped in a prior pass
(`research-notes/2026-11-08-jcode-enforcement-surface.md`): the rewrite-not-block hook
pattern built from `parse_gate_stdout` / `GateOutcome` / `CommandVerdict` /
`hookSpecificOutput.updatedInput`. Still to map for Phase 0's letter: the CLI/API surface,
swarm task-DAG status, MCP configuration, memory configuration and session lifecycle.

### Graphicle — CONSUMED AS A RELEASE BINARY, NOT BUILT FROM SOURCE

Pinned ddf11b1dac674b4a854356bda8ab67c0d5ced239. **Not a Rust workspace**: no root
Cargo.toml. It is a polyglot repo: three Rust crates under `crates/` (graphicle,
graphicle-graph, graphicle-memory), a Python bench suite under `bench/`, a Swift desktop
app under `desktop/`, and a Python demo.

**DO NOT BUILD THIS FROM SOURCE.** Owner ruling 2026-11-09: "For god's sake how many
times do I need to tell you to not reinvent the wheel!" The document's Phase 0 line
"build and test all three from clean checkouts" applies to BlockCell, which we fork, not
to graphicle, which we consume as-is. Graphicle publishes prebuilt
`graphicle-<version>-x86_64-unknown-linux-gnu.tar.gz` plus `.sha256` and `SHA256SUMS`,
and ships an `install.sh` that picks the archive for the machine, verifies the checksum
and copies both programs into `~/.local/bin` without sudo. The release artifact IS the
integration target.

A source build was started and aborted: it cost 7.3 GB in `crates/graphicle/target` and
produced no binary. That is the recorded cost of the mistake, kept here so it is not
repeated.

MCP surface mapped from source, `crates/graphicle-graph/src/serve.rs`:
- **10 tools**, from `assets/serve/constants.json`: `query_graph` (BFS/DFS search),
  `get_node`, `get_neighbors`, `get_community`, `god_nodes`, `graph_stats`,
  `shortest_path`, `list_prs`, `get_pr_impact`, `triage_prs`.
- **6 resources**: `graphicle-graph://report`, `://stats`, `://god-nodes`,
  `://surprises`, `://audit`, `://questions`.
- **Staleness signal**: `_built_at_commit` plus a `build_commit_line()` that compares
  against git HEAD, because an MCP client cannot stat graph.json. This is a real design
  detail worth copying rather than reinventing.
- An opt-in retrieval tool is appended to the tool list under `serve --retrieval`.

Still to map for Phase 0's letter: sync lifecycle, document ingestion, session capture.

## 6. Licences and third-party notices — IN PROGRESS

Ticket t_d47174be. Briefed to read each repo's licence file, every crate's `license`
field, and any NOTICE file, and to report notice-preservation obligations for forking.

## 7. Compatibility matrix and wrap-versus-copy list — IN PROGRESS

Ticket t_d47174be. This is the deliverable Phase 1 depends on: for each interface in
BlockCell's policy surface and jcode's harness surface, WRAP or COPY with a reason, plus
which of our four crates an upstream already covers.

## 8. Findings that change Phase 1

1. **BlockCell is a working runtime with 2,654 passing tests.** The greenfield
   crates (414 tests across four crates) contain no gateway, no audit chain, no
   skills layer and no running binary. Per the owner's ruling, the greenfield code
   is a liability, not an asset.
2. **The greenfield crates are the document's "central investment"** — the Memory
   and Learning Service, the decision layer and the event spine — and the document
   says they sit ABOVE the three runtimes. So they are integration crates to be
   moved inside the fork, not a replacement for it.
3. **BlockCell's policy surface is richer than anything built so far**
   (`tool_policy`, `path_policy`, `capability`, `evolution_boundary`,
   `agent_identity`, `budget`, `scope`, `task_contract`). Wrapping it is cheaper
   than reimplementing it.
4. **Unit supervision (D-085)**: `hermes.service` is a LEGACY unit per Hermes's
   own source; `hermes-gateway.service` is current. The legacy unit crash-looped
   4,645 times after the Nov 8 VPS restart. `hermes gateway migrate-legacy` does
   not detect this case because its ExecStart markers all require a trailing space
   after "gateway". Cleanup shipped as an owner paste block.

## 9. What Phase 1 needs that this document does not yet provide

jcode and Graphicle maps, the licence review, and the compatibility matrix. Until
those exist, Phase 1's "wrap rather than copy" decisions cannot be made from
evidence.
