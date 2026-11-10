# Tool policy — the key set

**Status: policy, adopted 2026-11-10. Review for efficiency later, as the owner
directed. This document supersedes the tool doctrine in TOOL-RANKING.md, which
measured a much larger inventory than we actually use.**

## The problem this policy solves

Measured across 1,538 jcode build commands and 5,038 native tool calls: jcode uses
its own built-in tools freely (bash 5,038, edit 501, read 491, agentgrep 93) but
external CLI tools only run when something forces them. rtk reached 53 bash
invocations the day its rewrite was wired correctly, and zero the day before.
gitleaks, cargo-deny, ast-grep, tokei, fd and bat have never been invoked once.

So tool use is not a knowledge problem, it is a mechanism problem. A tool named in
a prompt file is advisory and gets ignored. A tool wired into the pre_tool
transform gets used. This policy therefore defines a small key set and makes each
member mechanical.

## The key set — six tools

| Tool | Job | Why it is in the set | Mechanism |
|---|---|---|---|
| **rtk** | Filter command output | The only proven token lever: 474.7K saved, 50.2%, across ~100 wrapped subcommands | guard rewrites every command through it |
| **rg** | Search that respects .gitignore | Replaces grep, which the builds ran 91 times | guard rewrites grep to `rtk rg` |
| **gitleaks** | Secret detection on commit | Prevents a credential reaching a commit. Ran zero times before the guard | guard appends to `git commit` |
| **cargo-deny** | CVE and licence gate | Found RUSTSEC-2026-0459 (CVE-2026-119375) when it ran. Ran zero times in builds | guard appends to `cargo test` |
| **cargo fmt + clippy** | Formatting and lint | Already in the toolchain, zero install cost, cheapest gates in Rust | guard appends `cargo fmt --check` |
| **graphicle** | Dependency and impact questions | Answers "what breaks" without reading files; 10 MCP tools | prompt layer, until a mechanical trigger exists |

`agentgrep` is a built-in jcode tool, not a CLI binary, with 93 real calls. It
stays in the set as the code-location primitive and needs no wiring.

## What is demoted to situational

Installed and working, but not part of the doctrine. Reach for these on evidence,
not on principle. Do not build rules or prompts around them.

| Tool | Why demoted |
|---|---|
| ripsecrets, kingfisher | Overlap gitleaks. Keep gitleaks primary; use a second scanner only when a commit is security-relevant |
| cargo-public-api | Overlaps cargo-modules. Produced 147KB on the decisions crate, too big to inject |
| cargo-machete | grep-class, imprecise by design. Run at build end, not per edit |
| tokei | Overlaps cargo-modules; low value |
| difftastic, git-delta, `rtk diff` | Three ways to diff. rtk's wrap is enough |
| bat, fd | Duplicate `rtk read` and `rtk find` |
| rust-analyzer | Heaviest option, entirely unmeasured |
| sqlite3 | Ad-hoc queries only; not a build tool |
| jq | Infrastructure used by the guard itself, not a build tool |
| gh | Used by the operator, not by builds |

## Rules

1. **A tool joins the key set only with a mechanical trigger.** Naming it in a
   prompt file does not count; that is the failure this policy exists to fix.
2. **A tool leaves the key set when its trigger stops firing or its measurement
   goes flat.** Check the rtk ledger and the build logs before believing either.
3. **Never instruct a tool that is not installed or not available in the lane.**
   The guard's rule 1 told the model to run agentgrep as a CLI binary; agentgrep
   is a jcode built-in. Check `command -v` before writing an instruction.
4. **Redundancy is a cost, not safety.** Three secret scanners is not three times
   the protection, it is three times the runtime with one result.
5. **Measure before claiming a saving.** `rtk rg` measured 4% smaller than grep on
   a small query, not the large win assumed. The win is real because it is
   strictly better, but it is not large.

## Enforcement

`/home/hermes/.hermes/scripts/jcode-tool-guard.sh`, wired as BOTH `pre_tool` (the
gate: exit 0 allows, exit 2 blocks) and `pre_tool_transform` (the input replacer:
exit 0 required, stdout must be the complete replacement input object). The script
branches on `JCODE_HOOK_EVENT` because the two hooks have different contracts.

Current rewrites, all verified by a live ledger move:

- every command through `rtk`
- `grep` to `rtk rg`
- `cargo test|build|check` gains `cargo fmt --check` and `cargo deny check`
- `git commit` gains `gitleaks protect --staged --redact`

Escape hatch: `JCODE_GUARD_OFF=1` in the command passes it through untouched.

## Review trigger

Re-read this policy when any of these is true: the rtk ledger stops growing, a key
tool's invocation count stays at zero across a full build, a new tool is proposed,
or the owner asks. The review question is always the same: does this tool have a
mechanical trigger, and what did it measure?
