## [0.1.8] - 2026-10-22


This release runs on two tracks: the **Agent Runtime architecture evolution** (absorbing the composable / observable / interceptable / replayable design of a next-generation Agent Runtime while preserving Self-Evolution and Ghost Learning), and a **repo-wide security and correctness close-out** (multiple review passes across every crate, converging tool admission, sandbox boundaries, and path/credential handling onto shared primitives). The goal is to move from "it runs" to "explainable, replayable, auditable".

### Added

- **Runtime Kernel**: added an event bus (`EventBus`), a serial interception chain (`Waterfall`), and unified scope primitives (`ScopedRegistry` / `ScopeGuard`). Events are dispatched by priority and scope filtering, and listeners support timeout and failure isolation (fail-open / fail-closed).
- **Agent lifecycle events**: added `agent/pre-step`, `agent/request`, `agent/request-error`, `turn/start`, `turn/end`, `step/start`, `step/end`, `tool/pre-execute`, `tool/post-execute`, `tool/result`, `model/response`, `assistant/attempt`, `memory/write`, `subagent/spawn`, `subagent/completed`, `evolution/deployed`, `compaction/start`, and `compaction/end`, classified by Durable / Live / Capability consistency semantics.
- **Guarded Tool Pipeline**: centralized tool toggles, policy, dangerous-operation, path-access, and shell-escalation checks into one pipeline; individual tools no longer handle permissions, approvals, or auditing themselves.
- **Failure semantics**: added `FailureKind`, `Idempotency`, and `ToolOutcome` to distinguish failed / rejected / cancelled / timed-out / not-started / unknown; unknown outcomes are reconciled before retry instead of being retried automatically. Model request errors now pick a recovery strategy by type (compact-then-retry on context overflow, backoff on rate limit, provider switch on auth failure).
- **Session Event Log**: added an append-only session event log (JSONL sidecar, `schema_version`, monotonic `seq`, retention cleanup), with the transcript kept as the primary store and the event log running alongside it incrementally.
- **Session replay and evaluation**: added `blockcell session list / replay / verify` to replay turns, steps, model requests, and tool calls from the event log; added `SessionMetrics` for turn / step / tool-outcome / retry / token / duration metrics.
- **PromptRegistry**: the system prompt is now assembled from pluggable `PromptSection` registrations with scope override, template-variable resolution, and integrity checks; added `PromptContribution` and trust levels so learned skills and candidate behavior are injected behind a low-trust boundary.
- **Agent Loop contract**: separated the `AgentLoop` contract from the default loop implementation so the way a turn is driven can be replaced.
- **Capability seams**: added `SandboxPolicy` / `SandboxProvider` (native macOS Seatbelt / Linux Landlock+seccomp, falling back to approval when unavailable), `ShellExecutor`, `FilesystemBackend` (`LocalFilesystem`), `BrowserBackend` (`CdpBrowserBackend`), and `SubagentProvider` (a unified contract for fork and typed agents, with handles exposing status / cancel / result).
- **Three-layer tool definition**: `ToolDefinition` = `ToolSchema` (model-facing) + `ToolExecutionSpec` (permissions and idempotency) + `ToolPresentation` (presentation), decoupling presentation from execution permissions.
- **Compaction abstraction**: compaction is abstracted into a `ContextCompactor` strategy (`ModelSummaryCompactor` / `ExtractiveCompactor`).
- **`agent/pre-step` decision gate**: consolidates budget, compaction, and context injection, and supports a registrable policy chain that can rewrite or stop the step.
- **Evolution isolation**: added the Evolution Candidate Scope and an evolution trust boundary; candidate skills / prompts / policies are visible only inside a view that explicitly enters candidate evaluation.
- **Auditing and execution environment**: tool-call audit records now carry `call_id`, the execution outcome, and the turn `trace_id`, and record the tool's declared execution environment (`ExecutionEnvironment`), aligned with the `tool/call` event name; rejected, over-limit, and not-started calls are audited too.
- **Skill version observability**: `skill/load` events record the skill's current version, `SessionMetrics` breaks down turn success by `skill@version`, and `blockcell session replay` shows skill versions and per-turn statistics.
- **Subagent budget attribution**: LLM spend from subagents is folded into the originating session's budget, so background agents can no longer bypass global cost control.

### Changed

- **Toolchain pinned to Rust 1.96.0**: added `rust-toolchain.toml` and raised `rust-version` from 1.85 to 1.96; the Docker builder image and release workflow now use 1.96.0 as well (previously a drifting `rust:1` and 1.92.0 respectively). The old 1.85 floor was not buildable — `home` / `ignore` / `image` already require 1.88.
- Hooks and WebSocket tool events now feed into the EventBus, keeping the existing `hooks.yaml` configuration compatible.
- The ToolRegistry is now backed by the unified scope, so tools can be registered per agent / session / evolution candidate, with the more specific scope overriding on name conflicts.
- Ghost turn review now consumes the Session Event Log instead of inferring tool outcomes, memory writes, and model errors from the transcript.
- Session event-log writes moved off the async runtime (`spawn_blocking`), and listeners now default to a 30s timeout.
- Lock files are now long-lived synchronization anchors, avoiding the mutex being broken by `unlink`.
- `create_docx` now generates DOCX natively via `docx-rs` instead of relying on python-docx.
- **Execution environment decoupled from admission**: `exec` / `exec_local` / `exec_skill_script` now honestly declare `FullAccess` (running without a sandbox but without per-call confirmation), while dangerous commands remain gated by `DangerousOperation` on content; only `shell` with `sandbox_policy=full-access` asks for confirmation each time.
- **Per-message runtime derivation**: a lightweight per-message executor is derived from the long-lived runtime, sharing the response cache, evolution workers, learning coordinator, authorized directories, and budget trackers while isolating abort / steering / turn state, eliminating per-message disk rebuilds.
- **Layer 4 MemorySystem is now session-scoped and shared**, keeping cross-turn tracking state.
- **Policy engine precedence is now Deny > Ask > Allow** (declaration order within a level) instead of first-match-wins.
- **Forked tool permissions default to fail-closed**: when `can_use_tool` is not supplied explicitly, the allowlist is derived from the fork's declared `tool_schemas`.
- **Blocking primitives wrapped uniformly**: synchronous SQLite and cross-process locks no longer run directly inside `async fn` (`run_store_blocking` / `run_blocking`); tools that open their own SQLite now set `WAL` and `busy_timeout`; cron's `operation_lock` only guards short critical sections.
- **External input handling converged on shared primitives**: Python string escaping, filename sanitization, `~` path expansion, channel path/filename sanitization, response-body size limits, SSRF validation, and config secret masking/restoration are no longer reimplemented per module.

### Performance

- The main loop prefetches read-only tool calls concurrently (`read_file` / `list_dir` / `glob` / `grep` / `rg` / `code_search` / `session_search` / `web_fetch` / `web_search`) and skips tool-body execution on a hit, overlapping independent I/O latency.
- The MemorySystem pool now evicts by LRU and is shared per session, so long-lived processes no longer grow linearly.
- Vector search reuses the id map instead of decoding the whole table on every query.
- Hot-path deduplication: BFS statement reuse and merged brief queries; `reload_skills` gained a fingerprint gate so unchanged skills are not rescanned.
- Prompt-cache alignment: `prompt_cache_key` now covers the tools and model fingerprints, the fourth Anthropic cache breakpoint is aligned, and a `fork_alignment_misses` metric was added.

### Security

- Fixed a Python script injection through unescaped style fields in `chart_generate`; nested tool calls inside skill scripts no longer bypass admission checks (nested contexts cannot prompt, so `ask` is treated as a rejection).
- Fixed `path_glob` path traversal, `exec_local` / `exec_skill_script` path escapes, authorized-root escapes, and missing admin permissions.
- Fixed an updater signature-verification bypass and SSRF-defense bypasses in `network_monitor` / `video_process`; all outbound HTTP now goes through `safe_client_builder()` + `ensure_url_allowed()` + a redirect policy.
- Fixed plaintext credential echo in the gateway and CLI (unified masking plus write-side restoration), raw-key encryption leaking into argv, overly permissive `.env` permissions with empty templates overwriting real environment variables, and the `encrypt` tool's IV persistence and algorithm declaration.
- Tightened sandbox and command-execution boundaries: fixed Seatbelt sensitive-read `deny` being overridden by a global `allow`, cross-channel interference between persistent shell sessions, missed `full-access` declarations when reusing a session, and double confirmation with wrong attribution for dangerous commands.
- Added resource limits for IM inbound media downloads, OOXML decompression, CSV parsing, Whisper uploads, tool response bodies, and non-UTF-8 safety scanning.

### Fixed

- Concurrency and consistency: lost updates on concurrent edits, transaction upgrades, `fs2` file locks, cross-process exclusion for the audit hash chain, mutual exclusion for evolution triggers, cross-process merge-and-save for the capability registry, a shared cross-process lock file between `toggles` and the gateway, and confirmation-queue send timeouts plus replay after capacity exhaustion.
- Memory and knowledge base: `knowledge_forget` now writes a tombstone per item and rolls back only that tombstone on delete failure, per-database forgets are serialized by a file lock, `memory restore` replaces entries with the same `dedup_key`, memory body length is capped, FTS is version-gated, and dedup migration runs once.
- Scheduling: cron period overflow, 7-field expressions, DST-gap computation, and long tasks moved out of `operation_lock`.
- Skills: fingerprints now include a streaming content hash (catching same-length, same-mtime rewrites), directory symlinks are not recursed into, `activate_skill` snapshots are paired, and capability history collisions are fixed.
- Storage and backups: old backups are evicted only after a successful copy, partial artifacts are cleaned up on failure, and the audit lock gained a timeout.
- Configuration: JSON5 environment-variable injection and comment ordering are preserved, unknown keys are kept, `config set` no longer silently drops keys, and `alerts` rule files are written concurrently safely.
- Providers and channels: Gemini default routing 404, `sender_role` passthrough, notification `security.mainSessionKey` can name a trusted target, and typed subagents reuse the parent `memory_file_store`.
- Runtime: malformed tool-argument logs no longer keep the raw prefix; `exec_local` picks an interpreter only before execution, and runtime errors or timeouts no longer trigger a re-run; text approvals match on the tool plus the full JSON arguments; the prefetch budget is tighter; `PromptOnly` covers override directives that omit the previous/system qualifier (still heuristic, not a complete injection defense).
- Disabled selection of the incomplete Linux Landlock backend until read isolation is verified, falling back to explicit approval; no claim of native Linux read isolation.
- Tool safety checks now apply to the final arguments after a listener rewrites them, closing the hole where argument rewriting bypassed policy, dangerous-operation, and path checks.
- Release builds keep panic unwinding, restoring panic isolation for listeners and task supervision.
- Clearing a session now deletes the event log under lock and cleans up the lock file.
- Step numbering is unique within a turn and every step now has a `step/end`; an abnormally exiting turn now emits `turn/end`.
- Tool events are consistently attributed to the persisted session, and forced skill-routing turns re-emit `turn/end`.
- The capability registry now only tolerates skipping the removed `dynamic_library` entries; other deserialization failures still error out.

### Tests and gates

- Added the local gate `make test` (`cargo fmt --all --check` + `cargo test --workspace` + coding-eval + three infrastructure scripts), with `make hooks` installing the `pre-push` hook.
- Fixed a false pass in coding-eval acceptance where zero tests passing counted as success.
- Added protocol-level MCP client tests (real stdio JSON-RPC), cron timezone / DST / 6- and 7-field tests, updater ed25519 signature and version-comparison tests, evolution deployment-chain tests (generate → audit → compile → contract → deploy → observe → rollback), four-path skill hot-reload regression tests, and prompt-cache alignment regressions.
- Sandbox-escalation tests are now deterministically driven instead of depending on host state; migration tests now assert fail-closed behavior.
- `cargo fmt` and `clippy -D warnings` are clean across the repository.

### Docs

- Synced `docs/03`, `09`, `12`, `17`, and `20` with the implementation (including `-v` wiring and pool naming).
- Development-process documents (review reports, fix plans, decomposition drafts) no longer enter version control; only formal design docs are kept.

### Removed

- Removed the native dynamic-library capability evolution path.


## [0.1.8] - 2026-10-22

This release runs on two tracks: the **Agent Runtime architecture evolution** (absorbing the composable / observable / interceptable / replayable design of a next-generation Agent Runtime while preserving Self-Evolution and Ghost Learning), and a **repo-wide security and correctness close-out** (multiple review passes across every crate, converging tool admission, sandbox boundaries, and path/credential handling onto shared primitives). The goal is to move from "it runs" to "explainable, replayable, auditable".

### Added

- **Runtime Kernel**: added an event bus (`EventBus`), a serial interception chain (`Waterfall`), and unified scope primitives (`ScopedRegistry` / `ScopeGuard`). Events are dispatched by priority and scope filtering, and listeners support timeout and failure isolation (fail-open / fail-closed).
- **Agent lifecycle events**: added `agent/pre-step`, `agent/request`, `agent/request-error`, `turn/start`, `turn/end`, `step/start`, `step/end`, `tool/pre-execute`, `tool/post-execute`, `tool/result`, `model/response`, `assistant/attempt`, `memory/write`, `subagent/spawn`, `subagent/completed`, `evolution/deployed`, `compaction/start`, and `compaction/end`, classified by Durable / Live / Capability consistency semantics.
- **Guarded Tool Pipeline**: centralized tool toggles, policy, dangerous-operation, path-access, and shell-escalation checks into one pipeline; individual tools no longer handle permissions, approvals, or auditing themselves.
- **Failure semantics**: added `FailureKind`, `Idempotency`, and `ToolOutcome` to distinguish failed / rejected / cancelled / timed-out / not-started / unknown; unknown outcomes are reconciled before retry instead of being retried automatically. Model request errors now pick a recovery strategy by type (compact-then-retry on context overflow, backoff on rate limit, provider switch on auth failure).
- **Session Event Log**: added an append-only session event log (JSONL sidecar, `schema_version`, monotonic `seq`, retention cleanup), with the transcript kept as the primary store and the event log running alongside it incrementally.
- **Session replay and evaluation**: added `blockcell session list / replay / verify` to replay turns, steps, model requests, and tool calls from the event log; added `SessionMetrics` for turn / step / tool-outcome / retry / token / duration metrics.
- **PromptRegistry**: the system prompt is now assembled from pluggable `PromptSection` registrations with scope override, template-variable resolution, and integrity checks; added `PromptContribution` and trust levels so learned skills and candidate behavior are injected behind a low-trust boundary.
- **Agent Loop contract**: separated the `AgentLoop` contract from the default loop implementation so the way a turn is driven can be replaced.
- **Capability seams**: added `SandboxPolicy` / `SandboxProvider` (native macOS Seatbelt / Linux Landlock+seccomp, falling back to approval when unavailable), `ShellExecutor`, `FilesystemBackend` (`LocalFilesystem`), `BrowserBackend` (`CdpBrowserBackend`), and `SubagentProvider` (a unified contract for fork and typed agents, with handles exposing status / cancel / result).
- **Three-layer tool definition**: `ToolDefinition` = `ToolSchema` (model-facing) + `ToolExecutionSpec` (permissions and idempotency) + `ToolPresentation` (presentation), decoupling presentation from execution permissions.
- **Compaction abstraction**: compaction is abstracted into a `ContextCompactor` strategy (`ModelSummaryCompactor` / `ExtractiveCompactor`).
- **`agent/pre-step` decision gate**: consolidates budget, compaction, and context injection, and supports a registrable policy chain that can rewrite or stop the step.
- **Evolution isolation**: added the Evolution Candidate Scope and an evolution trust boundary; candidate skills / prompts / policies are visible only inside a view that explicitly enters candidate evaluation.
- **Auditing and execution environment**: tool-call audit records now carry `call_id`, the execution outcome, and the turn `trace_id`, and record the tool's declared execution environment (`ExecutionEnvironment`), aligned with the `tool/call` event name; rejected, over-limit, and not-started calls are audited too.
- **Skill version observability**: `skill/load` events record the skill's current version, `SessionMetrics` breaks down turn success by `skill@version`, and `blockcell session replay` shows skill versions and per-turn statistics.
- **Subagent budget attribution**: LLM spend from subagents is folded into the originating session's budget, so background agents can no longer bypass global cost control.

### Changed

- **Toolchain pinned to Rust 1.96.0**: added `rust-toolchain.toml` and raised `rust-version` from 1.85 to 1.96; the Docker builder image and release workflow now use 1.96.0 as well (previously a drifting `rust:1` and 1.92.0 respectively). The old 1.85 floor was not buildable — `home` / `ignore` / `image` already require 1.88.
- Hooks and WebSocket tool events now feed into the EventBus, keeping the existing `hooks.yaml` configuration compatible.
- The ToolRegistry is now backed by the unified scope, so tools can be registered per agent / session / evolution candidate, with the more specific scope overriding on name conflicts.
- Ghost turn review now consumes the Session Event Log instead of inferring tool outcomes, memory writes, and model errors from the transcript.
- Session event-log writes moved off the async runtime (`spawn_blocking`), and listeners now default to a 30s timeout.
- Lock files are now long-lived synchronization anchors, avoiding the mutex being broken by `unlink`.
- `create_docx` now generates DOCX natively via `docx-rs` instead of relying on python-docx.
- **Execution environment decoupled from admission**: `exec` / `exec_local` / `exec_skill_script` now honestly declare `FullAccess` (running without a sandbox but without per-call confirmation), while dangerous commands remain gated by `DangerousOperation` on content; only `shell` with `sandbox_policy=full-access` asks for confirmation each time.
- **Per-message runtime derivation**: a lightweight per-message executor is derived from the long-lived runtime, sharing the response cache, evolution workers, learning coordinator, authorized directories, and budget trackers while isolating abort / steering / turn state, eliminating per-message disk rebuilds.
- **Layer 4 MemorySystem is now session-scoped and shared**, keeping cross-turn tracking state.
- **Policy engine precedence is now Deny > Ask > Allow** (declaration order within a level) instead of first-match-wins.
- **Forked tool permissions default to fail-closed**: when `can_use_tool` is not supplied explicitly, the allowlist is derived from the fork's declared `tool_schemas`.
- **Blocking primitives wrapped uniformly**: synchronous SQLite and cross-process locks no longer run directly inside `async fn` (`run_store_blocking` / `run_blocking`); tools that open their own SQLite now set `WAL` and `busy_timeout`; cron's `operation_lock` only guards short critical sections.
- **External input handling converged on shared primitives**: Python string escaping, filename sanitization, `~` path expansion, channel path/filename sanitization, response-body size limits, SSRF validation, and config secret masking/restoration are no longer reimplemented per module.

### Performance

- The main loop prefetches read-only tool calls concurrently (`read_file` / `list_dir` / `glob` / `grep` / `rg` / `code_search` / `session_search` / `web_fetch` / `web_search`) and skips tool-body execution on a hit, overlapping independent I/O latency.
- The MemorySystem pool now evicts by LRU and is shared per session, so long-lived processes no longer grow linearly.
- Vector search reuses the id map instead of decoding the whole table on every query.
- Hot-path deduplication: BFS statement reuse and merged brief queries; `reload_skills` gained a fingerprint gate so unchanged skills are not rescanned.
- Prompt-cache alignment: `prompt_cache_key` now covers the tools and model fingerprints, the fourth Anthropic cache breakpoint is aligned, and a `fork_alignment_misses` metric was added.

### Security

- Fixed a Python script injection through unescaped style fields in `chart_generate`; nested tool calls inside skill scripts no longer bypass admission checks (nested contexts cannot prompt, so `ask` is treated as a rejection).
- Fixed `path_glob` path traversal, `exec_local` / `exec_skill_script` path escapes, authorized-root escapes, and missing admin permissions.
- Fixed an updater signature-verification bypass and SSRF-defense bypasses in `network_monitor` / `video_process`; all outbound HTTP now goes through `safe_client_builder()` + `ensure_url_allowed()` + a redirect policy.
- Fixed plaintext credential echo in the gateway and CLI (unified masking plus write-side restoration), raw-key encryption leaking into argv, overly permissive `.env` permissions with empty templates overwriting real environment variables, and the `encrypt` tool's IV persistence and algorithm declaration.
- Tightened sandbox and command-execution boundaries: fixed Seatbelt sensitive-read `deny` being overridden by a global `allow`, cross-channel interference between persistent shell sessions, missed `full-access` declarations when reusing a session, and double confirmation with wrong attribution for dangerous commands.
- Added resource limits for IM inbound media downloads, OOXML decompression, CSV parsing, Whisper uploads, tool response bodies, and non-UTF-8 safety scanning.

### Fixed

- Concurrency and consistency: lost updates on concurrent edits, transaction upgrades, `fs2` file locks, cross-process exclusion for the audit hash chain, mutual exclusion for evolution triggers, cross-process merge-and-save for the capability registry, a shared cross-process lock file between `toggles` and the gateway, and confirmation-queue send timeouts plus replay after capacity exhaustion.
- Memory and knowledge base: `knowledge_forget` now writes a tombstone per item and rolls back only that tombstone on delete failure, per-database forgets are serialized by a file lock, `memory restore` replaces entries with the same `dedup_key`, memory body length is capped, FTS is version-gated, and dedup migration runs once.
- Scheduling: cron period overflow, 7-field expressions, DST-gap computation, and long tasks moved out of `operation_lock`.
- Skills: fingerprints now include a streaming content hash (catching same-length, same-mtime rewrites), directory symlinks are not recursed into, `activate_skill` snapshots are paired, and capability history collisions are fixed.
- Storage and backups: old backups are evicted only after a successful copy, partial artifacts are cleaned up on failure, and the audit lock gained a timeout.
- Configuration: JSON5 environment-variable injection and comment ordering are preserved, unknown keys are kept, `config set` no longer silently drops keys, and `alerts` rule files are written concurrently safely.
- Providers and channels: Gemini default routing 404, `sender_role` passthrough, notification `security.mainSessionKey` can name a trusted target, and typed subagents reuse the parent `memory_file_store`.
- Runtime: malformed tool-argument logs no longer keep the raw prefix; `exec_local` picks an interpreter only before execution, and runtime errors or timeouts no longer trigger a re-run; text approvals match on the tool plus the full JSON arguments; the prefetch budget is tighter; `PromptOnly` covers override directives that omit the previous/system qualifier (still heuristic, not a complete injection defense).
- Disabled selection of the incomplete Linux Landlock backend until read isolation is verified, falling back to explicit approval; no claim of native Linux read isolation.
- Tool safety checks now apply to the final arguments after a listener rewrites them, closing the hole where argument rewriting bypassed policy, dangerous-operation, and path checks.
- Release builds keep panic unwinding, restoring panic isolation for listeners and task supervision.
- Clearing a session now deletes the event log under lock and cleans up the lock file.
- Step numbering is unique within a turn and every step now has a `step/end`; an abnormally exiting turn now emits `turn/end`.
- Tool events are consistently attributed to the persisted session, and forced skill-routing turns re-emit `turn/end`.
- The capability registry now only tolerates skipping the removed `dynamic_library` entries; other deserialization failures still error out.

### Tests and gates

- Added the local gate `make test` (`cargo fmt --all --check` + `cargo test --workspace` + coding-eval + three infrastructure scripts), with `make hooks` installing the `pre-push` hook.
- Fixed a false pass in coding-eval acceptance where zero tests passing counted as success.
- Added protocol-level MCP client tests (real stdio JSON-RPC), cron timezone / DST / 6- and 7-field tests, updater ed25519 signature and version-comparison tests, evolution deployment-chain tests (generate → audit → compile → contract → deploy → observe → rollback), four-path skill hot-reload regression tests, and prompt-cache alignment regressions.
- Sandbox-escalation tests are now deterministically driven instead of depending on host state; migration tests now assert fail-closed behavior.
- `cargo fmt` and `clippy -D warnings` are clean across the repository.

### Docs

- Synced `docs/03`, `09`, `12`, `17`, and `20` with the implementation (including `-v` wiring and pool naming).
- Development-process documents (review reports, fix plans, decomposition drafts) no longer enter version control; only formal design docs are kept.

### Removed

- Removed the native dynamic-library capability evolution path.

## [0.1.7] - 2026-06-28


### Added
- Added ModelRouter intelligent routing and connection-phase fallback with `manual`, `cost_optimized`, `quality_first`, and `latency_first` strategies.
- Added the Tool Policy execution policy system with tool-name globs, `|` multi-patterns, channel/path conditions, `allow` / `ask` / `deny` decisions, inherited rule groups, and simulation mode.
- Added global token and cost budget controls with per-session LLM usage tracking to prevent runaway tasks.
- Added the Steering Channel for injecting user messages into an active agent turn while it is still running.
- Added Agent lifecycle hooks for `session_start`, `user_prompt`, `pre_tool_use`, `post_tool_use`, and `agent_stop`, configurable through `~/.blockcell/hooks.yaml`.
- Added SHA-256 hash-chain verification for audit logs, plus SessionStart, SessionEnd, ProviderCall, and BudgetEvent audit events.
- Added on-demand MCP tool discovery: when many MCP tools are allowed, the model sees `mcp_search_tools` while remote tools stay executable but hidden from the default system prompt.
- Added GitHub Actions CI and multi-platform release build workflow.

### Changed
- Updated 2026-06 provider and default model presets across DeepSeek, OpenAI, Anthropic, Gemini, Ollama, GLM, Qwen, and related model families.
- Switched WebUI pages to `React.lazy` + `Suspense` chunks and improved the system-events button layout.
- Reworked Gateway event broadcasting around lightweight `WsEventRouting`, avoiding route-time copies of large content/token payloads.
- Reduced repeated token estimation, history cloning, and per-line allocation in streaming/read paths.
- Made Ghost config hot reload and Cron sync skip full disk reads and parsing when config mtime is unchanged.
- Moved SQLite memory query/write/delete and maintenance operations into `spawn_blocking` to reduce Tokio worker blocking.
- Continued splitting large modules by responsibility across runtime, agent, gateway, config, openai, memory, evolution, and consolidator code.
- Standardized the workspace on Rust 1.85.

### Fixed
- Fixed Gateway API token exposure through URL `?token=` on regular APIs; query tokens are now limited to WebSocket and required file download/serve paths.
- Fixed WebSocket / outbound broadcast session isolation issues and prevented one WS client from approving another session's pending confirmation request.
- Fixed file read/upload size limits, local image serve boundaries, multibyte UTF-8 URL decoding, CLI Unicode cursor editing, and Home/End/Delete key handling.
- Fixed `http_request` SSRF risk, `exec_local` / `exec_skill_script` path escapes, dangerous `rm` command bypasses, timeout orphan processes, and symlink write escapes.
- Fixed Ghost review TOCTOU/path traversal issues, main-flow blocking, throttle leakage, and accidental scanning of `.snapshots` / `.skill_file_store.lockdir`.
- Fixed concurrency, atomicity, transaction commit, recovery, lock-release, budget, and Unicode panic issues across Dream / Session / Auto / Compact memory paths.
- Fixed skill evolution and core evolution state-machine, rollback, staging-path, cross-workflow contamination, infinite retry, Python syntax check, and code-fence injection issues.
- Fixed OpenAI-compatible streaming tool calls, MCP short-name tool loading, hash algorithm use, browser navigation races, CDP listener leaks, and semver prerelease parsing.
- Fixed Windows atomic replacement, path separators, temporary cleanup, cross-process lock deletion, config loss, and schema consistency issues.

### Docs
- Added ModelRouter routing and fallback documentation.
- Added Hook lifecycle event system documentation.
- Updated README / README.en with v0.1.7 security, audit, routing, and hook capabilities.
- Added v0.1.7 release notes.


## [0.1.8] - 2026-10-22

本版本由两条主线组成：**Agent Runtime 架构演进**（吸收新一代 Agent Runtime 的可组合、可观察、可拦截、可回放设计，同时保留 Self-Evolution / Ghost Learning 能力），以及**全仓安全与正确性收口**（覆盖全部 crate 的多轮审查，把工具准入、沙箱边界、路径与凭据处理收敛到统一原语）。目标是把“能跑”提升到“可解释、可回放、可审计”。

### 新增

- **Runtime Kernel**：新增事件总线（`EventBus`）、串行拦截链（`Waterfall`）与统一作用域原语（`ScopedRegistry` / `ScopeGuard`），事件按优先级、作用域过滤派发，listener 支持超时与故障隔离（fail-open / fail-closed）。
- **Agent 生命周期事件**：新增 `agent/pre-step`、`agent/request`、`agent/request-error`、`turn/start`、`turn/end`、`step/start`、`step/end`、`tool/pre-execute`、`tool/post-execute`、`tool/result`、`model/response`、`assistant/attempt`、`memory/write`、`subagent/spawn`、`subagent/completed`、`evolution/deployed`、`compaction/start`、`compaction/end`，并按 Durable / Live / Capability 分类一致性语义。
- **Guarded Tool Pipeline**：把工具开关、Policy、危险操作、路径访问、Shell 提权等安全检查集中到统一管线，具体工具不再自行处理权限、审批与审计。
- **失败语义**：新增 `FailureKind`、`Idempotency` 与 `ToolOutcome`，区分失败 / 被拒绝 / 取消 / 超时 / 未开始 / 未知；结果未知时不自动重试而是先对账；模型请求错误按类型决定恢复策略（上下文超限先压缩再重试、限流退避、认证失败换 provider）。
- **Session Event Log**：新增追加式会话事件日志（JSONL 侧车、`schema_version`、单调 `seq`、保留期清理），transcript 仍为主存储、事件日志渐进并行。
- **会话回放与评估**：新增 `blockcell session list / replay / verify`，从事件日志重放回合、步骤、模型请求与工具调用；新增 `SessionMetrics` 计算回合 / 步骤 / 工具结论 / 重试 / token / 时长等评估指标。
- **PromptRegistry**：系统提示词改为可插拔 `PromptSection` 注册表拼装，支持作用域覆盖、模板变量解析与完整性校验；新增 `PromptContribution` 与信任级别，学习技能与候选行为以低信任边界注入。
- **Agent Loop 契约**：`AgentLoop` 与默认 Loop 实现分离，一次 turn 的驱动方式可替换。
- **能力 seam**：新增 `SandboxPolicy` / `SandboxProvider`（本机 macOS Seatbelt / Linux Landlock+seccomp，不可用时要求审批）、`ShellExecutor`、`FilesystemBackend`（`LocalFilesystem`）、`BrowserBackend`（`CdpBrowserBackend`）、`SubagentProvider`（fork 与 typed agent 统一契约，句柄提供状态 / 取消 / 结果）。
- **Tool 三层定义**：`ToolDefinition` = `ToolSchema`（模型可见）+ `ToolExecutionSpec`（权限与幂等声明）+ `ToolPresentation`（展示），展示层与执行权限解耦。
- **压缩抽象**：压缩摘要抽象为 `ContextCompactor` 策略（`ModelSummaryCompactor` / `ExtractiveCompactor`）。
- **`agent/pre-step` 决策门**：收拢预算、压缩与上下文注入，并支持可注册策略链改写或终止本步。
- **进化隔离**：新增 Evolution Candidate Scope 与进化信任边界，候选技能 / 提示 / 策略只对显式进入候选评估的视图可见。
- **审计与执行环境**：工具调用审计携带 `call_id`、执行结论与回合 `trace_id`，并记录工具声明的执行环境（`ExecutionEnvironment`），与 `tool/call` 事件同名对齐；被拒绝、超出上限、未开始的调用同样写入审计。
- **技能版本可观测**：`skill/load` 事件记录技能当前版本，`SessionMetrics` 按 `技能@版本` 统计回合成败，`blockcell session replay` 展示技能版本与回合统计。
- **子 Agent 预算归并**：子 Agent 的 LLM 消耗归并到发起会话的预算，后台代理不再绕过全局成本控制。

### 调整

- **工具链固定为 Rust 1.96.0**：新增 `rust-toolchain.toml`，`rust-version` 由 1.85 提到 1.96；Docker 构建镜像与发布工作流同步到 1.96.0（此前分别是漂移的 `rust:1`、1.92.0）。原 1.85 下限实际不可构建——`home` / `ignore` / `image` 已要求 1.88。
- Hook 与 WebSocket 工具事件统一接入 EventBus，保留原 `hooks.yaml` 配置兼容。
- ToolRegistry 接入统一 Scope，工具可按 agent / session / 进化候选作用域注册，同名时更具体作用域覆盖。
- Ghost 回合复盘改为消费 Session Event Log，工具结论 / 记忆写入 / 模型错误不再从 transcript 推断。
- 事件日志写入移出 async 线程（`spawn_blocking`），listener 默认 30s 超时。
- 锁文件改为长期同步锚点，避免 unlink 破坏互斥。
- `create_docx` 改用 `docx-rs` 原生生成，不再依赖 python-docx。
- **执行环境与准入解耦**：`exec` / `exec_local` / `exec_skill_script` 如实声明 `FullAccess`（无沙箱运行但不逐次确认），危险命令仍由 `DangerousOperation` 按内容把关；只有 `sandbox_policy=full-access` 的 `shell` 需要逐次确认。
- **每消息 runtime 派生**：由长生命周期 runtime 派生轻量消息级执行器，共享响应缓存、进化 worker、学习协调器、授权目录与预算跟踪，隔离 abort / steering / turn，消除每消息的磁盘重建。
- **Layer 4 MemorySystem 提升为会话级共享**，保留跨轮追踪状态。
- **策略引擎优先级改为 Deny > Ask > Allow**（同级按声明顺序），不再是 first-match-wins。
- **fork 工具权限默认 fail-closed**：未显式提供 `can_use_tool` 时按 fork 声明的 `tool_schemas` 派生白名单。
- **阻塞原语统一包装**：同步 SQLite 与跨进程锁不再直接跑在 `async fn`（`run_store_blocking` / `run_blocking`）；工具自开 SQLite 补 `WAL` 与 `busy_timeout`；cron 的 `operation_lock` 只护短临界区。
- **外部输入处理收敛到共享原语**：Python 字符串转义、文件名净化、`~` 路径展开、渠道路径与文件名净化、响应体限读、SSRF 校验、配置密钥掩码与还原，不再各模块手写副本。

### 性能

- 主循环对只读工具调用并发预取（`read_file` / `list_dir` / `glob` / `grep` / `rg` / `code_search` / `session_search` / `web_fetch` / `web_search`），命中后跳过工具体执行，重叠彼此独立的 I/O 延迟。
- MemorySystem 池改为 LRU 淘汰并提升为会话级共享，长生命周期进程不再线性增长。
- 向量检索复用 id 映射，不再每次全表解码。
- 热路径去重：BFS 语句复用、brief 查询合并；`reload_skills` 增加指纹门控，未变更时不再重扫。
- 提示词缓存对齐：`prompt_cache_key` 纳入 tools 与 model 指纹，补齐 Anthropic 第四缓存断点，新增 `fork_alignment_misses` 指标。

### 安全

- 修复 `chart_generate` 样式字段未转义导致的 Python 脚本注入；技能脚本内嵌套工具调用不再绕过准入检查（嵌套上下文无法确认，`ask` 视为拒绝）。
- 修复 `path_glob` 路径穿越、`exec_local` / `exec_skill_script` 路径逃逸、授权根逃逸与管理权限缺失。
- 修复 updater 验签绕过，以及 `network_monitor` / `video_process` 绕过 SSRF 防线；出站 HTTP 统一走 `safe_client_builder()` + `ensure_url_allowed()` + 重定向策略。
- 修复网关与 CLI 明文回显凭据（统一掩码 + 写入侧还原）、raw-key 加密泄露到 argv、`.env` 权限过宽与空模板覆盖真实环境变量、`encrypt` 工具 IV 持久化与算法声明。
- 收紧沙箱与命令执行边界：修复 Seatbelt 敏感读 `deny` 被全局 `allow` 覆盖、持久 shell 会话跨通道串扰、复用会话时 `full-access` 声明漏判、危险命令双重确认与归属错误。
- 补资源上限：IM 入站媒体下载、OOXML 解压、CSV 解析、Whisper 上传、工具响应体大小、非 UTF-8 安全扫描。

### 修复

- 并发与一致性：并发编辑丢更新、事务升级、`fs2` 文件锁、审计哈希链跨进程互斥、进化触发互斥、能力注册表跨进程合并保存、`toggles` 与网关共享同一跨进程文件锁、确认队列发送侧超时与容量耗尽后的重放。
- 记忆与知识库：`knowledge_forget` 逐项先写墓碑并在删除失败时撤销本次墓碑、同库遗忘用文件锁互斥、`memory restore` 同 `dedup_key` 自动顶替、记忆正文长度上限、FTS 版本门控、改为一次性去重迁移。
- 调度：cron 周期溢出、7 字段表达式、DST 空档计算、长任务移出 `operation_lock`。
- 技能：指纹增加流式内容哈希（避免同长度同 mtime 改写漏检）、目录符号链接不递归、`activate_skill` 快照配对、capability 历史碰撞。
- 存储与备份：快照复制成功后再淘汰旧备份，失败时清理半成品；审计锁增加超时。
- 配置：JSON5 环境变量注入与注释顺序保留、未知键保留、`config set` 静默丢键、`alerts` 规则文件并发写。
- Provider 与渠道：Gemini 默认路由 404、`sender_role` 透传、通知 `security.mainSessionKey` 可指定可信目标、typed 子代理复用父 `memory_file_store`。
- 运行时：畸形工具参数日志不再保留原文前缀；`exec_local` 仅在执行前选择解释器，运行错误与超时不再触发重跑；文本审批按工具与完整 JSON 参数匹配；预取预算收紧；`PromptOnly` 覆盖缺少 previous/system 限定词的 override 指令（仍是启发式，不是完整注入防御）。
- 未验证读隔离前禁用不完整的 Linux Landlock 后端选择，回退到显式审批；不宣称实现 Linux 原生读隔离。
- 工具安全检查作用于 listener 改写后的最终参数，关闭参数改写绕过 Policy / 危险操作 / 路径检查的漏洞。
- release 构建保留 panic unwind，恢复 listener 与任务监管的 panic 隔离。
- 清空会话时持锁删除事件日志并清理锁文件。
- step 编号在回合内唯一且每个 step 都有 `step/end`；回合异常退出时补发 `turn/end`。
- 工具事件统一归属到持久化会话；强制技能路由回合补发 `turn/end`。
- 能力注册表仅兼容跳过已移除的 `dynamic_library` 条目，其余反序列化失败仍报错。

### 测试与门禁

- 新增本地门禁 `make test`（`cargo fmt --all --check` + `cargo test --workspace` + coding-eval + 三个基础设施脚本），`make hooks` 安装 `pre-push` 钩子。
- 修复 coding-eval 验收可“零测试通过”的假通过。
- 补齐 MCP client 协议级测试（真实 stdio JSON-RPC）、cron 时区 / DST / 6 与 7 字段、updater ed25519 验签与版本比较、evolution 部署链路（generate → audit → compile → contract → deploy → observe → rollback）、技能热更新四路径、提示词缓存对齐回归。
- 沙箱升级用例改为确定性驱动，不再依赖宿主状态；迁移测试改为断言 fail-closed 行为。
- 全仓 `cargo fmt` 与 `clippy -D warnings` 清零。

### 文档

- 同步 `docs/03`、`09`、`12`、`17`、`20` 与实现（含 `-v` 接线与 pool 命名）。
- 开发过程文档（审查报告、修复计划、分解稿）不再进入版本库，仅保留正式设计文档。

### 移除

- 移除原生动态库能力进化路径。

## [0.1.6] - 2026-05-09


### Added
- Added the Ghost Native learning loop, capturing reusable lessons at turn end, pre-compress, session rotation, session end, delegation end, and evolution success boundaries, then persisting durable knowledge into `USER.md`, `MEMORY.md`, and workspace skills.
- Added a Ghost learning ledger and background review loop for episode audit, review runs, and restricted tool actions without blocking the main assistant response.
- Added Typed Agent and custom Agent loading from user-level and project-level Markdown definitions, including tool scope, model, skill, MCP, one-shot, permission, and background execution settings.
- Added multi-agent task improvements: checkpoints, chained AbortToken cancellation, progress events, task persistence, and improved result injection.
- Added a unified slash-command system for `/help`, `/tasks`, `/skills`, `/tools`, `/learn`, `/clear`, `/compact`, `/session-metrics`, `/log`, and shared Gateway/Channel handling.
- Added 7-layer memory metrics, circuit breakers, and the `/session-metrics` observability entry point.
- Added 30+ configurable memory and compression thresholds in `config.json5`.
- Added OpenClaw skill parsing, gbrain skill compatibility, and the optional RabitQ vector index backend.

### Changed
- Upgraded the default DeepSeek path toward DeepSeek V4 Pro, including 1M context defaults, `reasoningEffort`, and thinking/reasoning request parameters.
- Improved WebUI streaming for thinking/reasoning content, connection state handling, LLM defaults, and agent progress events.
- Unified skill loading, CLI completion, Gateway skills/search, and SkillIndex around recursive skill pack/category scanning and meta-only skill visibility.
- Unified skill description loading as `meta.yaml`/`meta.json` → `SKILL.md` frontmatter → `SKILL.md` body, fixing YAML multiline descriptions and search ranking gaps.
- Migrated core evolution and skill evolution to durable workflows backed by SQLite workflow records, claim/lease, step checkpoints, and background workers.
- Kept SQLite + FTS5 as the canonical memory store while adding optional RabitQ semantic vector recall.
- Hardened the learning write path with unified security scanning, write guards, snapshots, and undo support.

### Fixed
- Fixed DeepSeek DSML tool-call parsing, cross-delta filtering, and missing `reasoning_content` errors.
- Fixed missing thinking/reasoning display, duplicated CLI output, and missing user/LLM log persistence.
- Fixed deep skill-pack category scanning, composite-name truncation, invisible child skills, and accidental empty-skill registration.
- Fixed core evolution ticks blocking the Agent select loop and delaying user input handling.
- Fixed multi-agent task lifecycle, forked subagent cancellation, result injection, task persistence, and Auto Memory trigger logic.
- Fixed Windows config atomic replacement, Python script execution, colon-containing session IDs, and versioning path separators.
- Fixed memory compaction divide-by-zero panics, UTF-8 boundary panics for Chinese text, config propagation gaps, SQL injection risks, and learning write-safety issues.

### Docs
- Added Ghost Native learning design documentation.
- Added design docs for unified slash commands, session metrics, OpenClaw compatibility, configurable memory thresholds, log commands, intent classifier/loadAllTools, and the integrated learning framework.
- Updated Ghost Maintenance docs to clarify its boundary with embedded Ghost learning.


## [0.1.7] - 2026-06-28

### Added
- Added ModelRouter intelligent routing and connection-phase fallback with `manual`, `cost_optimized`, `quality_first`, and `latency_first` strategies.
- Added the Tool Policy execution policy system with tool-name globs, `|` multi-patterns, channel/path conditions, `allow` / `ask` / `deny` decisions, inherited rule groups, and simulation mode.
- Added global token and cost budget controls with per-session LLM usage tracking to prevent runaway tasks.
- Added the Steering Channel for injecting user messages into an active agent turn while it is still running.
- Added Agent lifecycle hooks for `session_start`, `user_prompt`, `pre_tool_use`, `post_tool_use`, and `agent_stop`, configurable through `~/.blockcell/hooks.yaml`.
- Added SHA-256 hash-chain verification for audit logs, plus SessionStart, SessionEnd, ProviderCall, and BudgetEvent audit events.
- Added on-demand MCP tool discovery: when many MCP tools are allowed, the model sees `mcp_search_tools` while remote tools stay executable but hidden from the default system prompt.
- Added GitHub Actions CI and multi-platform release build workflow.

### Changed
- Updated 2026-06 provider and default model presets across DeepSeek, OpenAI, Anthropic, Gemini, Ollama, GLM, Qwen, and related model families.
- Switched WebUI pages to `React.lazy` + `Suspense` chunks and improved the system-events button layout.
- Reworked Gateway event broadcasting around lightweight `WsEventRouting`, avoiding route-time copies of large content/token payloads.
- Reduced repeated token estimation, history cloning, and per-line allocation in streaming/read paths.
- Made Ghost config hot reload and Cron sync skip full disk reads and parsing when config mtime is unchanged.
- Moved SQLite memory query/write/delete and maintenance operations into `spawn_blocking` to reduce Tokio worker blocking.
- Continued splitting large modules by responsibility across runtime, agent, gateway, config, openai, memory, evolution, and consolidator code.
- Standardized the workspace on Rust 1.85.

### Fixed
- Fixed Gateway API token exposure through URL `?token=` on regular APIs; query tokens are now limited to WebSocket and required file download/serve paths.
- Fixed WebSocket / outbound broadcast session isolation issues and prevented one WS client from approving another session's pending confirmation request.
- Fixed file read/upload size limits, local image serve boundaries, multibyte UTF-8 URL decoding, CLI Unicode cursor editing, and Home/End/Delete key handling.
- Fixed `http_request` SSRF risk, `exec_local` / `exec_skill_script` path escapes, dangerous `rm` command bypasses, timeout orphan processes, and symlink write escapes.
- Fixed Ghost review TOCTOU/path traversal issues, main-flow blocking, throttle leakage, and accidental scanning of `.snapshots` / `.skill_file_store.lockdir`.
- Fixed concurrency, atomicity, transaction commit, recovery, lock-release, budget, and Unicode panic issues across Dream / Session / Auto / Compact memory paths.
- Fixed skill evolution and core evolution state-machine, rollback, staging-path, cross-workflow contamination, infinite retry, Python syntax check, and code-fence injection issues.
- Fixed OpenAI-compatible streaming tool calls, MCP short-name tool loading, hash algorithm use, browser navigation races, CDP listener leaks, and semver prerelease parsing.
- Fixed Windows atomic replacement, path separators, temporary cleanup, cross-process lock deletion, config loss, and schema consistency issues.

### Docs
- Added ModelRouter routing and fallback documentation.
- Added Hook lifecycle event system documentation.
- Updated README / README.en with v0.1.7 security, audit, routing, and hook capabilities.
- Added v0.1.7 release notes.

## [0.1.5] - 2026-04-05


### Added
- Unified skill runtime with `rhai` script execution support.
- Skill versioning, auditing, evolution, and service management improvements.
- New Weixin, QQ, and NapCatQQ channel support.
- Memory vector indexing support.
- Expanded WebUI interaction for chat, evolution, and connection states.
- Additional docs for skills, CLI, provider configuration, MCP servers, and path access policy.

### Changed
- Simplified the skill execution model and history handling.
- Improved message/tool call flow and runtime consistency across core modules.
- Enhanced WeCom long-connection support and channel startup/status display.
- Optimized WebUI chat UX, message rendering, and frontend test coverage.
- Updated cron/timezone handling for scheduled tasks.

### Fixed
- Fixed provider compatibility issues and configuration edge cases.
- Fixed gateway/agent/scheduler stability issues and duplicated output problems.
- Fixed path parsing, default model invocation, and media handling bugs.
- Fixed WebUI lag when the gateway disconnects.
- Fixed memory rule inconsistencies across storage, tools, and gateway.

### Docs
- Added and updated a large set of Chinese and English docs for skills, channels, memory, provider configuration, and CLI usage.
- Added workflow/rules documentation for skill development.


## [0.1.7] - 2026-06-28

### 新增
- 新增 ModelRouter 智能路由与连接阶段自动降级，支持 `manual`、`cost_optimized`、`quality_first` 和 `latency_first` 路由策略。
- 新增 Tool Policy 执行策略系统，支持工具名 glob、`|` 多模式、渠道/路径条件、`allow` / `ask` / `deny` 决策、规则组继承和 simulation mode。
- 新增全局 Token / 成本预算控制，按会话跟踪 LLM 用量，避免长任务或异常循环失控消耗。
- 新增 Steering Channel 实时消息注入，允许在 Agent 执行期间继续接收用户指令并注入当前轮次。
- 新增 Agent 生命周期 Hook：`session_start`、`user_prompt`、`pre_tool_use`、`post_tool_use`、`agent_stop`，可通过 `~/.blockcell/hooks.yaml` 执行本机命令。
- 新增审计日志 SHA-256 hash chain 防篡改校验，并补充 SessionStart、SessionEnd、ProviderCall、BudgetEvent 等审计事件。
- 新增 MCP 工具按需发现：当允许的 MCP 工具数量较大时，仅向模型暴露 `mcp_search_tools`，远端工具保持可执行但默认不注入 system prompt。
- 新增 GitHub Actions CI 与多平台 Release 构建流程。

### 调整
- 更新 2026-06 Provider 与默认模型预设，包括 DeepSeek、OpenAI、Anthropic、Gemini、Ollama、GLM、Qwen 等模型族。
- WebUI 改用 `React.lazy` + `Suspense` 分块加载，并优化系统动态区域的按钮布局。
- Gateway 事件广播改为轻量 `WsEventRouting`，避免在路由阶段复制 content/token 等大字段。
- Token 估算与流式读取路径减少无缓存重复计算、历史深拷贝和单行缓冲分配。
- Ghost 配置热重载与 Cron sync 按 mtime 判断是否需要重读配置，减少不必要的读盘和解析。
- SQLite 记忆查询、写入、删除和 maintenance 操作迁移到 `spawn_blocking`，降低阻塞 Tokio worker 的风险。
- 大文件继续按职责拆分，覆盖 runtime、agent、gateway、config、openai、memory、evolution、consolidator 等模块。
- 全仓库 Rust 版本统一为 1.85。

### 修复
- 修复 Gateway API 普通接口接受 URL `?token=` 导致 token 进入访问日志或 Referer 的风险；仅保留 WebSocket 与文件下载/serve 等必要入口。
- 修复 WebSocket / outbound 广播缺少会话隔离、跨会话泄漏，以及任意 WS 客户端可批准其他会话确认请求的问题。
- 修复文件读取/上传缺少大小限制、本地图片 serve 边界、`url_decode` 多字节 UTF-8、CLI Unicode 光标编辑、Home/End/Delete 键处理等问题。
- 修复 `http_request` SSRF 风险、`exec_local` / `exec_skill_script` 路径逃逸、危险 `rm` 命令绕过、超时子进程残留和 symlink 写入逃逸。
- 修复 Ghost review TOCTOU、路径穿越、阻塞主流程、节流泄漏，以及 `.snapshots` / `.skill_file_store.lockdir` 被误扫描的问题。
- 修复 Dream / Session / Auto / Compact 记忆链路中的并发、原子性、事务提交、恢复、锁释放、预算和 Unicode panic 问题。
- 修复 skill evolution 与 core evolution 中状态机、回滚、staging 路径、跨流程污染、无限重试、Python 语法检查和代码围栏注入等安全稳定性问题。
- 修复 OpenAI 兼容 streaming 工具调用、MCP 短名工具加载、hash 算法使用、浏览器导航竞态、CDP 监听器泄漏和 semver 预发布解析问题。
- 修复 Windows 原子写入、路径分隔符、临时目录清理、跨进程锁误删、配置丢失和 schema 不一致等兼容性问题。

### 文档
- 新增 ModelRouter 智能路由与自动降级文档。
- 新增 Hook 生命周期事件系统文档。
- 更新 README / README.en，补充 v0.1.7 的安全、审计、路由和 Hook 能力说明。
- 新增 v0.1.7 Release 说明文档。

## [0.1.4] - 2026-03-25


### Added
- Initial public release for the current tracked line of BlockCell.
- Core agent, provider, storage, scheduler, channels, and skills workspace crates.
- Basic WebUI and gateway integration.


## [0.1.6] - 2026-05-09

### Added
- Added the Ghost Native learning loop, capturing reusable lessons at turn end, pre-compress, session rotation, session end, delegation end, and evolution success boundaries, then persisting durable knowledge into `USER.md`, `MEMORY.md`, and workspace skills.
- Added a Ghost learning ledger and background review loop for episode audit, review runs, and restricted tool actions without blocking the main assistant response.
- Added Typed Agent and custom Agent loading from user-level and project-level Markdown definitions, including tool scope, model, skill, MCP, one-shot, permission, and background execution settings.
- Added multi-agent task improvements: checkpoints, chained AbortToken cancellation, progress events, task persistence, and improved result injection.
- Added a unified slash-command system for `/help`, `/tasks`, `/skills`, `/tools`, `/learn`, `/clear`, `/compact`, `/session-metrics`, `/log`, and shared Gateway/Channel handling.
- Added 7-layer memory metrics, circuit breakers, and the `/session-metrics` observability entry point.
- Added 30+ configurable memory and compression thresholds in `config.json5`.
- Added OpenClaw skill parsing, gbrain skill compatibility, and the optional RabitQ vector index backend.

### Changed
- Upgraded the default DeepSeek path toward DeepSeek V4 Pro, including 1M context defaults, `reasoningEffort`, and thinking/reasoning request parameters.
- Improved WebUI streaming for thinking/reasoning content, connection state handling, LLM defaults, and agent progress events.
- Unified skill loading, CLI completion, Gateway skills/search, and SkillIndex around recursive skill pack/category scanning and meta-only skill visibility.
- Unified skill description loading as `meta.yaml`/`meta.json` → `SKILL.md` frontmatter → `SKILL.md` body, fixing YAML multiline descriptions and search ranking gaps.
- Migrated core evolution and skill evolution to durable workflows backed by SQLite workflow records, claim/lease, step checkpoints, and background workers.
- Kept SQLite + FTS5 as the canonical memory store while adding optional RabitQ semantic vector recall.
- Hardened the learning write path with unified security scanning, write guards, snapshots, and undo support.

### Fixed
- Fixed DeepSeek DSML tool-call parsing, cross-delta filtering, and missing `reasoning_content` errors.
- Fixed missing thinking/reasoning display, duplicated CLI output, and missing user/LLM log persistence.
- Fixed deep skill-pack category scanning, composite-name truncation, invisible child skills, and accidental empty-skill registration.
- Fixed core evolution ticks blocking the Agent select loop and delaying user input handling.
- Fixed multi-agent task lifecycle, forked subagent cancellation, result injection, task persistence, and Auto Memory trigger logic.
- Fixed Windows config atomic replacement, Python script execution, colon-containing session IDs, and versioning path separators.
- Fixed memory compaction divide-by-zero panics, UTF-8 boundary panics for Chinese text, config propagation gaps, SQL injection risks, and learning write-safety issues.

### Docs
- Added Ghost Native learning design documentation.
- Added design docs for unified slash commands, session metrics, OpenClaw compatibility, configurable memory thresholds, log commands, intent classifier/loadAllTools, and the integrated learning framework.
- Updated Ghost Maintenance docs to clarify its boundary with embedded Ghost learning.
