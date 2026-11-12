## What is TurboHermes / TurboHermes 是什么

TurboHermes is research code forked from [BlockCell](https://github.com/blockcell-labs/blockcell), a self-evolving Rust agent framework. We are not chasing upstream feature parity. Our direction in v9 is contract-first coding harness integration, bi-temporal world models, and deterministic offline prose tooling — things BlockCell was not designed for.

TurboHermes 是基于 [BlockCell](https://github.com/blockcell-labs/blockcell) fork 的研究代码。我们不追求 upstream 功能对等。v9 方向是 contract-first coding harness 集成、双时态世界模型、以及确定性离线 prose 工具链 —— 这些都不是 BlockCell 原本的设计目标。

> **Status:** Research code. Breaking changes are expected. Production use requires pinning a commit.

> **状态：** 研究代码。预期会有 breaking changes。生产使用请锁定特定 commit。

---


## 🌟 What Makes BlockCell Different


BlockCell isn't just another chatbot — it's an AI agent that **actually executes tasks**. While ChatGPT can only tell you what to do, BlockCell can:

- 📁 Read and write files on your system
- 🌐 Control browsers and automate web tasks
- 📊 Analyze Excel/PDF files and generate reports
- 💰 Monitor stock prices and crypto markets
- 📧 Send emails and messages across platforms
- 🔄 **Evolve itself** — automatically fix bugs and deploy improvements

```
You: "Monitor Tesla stock and alert me if it drops below $200"
BlockCell: ✓ Sets up monitoring → ✓ Checks price every hour → ✓ Sends Telegram alert
```

---


## 🌟 BlockCell 有何不同

TurboHermes 不只是一个聊天机器人 — 它是一个**真正能执行任务**的 AI 智能体。当 ChatGPT 只能告诉你该做什么时，BlockCell 可以：

- 📁 读写你系统上的文件
- 🌐 控制浏览器并自动化网页任务
- 📊 分析 Excel/PDF 文件并生成报表
- 💰 监控股票价格和加密货币市场
- 📧 跨平台发送邮件和消息
- 🔄 **自我进化** — 自动修复 bug 并部署改进

```
你："监控特斯拉股价，如果跌破 200 美元就提醒我"
BlockCell: ✓ 设置监控 → ✓ 每小时检查价格 → ✓ 发送 Telegram 提醒
```

---

## 🎯 The Name


> *"Simplest units, most complex whole."*

**BlockCell** is inspired by the **Replicators** from *Stargate* — mechanical life forms built from countless tiny, independent blocks. Each block is simple, but together they form ships, soldiers, and minds. They adapt instantly, evolve faster than any weapon can counter, and cannot be destroyed.

That philosophy lives in this framework:

- **Block** → Immutable Rust host: secure, stable, deterministic
- **Cell** → Mutable skills layer: living, self-repairing, endlessly evolving

Traditional software dies the moment it ships. BlockCell is meant to be **alive**.

→ [Full naming story](https://blockcell.dev/naming-story)

---


## 🎯 名字由来

> *"极简的单元，极繁的整体。"*

**BlockCell** 的灵感来自《星际之门》中的**复制者（Replicators）** — 由无数微小、独立的模块块组成的机械生命体。每个模块本身很简单，但组合在一起就能形成战舰、士兵和智慧。它们瞬间适应，进化速度超过任何武器，永远无法被摧毁。

这种哲学贯穿于整个框架：

- **Block** → 不可变的 Rust 宿主：安全、稳定、确定性
- **Cell** → 可变的技能层：有生命、能自我修复、无限进化

传统软件在发布的那一刻就停止了生长。TurboHermes 是**活的**。

→ [完整命名故事](https://blockcell.dev/naming-story)  （BlockCell 命名故事；TurboHermes 继承此哲学）

---

## ✨ Key Features


### 🛠️ 60+ Built-in Tools

- **Files & System**: Read/write files, execute commands, process Excel/Word/PDF
- **Web & Browser**: Web scraping, headless Chrome automation (CDP), HTTP requests
- **Finance**: Real-time stock quotes (CN/HK/US), crypto prices, DeFi data
- **Communication**: Email (SMTP/IMAP), Telegram, Slack, Discord, Feishu
- **Media**: Screenshots, speech-to-text (Whisper), chart generation, Office file creation
- **AI**: Image understanding, text-to-speech, OCR

### 🧬 Self-Evolution System

When the AI repeatedly fails at a task, BlockCell can:

1. Detect the error pattern
2. Generate improved code using LLM
3. Automatically audit, compile, and test
4. Deploy via canary rollout (10% → 50% → 100%)
5. Auto-rollback if performance degrades

```
Error detected → LLM generates fix → Audit → Test → Canary deploy → Full rollout
                                                    ↓ on failure
                                                 Auto rollback
```

### 🧠 Ghost Native Learning

BlockCell can retain durable lessons from real usage:

- Stable user preferences go into `USER.md`
- Project facts, environment conventions, and recurring lessons go into `MEMORY.md`
- Ghost background review only writes declarative memory; reusable workflows become workspace skills through the main agent or the separate Skill Learning channel
- Background review failures do not block the current answer, and automatic writes are audited, snapshotted, and undoable

### 🧭 ModelRouter and Connection-Phase Fallback

When multiple models are configured, BlockCell can choose the right provider by policy:

- `manual`: keep Provider Pool priority and weighted selection
- `cost_optimized`: prefer lower-cost models for short contexts (≤4 messages) and return to primary models for long contexts
- `quality_first`: prefer higher-priority primary models
- `latency_first`: prefer entries with the best recent fast-success signal, falling back to priority selection

Main LLM calls also support connection-phase fallback. If the preferred provider fails before streaming starts, BlockCell can try the next available provider. Once streaming has started, it will not mix partial output from one model with another model's response.

### 🪝 Hook Lifecycle Events

BlockCell can run local commands from `~/.blockcell/hooks.yaml` at key lifecycle events:

- `pre_tool_use` / `post_tool_use` around tool execution, with tool-name glob matching
- `session_start` / `session_end` / `user_prompt` / `agent_stop` / `compaction` for session, message, and compaction lifecycle events
- Template variables such as `{tool_name}`, `{session_id}`, `{cwd}`, `{command}`, `{file_path}`, and `{result}`
- Hook failures and timeouts do not block the main agent flow, making hooks useful for auditing, formatting, notifications, and external logging

### 🧩 Runtime Kernel: Observable, Interceptable, Replayable

v0.1.8 splits the agent runtime into a composable kernel, so a run is no longer just a final answer:

- **Unified event bus**: lifecycle events such as `turn/start`, `step/end`, `tool/pre-execute`, `model/response`, `memory/write`, and `subagent/completed` are dispatched by priority and scope, with listener timeouts and failure isolation
- **Session event log**: an append-only JSONL sidecar (`schema_version` + monotonic `seq`) that evolves alongside the transcript and supports retention cleanup
- **Replay and evaluation**: `blockcell session list / replay / verify` replays turns, steps, model requests, and tool calls, and reports token, duration, and retry metrics; `verify` checks that the event-log projection matches the transcript
- **Guarded Tool Pipeline**: tool toggles, policy, dangerous operations, path access, and shell escalation are centralized in one pipeline, so individual tools no longer handle permissions or auditing themselves
- **Failure semantics**: distinguishes failed / rejected / cancelled / timed-out / not-started / unknown; unknown outcomes are reconciled before deciding whether to retry
- **Capability seams**: sandbox, shell, filesystem, browser, and subagent are exposed as replaceable interfaces, with native implementations behind them

### 🛡️ Sandbox and Execution Environment

- Sandbox backends: macOS Seatbelt and Linux Landlock + seccomp, falling back to explicit approval when unavailable
- **Execution environment is decoupled from "needs confirmation"**: the `exec` family honestly declares FullAccess, only `shell` with `sandbox_policy=full-access` asks per call, and dangerous commands are still gated on content
- Persistent shell sessions are isolated per session and no longer reused across channels
- Nested tool calls inside skill scripts are subject to the same policy, dangerous-operation checks, and auditing

### 🔐 Controlled Execution and Audit

v0.1.8 converges security boundaries onto shared primitives:

- Tool Policy supports tool-name globs, channel conditions, path conditions, `allow` / `ask` / `deny` decisions, and inherited rule groups, with **Deny > Ask > Allow** precedence
- Global token and cost budgets are tracked per session, and subagent spend is folded into the originating session so background agents cannot bypass it
- Audit logs support SHA-256 hash-chain verification (cross-process exclusive) and record sessions, provider calls, budget events, plus each tool call's execution environment and outcome
- Path expansion, filename sanitization, Python string escaping, SSRF validation, and config secret masking all go through shared primitives instead of per-module copies
- Gateway API authentication is stricter: regular APIs no longer accept URL `?token=`, so use `Authorization: Bearer <token>`

### 🤖 Multi-Agent and Custom Agents

- Supports typed agents, forked subagents, checkpoints, and chained cancellation
- Loads user-level and project-level Markdown agent definitions
- Each agent type can define tool scope, model, skills, MCP servers, one-shot behavior, and permission mode
- CLI, Gateway, and WebUI can receive background task progress events

### 🌐 Multi-Channel Support

Run BlockCell as a daemon and connect it to:

- **Telegram** (long polling)
- **WhatsApp** (bridge WebSocket)
- **Feishu** (long-connection WebSocket)
- **Lark** (webhook)
- **Slack** (Socket Mode, with polling fallback when `appToken` is absent)
- **Discord** (Gateway WebSocket)
- **DingTalk** (Stream SDK)
- **WeCom** (polling / webhook)
- **QQ** (official bot Gateway / webhook)
- **NapCatQQ** (OneBot 11 WebSocket / HTTP)
- **Weixin** (iLink Bot API with QR login and long polling)

#### 📖 Channel Integration Guides

Each channel has detailed configuration documentation (bilingual):

**Chinese Docs** | **English Docs**
--- | ---
[Telegram 配置](docs/channels/zh/01_telegram.md) | [Telegram Setup](docs/channels/en/01_telegram.md)
[Discord 配置](docs/channels/zh/02_discord.md) | [Discord Setup](docs/channels/en/02_discord.md)
[Slack 配置](docs/channels/zh/03_slack.md) | [Slack Setup](docs/channels/en/03_slack.md)
[飞书配置](docs/channels/zh/04_feishu.md) | [Feishu Setup](docs/channels/en/04_feishu.md)
[钉钉配置](docs/channels/zh/05_dingtalk.md) | [DingTalk Setup](docs/channels/en/05_dingtalk.md)
[企业微信配置](docs/channels/zh/06_wecom.md) | [WeCom Setup](docs/channels/en/06_wecom.md)
[WhatsApp 配置](docs/channels/zh/07_whatsapp.md) | [WhatsApp Setup](docs/channels/en/07_whatsapp.md)
[Lark 配置](docs/channels/zh/08_lark.md) | [Lark Setup](docs/channels/en/08_lark.md)
[QQ 配置](docs/channels/zh/09_qq.md) | [QQ Setup](docs/channels/en/09_qq.md)
[NapCatQQ 配置](docs/channels/zh/10_napcatqq.md) | [NapCatQQ Setup](docs/channels/en/10_napcatqq.md)
[微信配置](docs/channels/zh/11_weixin.md) | [Weixin Setup](docs/channels/en/11_weixin.md)

Each guide includes:
- 📝 Application creation steps
- 🔑 Permission configuration
- ⚙️ Blockcell configuration examples
- 💬 Interaction methods
- ⚠️ Troubleshooting common issues

### 🏗️ Rust Host + Three Skill Forms

```
┌─────────────────────────────────────────────┐
│         Rust Host (Trusted Core)            │
│  Message bus | Tool registry | Scheduler    │
│  Storage | Auditing | Security              │
└─────────────────────────────────────────────┘
                     ↕
┌─────────────────────────────────────────────┐
│         Skills Layer (Mutable Layer)        │
│  Pure Markdown | Markdown + Rhai            │
│  Markdown + Python                          │
└─────────────────────────────────────────────┘
```

- **Rust host**: Immutable, secure, high-performance foundation
- **Pure Markdown skills**: define behavior with `SKILL.md` only, ideal for knowledge and workflow-oriented skills
- **Markdown + Rhai skills**: combine `SKILL.md` with `SKILL.rhai` for structured orchestration and tool calling
- **Markdown + Python skills**: combine `SKILL.md` with Python scripts for heavier data processing, integrations, and execution logic
- **Community skill compatibility**: recursive skill pack/category scanning, with OpenClaw and gbrain skill support

---


## ✨ 核心特性

### 🛠️ 内置 60+ 工具

- **文件与系统**：读写文件、执行命令、处理 Excel/Word/PDF
- **网页与浏览器**：网页抓取、无头 Chrome 自动化（CDP）、HTTP 请求
- **金融数据**：实时股票行情（A股/港股/美股）、加密货币价格、DeFi 数据
- **通讯**：邮件（SMTP/IMAP）、Telegram、Slack、Discord、飞书
- **媒体**：截图、语音转文字（Whisper）、图表生成、Office 文件创建
- **AI 增强**：图像理解、文字转语音、OCR

### 🧬 自我进化系统

当 AI 在执行任务时反复失败，BlockCell 可以：

1. 检测错误模式
2. 使用 LLM 生成改进的代码
3. 自动审计、编译和测试
4. 通过金丝雀部署（10% → 50% → 100%）
5. 如果性能下降则自动回滚

```
检测到错误 → LLM 生成修复 → 审计 → 测试 → 金丝雀部署 → 全量发布
                                        ↓ 失败时
                                      自动回滚
```

### 🧠 Ghost Native 自学习

BlockCell 可以在真实使用中沉淀长期有效的经验：

- 将稳定用户偏好写入 `USER.md`
- 将项目事实、环境约定和踩坑记录写入 `MEMORY.md`
- Ghost 后台 Review 只沉淀声明性记忆；可复用流程由主 Agent 或独立 Skill Learning 通道整理成 workspace skills
- 后台 review 失败不影响当前对话，所有自动写入都有审计、快照和撤销入口

### 🧭 ModelRouter 智能路由与连接阶段降级

当你配置多个模型时，BlockCell 可以按策略选择合适的 Provider：

- `manual`：沿用 Provider Pool 的优先级 + 权重选择
- `cost_optimized`：短上下文（≤4 条消息）优先使用低成本模型，长上下文回到主力模型
- `quality_first`：优先使用高优先级主力模型
- `latency_first`：优先选择近期快速成功的条目，无历史数据时回退到优先级选择

主对话 LLM 调用还支持**连接阶段自动降级**：如果首选 Provider 在建立流之前出现连接、超时、DNS 等错误，会立即尝试下一个可用 Provider；一旦流式输出已经开始，中途错误不会切换模型，避免把部分输出和另一个模型的结果混在一起。

### 🪝 Hook 生命周期事件

BlockCell 可以通过 `~/.blockcell/hooks.yaml` 在关键生命周期事件上执行本机命令：

- `pre_tool_use` / `post_tool_use`：工具实际执行前后触发，支持按工具名 glob 匹配
- `session_start` / `session_end` / `user_prompt` / `agent_stop` / `compaction`：会话、消息与压缩级事件
- 支持 `{tool_name}`、`{session_id}`、`{cwd}`、`{command}`、`{file_path}`、`{result}` 等模板变量
- Hook 失败或超时不会阻断 agent 主流程，适合审计、格式化、通知和外部日志接入

### 🧩 Runtime Kernel：可观察、可拦截、可回放

v0.1.8 把 Agent 运行时拆成可组合的内核，执行过程不再只剩一个最终回答：

- **统一事件总线**：`turn/start`、`step/end`、`tool/pre-execute`、`model/response`、`memory/write`、`subagent/completed` 等生命周期事件按优先级与作用域派发，listener 带超时与故障隔离
- **会话事件日志**：追加式 JSONL 侧车（`schema_version` + 单调 `seq`），与 transcript 并行演进，支持保留期清理
- **回放与评估**：`blockcell session list / replay / verify` 可重放回合、步骤、模型请求与工具调用，输出 token、时长、重试等指标；`verify` 校验事件日志投影与 transcript 是否一致
- **Guarded Tool Pipeline**：工具开关、Policy、危险操作、路径访问、Shell 提权集中到一条管线，工具本身不再各自处理权限与审计
- **失败语义**：区分失败 / 被拒绝 / 取消 / 超时 / 未开始 / 结果未知；结果未知时先对账再决定是否重试
- **能力 seam**：沙箱、Shell、文件系统、浏览器、子 Agent 均以可替换接口暴露，本机实现可被替换

### 🛡️ 沙箱与执行环境

- 沙箱后端：macOS Seatbelt、Linux Landlock + seccomp；不可用时要求显式审批
- **执行环境与“是否需要确认”解耦**：`exec` 家族如实声明 FullAccess，只有 `sandbox_policy=full-access` 的 `shell` 逐次确认，危险命令仍按内容把关
- 持久 shell 会话按会话隔离，不再跨通道复用
- 技能脚本内的嵌套工具调用同样受 Policy、危险操作判定与审计约束

### 🔐 可控执行与审计

v0.1.8 把安全边界收口到统一原语：

- Tool Policy 支持工具名 glob、渠道条件、路径条件、`allow` / `ask` / `deny` 决策和规则组继承，优先级为 **Deny > Ask > Allow**
- 全局 Token / 成本预算按会话跟踪，子 Agent 的消耗归并到发起会话，后台代理无法绕过
- 审计日志支持 SHA-256 hash chain 校验（跨进程互斥），记录会话、Provider 调用、预算事件，以及每次工具调用的执行环境与结论
- 路径展开、文件名净化、Python 字符串转义、SSRF 校验、配置密钥掩码统一走共享原语，不再各模块各写一份
- Gateway API 默认更严格，普通 API 不再接受 URL `?token=`，应使用 `Authorization: Bearer <token>`

### 🤖 多智能体与自定义 Agent

- 支持 typed agent、forked subagent、checkpoint 和链式取消
- 支持用户级与项目级 Markdown agent 定义
- 每类 agent 可单独配置工具范围、模型、技能、MCP、one-shot 和权限模式
- CLI、Gateway 和 WebUI 可接收后台任务进度事件

### 🌐 多渠道支持

将 BlockCell 作为守护进程运行，连接到：

- **Telegram**（长轮询）
- **WhatsApp**（桥接 WebSocket）
- **飞书**（长连接 WebSocket）
- **Lark**（Webhook）
- **Slack**（Socket Mode，缺少 `appToken` 时可轮询回退）
- **Discord**（Gateway WebSocket）
- **钉钉**（Stream SDK）
- **企业微信**（WeCom，轮询/Webhook）
- **QQ**（官方机器人 Gateway/Webhook）
- **NapCatQQ**（OneBot 11 WebSocket/HTTP）
- **微信**（Weixin iLink Bot API，扫码登录/长轮询）

#### 📖 渠道接入指南

每个渠道都有详细的配置文档（中英双语）：

**中文文档** | **English Docs**
--- | ---
[Telegram 配置](docs/channels/zh/01_telegram.md) | [Telegram Setup](docs/channels/en/01_telegram.md)
[Discord 配置](docs/channels/zh/02_discord.md) | [Discord Setup](docs/channels/en/02_discord.md)
[Slack 配置](docs/channels/zh/03_slack.md) | [Slack Setup](docs/channels/en/03_slack.md)
[飞书配置](docs/channels/zh/04_feishu.md) | [Feishu Setup](docs/channels/en/04_feishu.md)
[钉钉配置](docs/channels/zh/05_dingtalk.md) | [DingTalk Setup](docs/channels/en/05_dingtalk.md)
[企业微信配置](docs/channels/zh/06_wecom.md) | [WeCom Setup](docs/channels/en/06_wecom.md)
[WhatsApp 配置](docs/channels/zh/07_whatsapp.md) | [WhatsApp Setup](docs/channels/en/07_whatsapp.md)
[Lark 配置](docs/channels/zh/08_lark.md) | [Lark Setup](docs/channels/en/08_lark.md)
[QQ 配置](docs/channels/zh/09_qq.md) | [QQ Setup](docs/channels/en/09_qq.md)
[NapCatQQ 配置](docs/channels/zh/10_napcatqq.md) | [NapCatQQ Setup](docs/channels/en/10_napcatqq.md)
[微信配置](docs/channels/zh/11_weixin.md) | [Weixin Setup](docs/channels/en/11_weixin.md)

每份指南包含：
- 📝 应用创建步骤
- 🔑 权限配置说明
- ⚙️ Blockcell 配置示例
- 💬 交互方式说明
- ⚠️ 常见问题排查

### 🏗️ Rust 宿主 + 三种技能形态

```
┌─────────────────────────────────────────────┐
│         Rust 宿主（可信核心）                │
│  消息总线 | 工具注册表 | 调度器              │
│  存储 | 审计 | 安全                          │
└─────────────────────────────────────────────┘
                     ↕
┌─────────────────────────────────────────────┐
│         技能层（可变层）                     │
│  纯 Markdown | Markdown + Rhai              │
│  Markdown + Python                          │
└─────────────────────────────────────────────┘
```

- **Rust 宿主**：不可变、安全、高性能的基础
- **纯 Markdown 技能**：只用 `SKILL.md` 定义行为说明，适合知识型与流程型技能
- **Markdown + Rhai 技能**：使用 `SKILL.md` + `SKILL.rhai` 实现结构化编排与工具调用
- **Markdown + Python 技能**：使用 `SKILL.md` + Python 脚本承载更复杂的数据处理、集成与执行逻辑
- **社区技能兼容**：支持 skill pack/category 递归扫描，兼容 OpenClaw 与 gbrain skill 格式

---

## 🚀 Quick Start


### Installation (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/blockcell-labs/blockcell/main/install.sh | sh
```

This installs `blockcell` to `~/.local/bin`. To customize the location:

```bash
curl -fsSL https://raw.githubusercontent.com/blockcell-labs/blockcell/main/install.sh | BLOCKCELL_INSTALL_DIR="$HOME/bin" sh
```

### Build from Source

**Prerequisites**: Rust 1.96+

```bash
git clone https://github.com/blockcell-labs/blockcell.git
cd blockcell
cargo build --release
```

### First Run

```bash
# Recommended: interactive setup wizard
blockcell setup

# Start interactive mode
blockcell agent
```

`setup` creates `~/.blockcell/`, saves provider settings, and auto-binds newly enabled external channels to the `default` agent when no owner is set yet.

### Daemon Mode (with WebUI)

```bash
blockcell gateway
```

- **API Server**: `http://localhost:18790`
- **WebUI**: `http://localhost:18791`
- **Default routing**: CLI / WebUI / WebSocket go to the `default` agent; external channels first check `channelAccountOwners.<channel>.<accountId>` and fall back to `channelOwners.<channel>`

### Common Commands

| Command | Description |
| --- | --- |
| `blockcell setup` | Interactive wizard: initialize config, pick a provider, optionally set up channels |
| `blockcell agent` | Interactive chat; `-m <msg>` for a one-shot run |
| `blockcell gateway` | Daemon mode: API + WebUI + external channels |
| `blockcell status` / `doctor` | Show config status / run environment diagnostics |
| `blockcell config` | Inspect, edit, or open `config.json5` |
| `blockcell tools` / `mcp` | Manage built-in tool toggles and MCP servers |
| `blockcell session list/replay/verify` | List, replay, and verify session event logs |
| `blockcell memory` / `skills` / `cron` | Manage memory, skills, and scheduled tasks |
| `blockcell upgrade` | Upgrade to the latest release |

See the [CLI reference](docs/en/17_cli_reference.md) for the full command list.

---


## 🚀 快速开始

### 安装（推荐）

```bash
curl -fsSL https://raw.githubusercontent.com/blockcell-labs/blockcell/main/install.sh | sh
```

这会将 `blockcell` 安装到 `~/.local/bin`。自定义安装位置：

```bash
curl -fsSL https://raw.githubusercontent.com/blockcell-labs/blockcell/main/install.sh | BLOCKCELL_INSTALL_DIR="$HOME/bin" sh
```

### 从源码构建

**前置要求**：Rust 1.96+

```bash
git clone https://github.com/blockcell-labs/blockcell.git
cd blockcell
cargo build --release
```

### 首次运行

```bash
# 推荐：交互式向导
blockcell setup

# 启动交互模式
blockcell agent
```

`setup` 会创建 `~/.blockcell/` 目录、写入 provider 配置，并在你启用外部渠道时自动补默认 owner 绑定。

### 守护进程模式（带 WebUI）

```bash
blockcell gateway
```

- **API 服务器**：`http://localhost:18790`
- **WebUI**：`http://localhost:18791`
- **默认路由**：CLI / WebUI / WebSocket 进入 `default` agent；外部渠道优先按 `channelAccountOwners.<channel>.<accountId>` 路由，未命中时回退到 `channelOwners.<channel>`

### 常用命令

| 命令 | 说明 |
| --- | --- |
| `blockcell setup` | 交互式向导：初始化配置、选择 provider、可选配置渠道 |
| `blockcell agent` | 交互式对话；`-m <msg>` 单次执行 |
| `blockcell gateway` | 守护进程：API + WebUI + 外部渠道接入 |
| `blockcell status` / `doctor` | 查看配置状态 / 环境诊断 |
| `blockcell config` | 查看、修改或打开 `config.json5` |
| `blockcell tools` / `mcp` | 管理内置工具开关与 MCP 服务器 |
| `blockcell session list/replay/verify` | 列出、回放、校验会话事件日志 |
| `blockcell memory` / `skills` / `cron` | 记忆、技能与定时任务管理 |
| `blockcell upgrade` | 升级到最新 Release |

完整命令参考见 [CLI 参考](docs/17_cli_reference.md)。

---

## 📸 Screenshots


<div align="center">

### Gateway Mode
![Start Gateway](screenshot/start-gateway.png)

### WebUI Interface
![WebUI Chat](screenshot/webui-chat.png)

![WebUI Login](screenshot/webui-login.png)

</div>

---


## 📸 项目截图

<div align="center">

### 守护进程模式
![启动 Gateway](screenshot/start-gateway.png)

### WebUI 界面
![WebUI 对话](screenshot/webui-chat.png)

![WebUI 登录](screenshot/webui-login.png)

</div>

---

## ⚙️ Configuration


Minimal configuration example (`~/.blockcell/config.json5`):

```json
{
  "providers": {
    "deepseek": {
      "apiKey": "YOUR_API_KEY",
      "apiBase": "https://api.deepseek.com"
    }
  },
  "agents": {
    "defaults": {
      "model": "deepseek-flash",
      "provider": "deepseek",
      "maxContextTokens": 1048576,
      "reasoningEffort": "high"
    }
  }
}
```

To enable multi-agent routing plus external channels, extend it using the structure currently supported by the codebase:

<details>
<summary>Expand the example: a complete "2 agents + 2 Telegram accounts" config</summary>

```json
{
  "agents": {
    "defaults": {
      "model": "deepseek-flash",
      "provider": "deepseek",
      "maxContextTokens": 1048576,
      "reasoningEffort": "high"
    },
    "list": [
      {
        "id": "default",
        "enabled": true,
        "name": "General Assistant",
        "intentProfile": "default"
      },
      {
        "id": "ops",
        "enabled": true,
        "name": "Operations Assistant",
        "intentProfile": "ops",
        "maxToolIterations": 12
      }
    ]
  },
  "intentRouter": {
    "enabled": true,
    "defaultProfile": "default",
    "agentProfiles": {
      "default": "default",
      "ops": "ops"
    },
    "profiles": {
      "default": {
        "coreTools": ["read_file", "write_file", "list_dir", "web_fetch", "message"],
        "intentTools": {
          "Chat": { "inheritBase": false, "tools": [] },
          "FileOps": ["read_file", "write_file", "list_dir"],
          "WebSearch": ["web_search", "web_fetch"]
        }
      },
      "ops": {
        "coreTools": ["http_request", "message", "notification", "alert_rule", "list_tasks"],
        "intentTools": {
          "DevOps": ["http_request", "notification", "alert_rule", "list_tasks"],
          "Communication": ["message", "notification"]
        },
        "denyTools": ["write_file", "exec"]
      }
    }
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "accounts": {
        "main_bot": {
          "enabled": true,
          "token": "123456:MAIN_BOT_TOKEN",
          "allowFrom": ["alice"]
        },
        "ops_bot": {
          "enabled": true,
          "token": "123456:OPS_BOT_TOKEN",
          "allowFrom": ["oncall_group"]
        }
      },
      "defaultAccountId": "main_bot"
    }
  },
  "channelOwners": {
    "telegram": "default"
  },
  "channelAccountOwners": {
    "telegram": {
      "main_bot": "default",
      "ops_bot": "ops"
    }
  },
  "gateway": {
    "apiToken": "YOUR_STABLE_API_TOKEN",
    "webuiPass": "YOUR_WEBUI_PASSWORD"
  }
}
```

</details>

Notes:

- `agents.list` should use fields actually supported by the code, such as `id`, `enabled`, `name`, `intentProfile`, and `maxToolIterations`
- `intentRouter` currently supports `enabled`, `defaultProfile`, `agentProfiles`, and `profiles`
- Each `profiles.<name>` entry can define `coreTools`, `intentTools`, and `denyTools`
- Telegram multi-account config belongs under `channels.telegram.accounts`, and each account uses `enabled`, `token`, and `allowFrom`
- Channel-level routing uses `channelOwners`
- Account-level overrides use `channelAccountOwners`
- If you only need a single agent, use the minimal config above or read `QUICKSTART.md`
- If you want the full multi-agent walkthrough, read `QUICKSTART.multi-agent.md`

### Supported LLM Providers

- **OpenAI** (GPT-5.5, GPT-5.4-mini, o1, o3)
- **Anthropic** (Claude Opus 4.8, Claude Sonnet 4.6)
- **Google Gemini** (Gemini 3.1 Pro, Gemini 3.5 Flash)
- **DeepSeek** (DeepSeek V4 Pro, V3, R1)
- **Kimi/Moonshot**
- **MiniMax** ([MiniMax M3.1 Flash](https://www.minimaxi.com/))
- **Zhipu AI** ([GLM-5](https://bigmodel.cn/))
- **Qwen**
- **xAI** (Grok)
- **Mistral**
- **Groq**
- **SiliconFlow** ([SiliconFlow](https://siliconflow.cn/))
- **Ollama** (local models, fully offline)
- **OpenRouter** (unified access to 200+ models)

---


## ⚙️ 配置说明

最小配置示例（`~/.blockcell/config.json5`）：

```json
{
  "providers": {
    "deepseek": {
      "apiKey": "YOUR_API_KEY",
      "apiBase": "https://api.deepseek.com"
    }
  },
  "agents": {
    "defaults": {
      "model": "deepseek-flash",
      "provider": "deepseek",
      "maxContextTokens": 1048576,
      "reasoningEffort": "high"
    }
  }
}
```

多模型路由示例：

```json
{
  "agents": {
    "defaults": {
      "routingStrategy": "cost_optimized",
      "modelPool": [
        { "model": "deepseek-flash", "provider": "deepseek", "priority": 1, "weight": 3 },
        { "model": "gpt-6-luna", "provider": "openai", "priority": 5, "weight": 1 }
      ]
    }
  },
  "providers": {
    "deepseek": { "apiKey": "YOUR_DEEPSEEK_KEY" },
    "openai": { "apiKey": "YOUR_OPENAI_KEY" }
  }
}
```

`cost_optimized` 会让短对话优先使用低优先级的便宜条目，复杂长上下文仍优先使用主力模型。更多策略与降级细节见 [ModelRouter 智能路由与自动降级](docs/28_model_router.md)。

如果要启用多 agent 与外部渠道，建议按代码当前支持的结构补充：

<details>
<summary>展开示例：“2 个 agent + 2 个 Telegram 账号”的完整配置</summary>

```json
{
  "agents": {
    "defaults": {
      "model": "deepseek-flash",
      "provider": "deepseek",
      "maxContextTokens": 1048576,
      "reasoningEffort": "high"
    },
    "list": [
      {
        "id": "default",
        "enabled": true,
        "name": "General Assistant",
        "intentProfile": "default"
      },
      {
        "id": "ops",
        "enabled": true,
        "name": "Operations Assistant",
        "intentProfile": "ops",
        "maxToolIterations": 12
      }
    ]
  },
  "intentRouter": {
    "enabled": true,
    "defaultProfile": "default",
    "agentProfiles": {
      "default": "default",
      "ops": "ops"
    },
    "profiles": {
      "default": {
        "coreTools": ["read_file", "write_file", "list_dir", "web_fetch", "message"],
        "intentTools": {
          "Chat": { "inheritBase": false, "tools": [] },
          "FileOps": ["read_file", "write_file", "list_dir"],
          "WebSearch": ["web_search", "web_fetch"]
        }
      },
      "ops": {
        "coreTools": ["http_request", "message", "notification", "alert_rule", "list_tasks"],
        "intentTools": {
          "DevOps": ["http_request", "notification", "alert_rule", "list_tasks"],
          "Communication": ["message", "notification"]
        },
        "denyTools": ["write_file", "exec"]
      }
    }
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "accounts": {
        "main_bot": {
          "enabled": true,
          "token": "123456:MAIN_BOT_TOKEN",
          "allowFrom": ["alice"]
        },
        "ops_bot": {
          "enabled": true,
          "token": "123456:OPS_BOT_TOKEN",
          "allowFrom": ["oncall_group"]
        }
      },
      "defaultAccountId": "main_bot"
    }
  },
  "channelOwners": {
    "telegram": "default"
  },
  "channelAccountOwners": {
    "telegram": {
      "main_bot": "default",
      "ops_bot": "ops"
    }
  },
  "gateway": {
    "apiToken": "YOUR_STABLE_API_TOKEN",
    "webuiPass": "YOUR_WEBUI_PASSWORD"
  }
}
```

</details>

这里要注意：

- `agents.list` 里的字段要使用代码实际支持的字段，例如 `id`、`enabled`、`name`、`intentProfile`、`maxToolIterations`
- `intentRouter` 当前支持 `enabled`、`defaultProfile`、`agentProfiles`、`profiles`
- `profiles.<name>` 里可以配置 `coreTools`、`intentTools`、`denyTools`
- Telegram 多账号要写在 `channels.telegram.accounts` 下，每个账号使用 `enabled`、`token`、`allowFrom`
- 渠道路由使用 `channelOwners`
- 账号级覆盖路由使用 `channelAccountOwners`
- 如果你只需要单 agent，请直接看上面的最小配置，或者阅读 `QUICKSTART.zh-CN.md`
- 如果你需要完整多 agent 部署说明，请阅读 `QUICKSTART.multi-agent.zh-CN.md`

### 支持的 LLM 提供商

- **OpenAI**（GPT-5.5、GPT-5.4-mini、o1、o3）
- **Anthropic**（Claude Opus 4.8、Claude Sonnet 4.6）
- **Google Gemini**（Gemini 3.1 Pro、Gemini 3.5 Flash）
- **DeepSeek**（DeepSeek V4 Pro、V3、R1）
- **Kimi/Moonshot**（月之暗面）
- **MiniMax**（[MiniMax M3.1 Flash](https://www.minimaxi.com/)）
- **智谱 AI**（[GLM-5](https://bigmodel.cn/)）
- **通义千问**（Qwen）
- **xAI**（Grok）
- **Mistral**
- **Groq**
- **硅基流动**（[SiliconFlow](https://siliconflow.cn/)）
- **Ollama**（本地模型，完全离线）
- **OpenRouter**（统一访问 200+ 模型）

---

## 🔧 Optional Dependencies


For full functionality, install these tools:

- **Charts**: Python 3 + `matplotlib` / `plotly`
- **Office**: Python 3 + `python-pptx` / `openpyxl` (DOCX is generated natively, no Python needed)
- **Audio**: `ffmpeg` + `whisper` (or use API backend)
- **Browser**: Chrome/Chromium (for CDP automation)
- **macOS only**: `chrome_control`, `app_control`

---


## 🔧 可选依赖

要使用完整功能，请安装这些工具：

- **图表**：Python 3 + `matplotlib` / `plotly`
- **Office**：Python 3 + `python-pptx` / `openpyxl`（DOCX 为原生生成，无需 Python）
- **音频**：`ffmpeg` + `whisper`（或使用 API 后端）
- **浏览器**：Chrome/Chromium（用于 CDP 自动化）
- **仅 macOS**：`chrome_control`、`app_control`

---

## 📚 Documentation


- [Full series index](docs/en/00_index.md)
- [What is blockcell?](docs/en/01_what_is_blockcell.md)
- [Quick Start Guide (Single Agent)](QUICKSTART.md)
- [Quick Start Guide (Multi-Agent)](QUICKSTART.multi-agent.md)
- [Architecture Deep Dive](docs/en/12_architecture.md)
- [Tool System](docs/en/03_tools_system.md)
- [Skill System](docs/en/04_skill_system.md)
- [Memory System](docs/en/05_memory_system.md)
- [Ghost Native Learning Design (Chinese)](docs/27_ghost_learning_design.md)
- [ModelRouter and Fallback (Chinese)](docs/28_model_router.md)
- [Provider Pool](docs/en/20_provider_pool.md)
- [Intent Router and Profiles](docs/en/21_intent_router_profiles.md)
- [Hook Lifecycle Events (Chinese)](docs/29_hook_system.md)
- [MCP Servers](docs/en/19_mcp_servers.md)
- [Channel Configuration](docs/en/06_channels.md)
- [Browser Automation](docs/en/07_browser_automation.md)
- [Gateway Mode](docs/en/08_gateway_mode.md)
- [Self-Evolution](docs/en/09_self_evolution.md)
- [Path Access Policy](docs/en/22_path_access_policy.md)
- [CLI Reference](docs/en/17_cli_reference.md)

---


## 📚 文档

- [完整系列索引](docs/00_index.md)
- [什么是 blockcell？](docs/01_what_is_blockcell.md)
- [快速开始指南（单 agent）](QUICKSTART.zh-CN.md)
- [多 agent 快速开始](QUICKSTART.multi-agent.zh-CN.md)
- [架构深度解析](docs/12_architecture.md)
- [工具系统](docs/03_tools_system.md)
- [技能系统](docs/04_skill_system.md)
- [记忆系统](docs/05_memory_system.md)
- [Ghost Native 学习闭环](docs/27_ghost_learning_design.md)
- [ModelRouter 智能路由与自动降级](docs/28_model_router.md)
- [Provider Pool](docs/20_provider_pool.md)
- [Intent Router 与 Profile](docs/21_intent_router_profiles.md)
- [Hook 生命周期事件系统](docs/29_hook_system.md)
- [MCP 服务器](docs/19_mcp_servers.md)
- [渠道配置](docs/06_channels.md)
- [浏览器自动化](docs/07_browser_automation.md)
- [Gateway 模式](docs/08_gateway_mode.md)
- [自我进化](docs/09_self_evolution.md)
- [路径访问策略](docs/22_path_access_policy.md)
- [CLI 参考](docs/17_cli_reference.md)

---

## 🏗️ Project Structure


```
blockcell/
├── bin/blockcell/          # CLI entry point
├── crates/
│   ├── core/               # Config, paths, shared types
│   ├── agent/              # Agent runtime and safety
│   ├── tools/              # 60+ built-in tools
│   ├── skills/             # Skill loading, OpenClaw compatibility & evolution
│   ├── storage/            # SQLite memory, sessions & RabitQ vector index
│   ├── channels/           # Messaging channel adapters
│   ├── providers/          # LLM provider clients and routing
│   ├── scheduler/          # Cron & heartbeat
│   └── updater/            # Self-upgrade system
├── skills/                 # Built-in skills (Markdown / Rhai / Python)
├── webui/                  # Web console (Vite + React)
├── docs/                   # Chinese & English docs
└── tests/                  # E2E and coding-ability evals
```

---


## 🏗️ 项目结构

```
blockcell/
├── bin/blockcell/          # CLI 入口
├── crates/
│   ├── core/               # 配置、路径、共享类型
│   ├── agent/              # Agent 运行时和安全
│   ├── tools/              # 60+ 内置工具
│   ├── skills/             # 技能加载、OpenClaw 兼容与进化
│   ├── storage/            # SQLite 记忆、会话、RabitQ 向量索引
│   ├── channels/           # 消息渠道适配器
│   ├── providers/          # LLM 提供商客户端与路由
│   ├── scheduler/          # Cron 与心跳
│   └── updater/            # 自升级系统
├── skills/                 # 内置技能（Markdown / Rhai / Python）
├── webui/                  # Web 控制台（Vite + React）
├── docs/                   # 中英文档
└── tests/                  # 端到端与编码能力评测
```

---

## 🤝 Contributing


We welcome contributions! Here's how to get started:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

---


## 🤝 贡献

我们欢迎贡献！以下是开始的方法：

1. Fork 本仓库
2. 创建特性分支（`git checkout -b feature/amazing-feature`）
3. 提交你的更改（`git commit -m 'Add amazing feature'`）
4. 推送到分支（`git push origin feature/amazing-feature`）
5. 打开 Pull Request

详细指南请参阅 [CONTRIBUTING.md](CONTRIBUTING.md)。

---

## 🔒 Security


- **Path safety**: Automatic validation of file system access
- **Sandboxed execution**: Rhai scripts run in isolated environment, and shells can run behind the Seatbelt / Landlock backends
- **Audit logging**: All tool executions are logged, including execution environment and outcome
- **Session replay**: `blockcell session replay` replays the full chain of any run for after-the-fact auditing
- **Gateway authentication**: Bearer token support for API access

In interactive mode, operations outside `~/.blockcell/workspace` require explicit confirmation.

---


## 🔒 安全性

- **路径安全**：自动验证文件系统访问
- **沙箱执行**：Rhai 脚本在隔离环境中运行，Shell 可运行在 Seatbelt / Landlock 后端
- **审计日志**：所有工具执行都被记录，含执行环境与结论
- **会话回放**：`blockcell session replay` 可回放每次执行的完整链路，便于事后审计
- **网关认证**：API 访问支持 Bearer token

在交互模式下，`~/.blockcell/workspace` 外的操作需要明确确认。

---

## 📊 Use Cases


### Finance Automation
```
"Monitor AAPL stock and alert me if it drops 5%"
"Analyze my portfolio.xlsx and suggest rebalancing"
```

### Data Processing
```
"Read all PDFs in ~/Documents and create a summary spreadsheet"
"Generate a sales report with charts from data.csv"
```

### Web Automation
```
"Check my company's website every hour and alert if it's down"
"Fill out this form on example.com with data from sheet.xlsx"
```

### Communication
```
"Send daily standup summary to #team-updates on Slack"
"Forward urgent emails to my Telegram"
```

---


## 📊 使用场景

### 金融自动化
```
"监控茅台股价，如果跌破 1500 就提醒我"
"分析我的 portfolio.xlsx 并建议再平衡"
```

### 数据处理
```
"读取 ~/Documents 中的所有 PDF 并创建摘要表格"
"从 data.csv 生成带图表的销售报告"
```

### 网页自动化
```
"每小时检查公司网站，如果宕机就提醒"
"用 sheet.xlsx 中的数据填写 example.com 上的表单"
```

### 通讯
```
"每天发送站会总结到 Slack #team-updates"
"将紧急邮件转发到我的 Telegram"
```

---

## 🌍 Community


- **GitHub**: [blockcell-labs/blockcell](https://github.com/blockcell-labs/blockcell)
- **Website**: [blockcell.dev](https://blockcell.dev)
- **Discord**: [Join our community](https://discord.gg/E8TXuHk9QZ)
- **Twitter/X**: [@blockcell_ai](https://x.com/blockcell_ai)

---


## 🌍 社区

- **GitHub**：[blockcell-labs/blockcell](https://github.com/blockcell-labs/blockcell)
- **官网**：[blockcell.dev](https://blockcell.dev)
- **Discord**：[加入我们的社区](https://discord.gg/E8TXuHk9QZ)
- **Twitter/X**：[@blockcell_ai](https://x.com/blockcell_ai)

---

## 📝 License


This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---


## 📝 许可证

本项目采用 MIT 许可证 - 详见 [LICENSE](LICENSE) 文件。

---

## 🙏 Acknowledgments


BlockCell stands on the shoulders of giants:

- [Rust](https://www.rust-lang.org/) - Systems programming language
- [Rhai](https://rhai.rs/) - Embedded scripting engine
- [Tokio](https://tokio.rs/) - Async runtime
- [SQLite](https://www.sqlite.org/) - Embedded database
- [OpenClaw](https://github.com/openclaw/openclaw) - Source of the compatible skill format
- [NanoClaw](https://github.com/nanocoai/nanoclaw) - A lightweight OpenClaw alternative

---

<div align="center">

**If you find BlockCell useful, please consider giving it a ⭐️ on GitHub!**

[⭐ Star on GitHub](https://github.com/blockcell-labs/blockcell) • [📖 Read the Docs](https://blockcell.dev/docs) • [💬 Join Discord](https://discord.gg/E8TXuHk9QZ)

</div>


## 🙏 致谢

BlockCell 站在巨人的肩膀上：

- [Rust](https://www.rust-lang.org/) - 系统编程语言
- [Rhai](https://rhai.rs/) - 嵌入式脚本引擎
- [Tokio](https://tokio.rs/) - 异步运行时
- [SQLite](https://www.sqlite.org/) - 嵌入式数据库
- [OpenClaw](https://github.com/openclaw/openclaw) - 技能格式兼容来源
- [NanoClaw](https://github.com/nanocoai/nanoclaw) - 轻量化的 OpenClaw 替代实现

---

<div align="center">

**如果你觉得 BlockCell 有用，请在 GitHub 上给我们一个 ⭐️！**

[⭐ 在 GitHub 上 Star](https://github.com/blockcell-labs/blockcell) • [📖 阅读文档](https://blockcell.dev/docs) • [💬 加入 Discord](https://discord.gg/E8TXuHk9QZ)

</div>

---

## Technical Appendix / 技术附录

This appendix covers the underlying BlockCell mechanisms for readers who need implementation detail. Casual users can skip this.

本附录面向需要实现细节的读者，说明底层 BlockCell 机制。普通用户可以跳过。

### Runtime Kernel / 运行时内核

The v0.1.8 runtime kernel introduces an event bus (`EventBus`), a serial interception chain (`Waterfall`), and unified scope primitives (`ScopedRegistry` / `ScopeGuard`). Events are dispatched by priority and scope filtering, and listeners support timeout and failure isolation (fail-open / fail-closed).

v0.1.8 运行时内核引入了事件总线（`EventBus`）、串行拦截链（`Waterfall`）和统一作用域原语（`ScopedRegistry` / `ScopeGuard`）。事件按优先级和作用域过滤分发，监听器支持超时和故障隔离（fail-open / fail-closed）。

### Agent Lifecycle Events / Agent 生命周期事件

The system emits `agent/pre-step`, `agent/request`, `agent/request-error`, `turn/start`, `turn/end`, `step/start`, `step/end`, `tool/pre-execute`, `tool/post-execute`, `tool/result`, `model/response`, `assistant/attempt`, `memory/write`, `subagent/spawn`, `subagent/completed`, `evolution/deployed`, `compaction/start`, and `compaction/end` events, classified by Durable / Live / Capability consistency semantics.

系统发出 `agent/pre-step`、`agent/request`、`agent/request-error`、`turn/start`、`turn/end`、`step/start`、`step/end`、`tool/pre-execute`、`tool/post-execute`、`tool/result`、`model/response`、`assistant/attempt`、`memory/write`、`subagent/spawn`、`subagent/completed`、`evolution/deployed`、`compaction/start`、`compaction/end` 事件，按 Durable / Live / Capability consistency 语义分类。

### Guarded Tool Pipeline /  guarded tool pipeline

Tool toggles, policy, dangerous-operation, path-access, and shell-escalation checks are centralized into one pipeline; individual tools no longer handle permissions, approvals, or auditing themselves.

工具开关、策略、危险操作、路径访问和 shell 升级检查集中到一个 pipeline；各个工具不再自行处理权限、审批或审计。

### Failure Semantics / 失败语义

`FailureKind`, `Idempotency`, and `ToolOutcome` distinguish failed / rejected / cancelled / timed-out / not-started / unknown. Unknown outcomes are reconciled before retry. Model request errors pick a recovery strategy by type: compact-then-retry on context overflow, backoff on rate limit, provider switch on auth failure.

`FailureKind`、`Idempotency` 和 `ToolOutcome` 区分 failed / rejected / cancelled / timed-out / not-started / unknown。未知结果在重试前进行核对。模型请求错误按类型选择恢复策略：上下文溢出时 compact-then-retry，速率限制时 backoff，认证失败时切换 provider。

### Session Event Log and Replay / 会话事件日志与回放

An append-only JSONL sidecar (`schema_version`, monotonic `seq`, retention cleanup) runs alongside the transcript. `blockcell session list / replay / verify` replays turns, steps, model requests, and tool calls. `SessionMetrics` reports turn / step / tool-outcome / retry / token / duration metrics.

一个仅追加的 JSONL 伴生文件（`schema_version`、单调递增 `seq`、保留清理）与 transcript 并行运行。`blockcell session list / replay / verify` 可回放 turns、steps、模型请求和工具调用。`SessionMetrics` 报告 turn / step / tool-outcome / retry / token / duration 指标。

### Compaction and Context Injection / 压缩与上下文注入

Compaction is abstracted into a `ContextCompactor` strategy (`ModelSummaryCompactor` / `ExtractiveCompactor`). The `agent/pre-step` decision gate consolidates budget, compaction, and context injection, and supports a registrable policy chain that can rewrite or stop the step.

压缩被抽象为 `ContextCompactor` 策略（`ModelSummaryCompactor` / `ExtractiveCompactor`）。`agent/pre-step` 决策门合并预算、压缩和上下文注入，支持可注册策略链来重写或停止 step。

### Evolution Isolation / 进化隔离

The Evolution Candidate Scope and evolution trust boundary keep candidate skills / prompts / policies visible only inside a view that explicitly enters candidate evaluation.

Evolution Candidate Scope 和 evolution trust boundary 确保候选 skill / prompt / policy 只在显式进入候选评估的视图中可见。
