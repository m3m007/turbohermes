# Mutation-pipeline audit: BlockCell at the fork pin

Authority: work order v7 section 2.2 (Mutation safety row) and section 9 Phase 1; `agent-daemon/TURBOHERMES-ARCHITECTURE-v2.md` line 395, the transaction protocol (propose, policy check, preconditions, record intent, checkpoint, execute, observe actual state, verify postconditions, commit or restore).

**Read from commit, not the working tree**, which other workers were editing. Every BlockCell line below came from `git show 0e771d5:<path>` (last audit commit, pre-port state). Verified live: `5ca38be861b4a3e8f7a1e8d332e242b42ed25710` is an ancestor of `0e771d5`, and `git diff --numstat 5ca38be..0e771d5 -- <file>` is empty for all twelve audited files, so pin and audit commit hold identical bytes. The port that followed this audit is described in sections 4 and 5; it does not change the pre-port verdicts in section 2.

## 1. Scope and pins

| Project | Commit | Licence | Evidence read |
|---|---|---|---|
| blockcell-labs/blockcell | `5ca38be861b4a3e8f7a1e8d332e242b42ed25710` | MIT | `LICENSE:1` "MIT License", "Copyright (c) 2026 blockcell.ai"; `Cargo.toml:20` `license = "MIT"` |
| moltis-org/moltis | `8a12400dc10551dfb9a6b568414b91a343ef2d5d` | MIT | `LICENSE.md:1` "MIT License", "Copyright (c) 2025 Fabien Penso"; `Cargo.toml:194` `license = "MIT"` |

Both permissive, no copyleft, no conflict with the MIT fork. Moltis HEAD at
`/home/hermes/projects/upstream/moltis` confirmed at `8a12400d` before reading.

## 2. Verdict table

| Check | Verdict | BlockCell evidence | What the evidence shows |
|---|---|---|---|
| A. Durable snapshot before every mutation | **ABSENT** | `skill_file_store.rs:506-512`, `:514-530`; `memory_file_store.rs:397-410`; `fs.rs:268`, `:503`; `fs_backend.rs:136-141` | Snapshotting exists but is confined to two file stores. The generic file tools write with no snapshot. |
| B. Restore at the level of a single mutation | **ABSENT** | `skill_file_store.rs:389-469`, `:568-591`; `memory_file_store.rs:295-327`, `:412-438` | Both stores restore only the newest snapshot for a target, and that snapshot is the whole target, not the one mutation. |
| C. A verification step | **PRESENT** (partly) | `task_progress.rs:44-106`, `:267-371`, `:373-385`, `:387-420`; `task_contract.rs`; `tool_pipeline.rs:155-164`, `:457-478` | Revision-tracked task verification and an 8-stage admission gate exist. Per-mutation postcondition verification does not. |
| D. Uncertain-outcome handling | **PRESENT** | `failure.rs:28`, `:34-41`, `:48-61`, `:95-106`; `tool_pipeline.rs:232-237`, `:444-449`, `:1344-1377`; `task_progress.rs:294`; `prefetch.rs:158` | `Unknown` is modelled as distinct from `Failed`, with reconcile-before-retry wired into the pipeline. |

## 3. Per-check findings

**A, ABSENT.** Snapshotting is confined to two hand-written stores.
`crates/agent/src/skill_file_store.rs` `snapshot_before_write` (506-512) delegates to `snapshot_skill_dir` (514-530), which `copy_dir_recursive`s the whole skill directory; `crates/agent/src/memory_file_store.rs` `snapshot_before_write` (397-410) does one `fs::copy`. Neither is invoked from a generic tool.

Negative evidence, verified by search. `crates/tools/src/file_ops.rs` has **zero** matches for `snapshot|backup|\.bak|restore` across 845 lines, though it implements `action_delete` (202), `action_move` (234), `action_copy` (280), `action_compress` (380) and `action_decompress` (541), each destructive. `crates/tools/src/fs.rs` `write_file_result` (260-273) calls `fs.write()` at 268 and `edit_file` at 503, both straight through `FilesystemBackend::write` (`crates/tools/src/fs_backend.rs:136-141`), which is `create_dir_all` plus `tokio::fs::write` and nothing else. `exec` and `shell` mutate through an opaque subprocess with no snapshot path.

A third mechanism exists and was previously unrecorded: `crates/skills/src/versioning.rs:707-765` `restore_version_snapshot`, which verifies a tree hash and restores via a temp-dir swap. It is **not** tool-pipeline rollback. Verified reachability: called only from `switch_to_version_unlocked` (`versioning.rs:190-214`) and `rollback` (`versioning.rs:217-273`), whose non-test callers are `evolution/versioning.rs:440` `restore_previous_version` <- `evolution/compile_deploy.rs:442` in `SkillEvolution::rollback` (`compile_deploy.rs:409`) <- `service.rs:1262` in `tick_single_observation` (`service.rs:1208`) <- `tick_observations` (`service.rs:1051-1061`) <- `crates/scheduler/src/skill_evolution_worker.rs:184` and `:376`. Background scheduler, not a tool. `manager.rs:1383` `rollback_version` has **zero callers** repo-wide.

**B, ABSENT.** `SkillFileStore::restore_latest` (`skill_file_store.rs:389-469`) and `MemoryFileStore::restore_latest` (`memory_file_store.rs:295-327`) both resolve through `latest_snapshot_for` (`skill_file_store.rs:568-591`, `memory_file_store.rs:412-438`), which iterates `read_dir` and takes the lexicographically greatest name. No per-mutation id, no manifest, no way to address one mutation. Granularity is wrong in the other direction too: a skill mutation snapshots the entire skill directory, so restoring reverts every auxiliary file. The handle returned to the caller is a bare path string (`snapshot_ref`, serialized at `skill_file_store.rs:679-686`) with no manifest, no recorded reason and no validity check.

**C, PRESENT (partly).** Two mechanisms exist, neither a per-mutation postcondition check. First, task verification: `TaskProgress` (`task_progress.rs:134`) tracks `changed` and `uncertain_effects`, and `needs_verification` (373-385) requires a recognized check at the current `revision`, so an unverified change blocks further execution; `observe` (267-371) feeds it and `verification_reason` (387-420) explains the state. Evidence comes from `ToolEvidence::from_execution_with_policy` (44-106) consulting `TaskContract` and `VerificationPolicy` (`crates/core/src/task_contract.rs:7`, `:15`, `:38-40`). Second, admission preconditions: the 8-stage `ADMISSION_STAGES` (`tool_pipeline.rs:155-164`, dispatched in `admit_tool_call` 457-478) runs `Toggle`, `PreExecute`, `ExecutionContract`, `Policy`, `DangerousOperation`, `PathAccess`, `FileObservation`, `ExplicitConfirmation`. Missing against the transaction protocol: no `after_hash` comparison, no postcondition check tied to one mutation, no `verification_result` on a mutation record. Verified: `git grep "after_hash\|before_hash\|postcondition\|verification_result\|MutationRecord" 0e771d5 -- crates` returns zero matches.

**D, PRESENT.** `ToolOutcome` (`failure.rs:48-61`) separates `Succeeded`, `Failed`, `Rejected`, `Cancelled`, `TimedOut`, `NotStarted`, `Unknown`, with `FailureKind::Unknown` (28) and `Idempotency` (34-41). The module doc at 4-5 states the rule; `ToolOutcome::retry_allowed` (95-106) encodes it, mapping `Unknown` to `FailureKind::Unknown`, whose `retry_allowed` (169-178) permits retry only when idempotency is `Safe`. At runtime `unknown_outcome_hint` (`tool_pipeline.rs:232-237`) injects reconcile-first, wired at 444-449; `classify_tool_error` (`finish_tool_execution`, 1344-1377) maps a timed-out non-idempotent call to `Unknown` not `Failed` (`failure.rs:263-267`); `observe` sets `uncertain_effects` at 294; `prefetch.rs:158` refuses to prefetch non-idempotent tools. Kept as is.

## 4. What Moltis has, and the port mapping

Two files in `crates/tools/src/` at `8a12400d`, 823 lines total, both MIT.

| Upstream file | What it holds | BlockCell insertion point |
|---|---|---|
| `checkpoints.rs` (631 lines) | `CheckpointRecord` (36-58: id, created_at, reason, source_path, source_kind, existed, backup_path), `CheckpointManager` (60-118) with `checkpoint_path` (72) and `restore` (108), per-turn `turns.jsonl` index `TurnRecord` (122-), `checkpoint_path_blocking` (341), `restore_checkpoint_blocking` (398-429), `validate_checkpoint_id` (431-440, 32 lowercase hex chars, rejecting path traversal), `copy_dir_recursive` (455) with symlink refusal (461-465, 488-494), manifest write/read, pruning above 500 (`CLEANUP_CHECKPOINT_THRESHOLD`, 24) | The manifest format and single-mutation restore semantics BlockCell lacks (A and B). Each checkpoint is one mutation of one path with its own 32-hex id, so restore is addressable per mutation. |
| `auto_checkpoint.rs` (192 lines) | `AutoCheckpointHook` (33), a `HookHandler` on `BeforeToolCall` + `AgentEnd` (69), priority 100 so it runs before the tool modifies the file (72-75), `CHECKPOINT_TOOLS = ["Write", "Edit", "MultiEdit"]` (21), `extract_file_path` (48), `extract_run_id` (57), per-session `ActiveTurn` accumulation (24, 36, 154-164) | The hook that makes snapshotting cover every mutation instead of two hand-picked stores. Insertion point chosen: `checkpoint_mutation_targets` called in `run_tool_pipeline` after `admit_tool_call` returns and before `execute_guarded_tool`, not at `AdmissionStage::PreExecute`. Two reasons: `PreExecute` is skipped when a prefetched call already fired it (`pre_execute_done`), and path-policy approval must run before a snapshot path is resolved. `PreExecute` still sees listener-rewritten arguments, and so does this call. |

`docs/src/checkpoints.md` (75 lines) documents it: `checkpoints_list` (42), `checkpoint_restore` (53), the `checkpointId` field on results (65), per-turn grouping (9-12). Its tool list is wider than the hook's: `create_skill`, `update_skill`, `delete_skill`, `write_skill_files`, `memory_save`, `memory_forget`, `memory_delete` (14-18).

Implementation (see section 5 for the files): from `checkpoints.rs` the `CheckpointRecord` manifest schema, `validate_checkpoint_id`, `checkpoint_path_blocking`, `restore_checkpoint_blocking`, `copy_dir_recursive` and the symlink refusal are ported; the error type is `blockcell_core::Error`, `moltis_agents::tool_registry::AgentTool` is BlockCell's `Tool`, and `time::OffsetDateTime` is `chrono::Utc`. From `auto_checkpoint.rs` the hook shape is ported, not the tool names: the snapshot set is `write_file`, `edit_file`, `file_ops` and `office_write`. `exec` and `shell` are excluded because they have no addressable target; `skill_manage` and the memory tools are excluded because their stores already snapshot (`skill_file_store.rs`, `memory_file_store.rs`). Grouping by turn becomes grouping by `current_turn_id`, which already flows into the audit chain (`audit_tool_call`, `tool_pipeline.rs:591`).

**Divergence decided.** `auto_checkpoint.rs:131-141` warns and returns `HookAction::Continue` when checkpointing fails, so the tool proceeds without a snapshot. Work order v7 section 4 makes filesystem snapshots a required element, so silently proceeding is not adopted. The port blocks: a failed snapshot returns `CHECKPOINT_FAILED` and the mutation does not run. This is the one deliberate behavioural change from upstream, recorded in `mutation_checkpoint.rs`'s module doc.

## 5. The port as implemented, and what remains

Files: `crates/tools/src/checkpoints.rs` (799 lines, port plus the two tools and
`restore_within`), `crates/agent/src/runtime/mutation_checkpoint.rs` (272 lines,
hook shape and target extraction), plus wiring in `crates/agent/src/runtime.rs`,
`crates/agent/src/runtime/tool_pipeline.rs`, `crates/tools/src/lib.rs`,
`crates/tools/src/registry.rs` and `crates/core/src/config/intent.rs`. A failed
snapshot returns `CHECKPOINT_FAILED` and the mutation is rejected.

Verified on this host (commands in `REUSE_MAP.md`, section "Implemented in this run"):
`cargo test -p blockcell-tools checkpoints` 7 passed; `-p blockcell-agent checkpoint`
15 passed; `cargo test -p blockcell-tools --test checkpoint_crash_recovery` 2 passed,
including a child process that aborts after the mutation and a parent that restores
the pre-crash content from the on-disk manifest; `-p blockcell-core -p blockcell-tools
-p blockcell-agent` 1816 passed, 9 ignored, 0 failed.

These are not covered, and Moltis does not provide them either, so porting cannot
close them.

1. **`exec` and `shell` opaque effects.** The checkpoint model snapshots a path
   before it is modified. An `exec` call writes anywhere, spawns processes, hits
   network endpoints and mutates databases, with no single `source_path` to
   snapshot. Neither BlockCell nor Moltis addresses this: `rg -n
   "snapshot\|checkpoint" crates/tools/src/exec.rs crates/tools/src/exec/` in
   Moltis returns nothing.
2. **Per-mutation `after_hash`, postcondition and `verification_result`.** The
   transaction protocol requires intended effect, actual observed effect,
   postcondition result and recovery outcome on every mutation record. BlockCell
   has none of these fields and Moltis has none either: `rg -c` over Moltis
   `crates/` and `docs/` finds zero files containing `before_hash`,
   `postcondition`, `verification_result` or `MutationRecord`; its one
   `after_hash` hit is an unrelated variable in `crates/voice/src/tts/mod.rs:517`.
   This is glue, written rather than ported, because no approved source has it.

## 6. Reproduce commands

BlockCell, all at `0e771d5`:

```
git merge-base --is-ancestor 5ca38be861b4a3e8f7a1e8d332e242b42ed25710 0e771d5
git diff --numstat 5ca38be861b4a3e8f7a1e8d332e242b42ed25710 0e771d5 -- crates/tools crates/agent crates/skills crates/core
git show 0e771d5:crates/agent/src/skill_file_store.rs | rg -n "fn restore_latest|fn snapshot_before_write|fn snapshot_skill_dir|fn latest_snapshot_for"
git show 0e771d5:crates/agent/src/skill_file_store.rs | sed -n '389,392p;504,531p;566,591p;679,686p'
git show 0e771d5:crates/agent/src/memory_file_store.rs | rg -n "fn restore_latest|fn snapshot_before_write|fn latest_snapshot_for"
git show 0e771d5:crates/agent/src/memory_file_store.rs | sed -n '293,298p;395,438p'
git show 0e771d5:crates/tools/src/fs.rs | rg -n "async fn write_file_result|self\.fs\.write"
git show 0e771d5:crates/tools/src/fs_backend.rs | rg -n "async fn write"
git show 0e771d5:crates/tools/src/file_ops.rs | rg -n "snapshot|backup|\.bak|restore"   # zero matches
git show 0e771d5:crates/tools/src/file_ops.rs | rg -n "^async fn action_|^fn action_"
git show 0e771d5:crates/skills/src/versioning.rs | rg -n "fn restore_version_snapshot|fn switch_to_version_unlocked|fn rollback"
git show 0e771d5:crates/skills/src/versioning.rs | sed -n '190,214p;217,273p;707,765p'
git grep -n "restore_previous_version\|rollback_capability\|rollback_version\|switch_version\|tick_observations\|SkillVersionManager" 0e771d5 -- crates
git show 0e771d5:crates/agent/src/runtime/task_progress.rs | sed -n '44,107p;267,296p;373,420p'
git show 0e771d5:crates/agent/src/runtime/tool_pipeline.rs | rg -n "ADMISSION_STAGES|PreExecute|fn unknown_outcome_hint|fn admit_tool_call|fn finish_tool_execution|fn audit_tool_call"
git show 0e771d5:crates/agent/src/runtime/tool_pipeline.rs | sed -n '155,164p;232,237p;444,449p;457,478p;572,616p;1344,1377p'
git show 0e771d5:crates/agent/src/runtime/prefetch.rs | sed -n '150,168p'
git show 0e771d5:crates/core/src/failure.rs | sed -n '28p;34,41p;48,61p;95,106p;249,267p'
git show 0e771d5:crates/core/src/task_contract.rs | rg -n "enum VerificationPolicy|struct TaskContract"
git grep -n "after_hash|before_hash|postcondition|verification_result|MutationRecord" 0e771d5 -- crates   # zero matches
```

Moltis at `/home/hermes/projects/upstream/moltis`:

```
git rev-parse HEAD                                      # 8a12400dc10551dfb9a6b568414b91a343ef2d5d
head -3 LICENSE.md; rg -n "^license" Cargo.toml
wc -l crates/tools/src/checkpoints.rs crates/tools/src/auto_checkpoint.rs
rg -n "struct CheckpointRecord|struct CheckpointManager|struct TurnRecord|fn checkpoint_path|fn restore|fn validate_checkpoint_id|fn copy_dir_recursive|CLEANUP_CHECKPOINT_THRESHOLD" crates/tools/src/checkpoints.rs
sed -n '341,370p;398,440p;455,470p;488,496p' crates/tools/src/checkpoints.rs
rg -n "CHECKPOINT_TOOLS|struct AutoCheckpointHook|impl HookHandler|BeforeToolCall|AgentEnd|fn extract_file_path|fn extract_run_id|ActiveTurn|fn priority" crates/tools/src/auto_checkpoint.rs
sed -n '125,145p' crates/tools/src/auto_checkpoint.rs
rg -n "checkpoints_list|checkpoint_restore|checkpointId|rollback|500" docs/src/checkpoints.md
rg -c "after_hash|before_hash|postcondition|verification_result|MutationRecord" crates/ docs/   # zero, one unrelated after_hashes
rg -n "snapshot|checkpoint" crates/tools/src/exec.rs crates/tools/src/exec/                # zero matches
```

## 7. Citation corrections made against the existing record

Re-checked every citation in `REUSE_MAP.md` section "Phase 1 mutation-pipeline audit" at `0e771d5` and in commits `d89825c` and `0e771d5`. Five corrections:

1. **`ToolOutcome::retry_allowed` end line: recorded 107, actually 106.** The prior correction commit changed "101" to "107"; `failure.rs` 95-106 is the function, 107 is the `impl ToolOutcome` closing brace.
2. **`ToolEvidence::from_execution_with_policy` end line: recorded 107, actually 106.** Same commit changed "106" to "107"; `task_progress.rs` 100-106 is the struct literal, 107 the `impl` closing brace.
3. **`MemoryFileStore::restore_latest` end line: recorded 329, actually 327.** The `Ok(MemoryFileMutation { ... })` literal closes at 326, the function at 327; 328 is blank and 329 begins the next method's doc comment.
4. **`SkillVersionManager` is cited as a symbol that does not exist.** `git grep -n "SkillVersionManager" 0e771d5 -- crates` returns zero matches. The real types are `VersionManager` (`versioning.rs:67`, used at `manager.rs:772`) and `CapabilityVersionManager` (`capability_versioning.rs:271`, used at `core_evolution.rs:182`). The negative-evidence claim still holds; the term is removed from it.
5. **The third snapshot path was mis-attributed.** The record says `restore_version_snapshot` is reached via `core_evolution.rs:1453` `rollback_capability`. Wrong: that line calls `CapabilityVersionManager::rollback`, a different type in a different file, which never calls `restore_version_snapshot`, and `rollback_capability` has zero callers repo-wide. The chain that does reach it is `evolution/versioning.rs:440` <- `evolution/compile_deploy.rs:442` <- `service.rs:1262` <- `service.rs:1061` <- `skill_evolution_worker.rs:184,376`. Verdict unchanged: every link is background scheduler code with no tool reachability, and `manager.rs:1383` `rollback_version` genuinely has zero callers.

Confirmed correct without change: `skill_file_store.rs:506-512`, `:514-530`, `:389-469`, `:568-591`; `memory_file_store.rs:397-410`, `:412-438`; `fs.rs:268`, `:503`; `fs_backend.rs:136-141`; `task_progress.rs:267-371`, `:373-385`, `:387-420`; `tool_pipeline.rs:155-164`, `:232-237`, `:444-449`, `:457-478`, `:1344-1377`; `failure.rs:28`, `:34-41`, `:48-61`; `task_progress.rs:294`; `prefetch.rs:158`; `versioning.rs:707-765`; and the zero-match claims for `file_ops.rs` and for `after_hash|before_hash|postcondition|verification_result|MutationRecord`.
