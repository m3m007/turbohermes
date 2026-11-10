# Preferred tools — the key set

Loaded into the static, cacheable part of the system prompt. This file is the
tool-selection doctrine, and it is short on purpose: it is paid for on every
request. The full policy, including what is demoted and why, is
`/home/hermes/projects/agent-daemon/TOOL-POLICY.md`.

**The mechanical enforcement is the guard, not this file.** Measured across
1,538 build commands: tools named in prose get ignored, tools wired into the
pre_tool transform get used. rtk went from zero invocations to 53 the day its
rewrite was wired correctly. So read this file as context, and expect the guard
to have already rewritten your command.

## The key set — six tools

| Tool | Use it for |
|---|---|
| **rtk** | Every command. It filters output; 474.7K tokens saved at 50.2% |
| **rg** | Search. Replaces grep, and respects .gitignore |
| **gitleaks** | Runs automatically on `git commit` |
| **cargo-deny** | Runs automatically on `cargo test` |
| **cargo fmt + clippy** | Formatting and lint. Free, already in the toolchain |
| **graphicle** | "What depends on this, what breaks." 10 MCP tools |

`agentgrep` is a built-in tool, not a CLI binary, and needs no wiring.

## What you do not need to think about

The guard already appends the gates. `cargo test` arrives as
`rtk cargo test && rtk cargo fmt --check && cargo deny check`. `git commit`
arrives with `gitleaks protect --staged`. Recursive `grep` arrives as `rtk rg`.
Do not add them yourself; a second copy is noise.

## Reading code

Measured on an 84,739-byte file:

| What | Cut |
|---|---|
| `Read` whole | — |
| `rtk read <file> -l aggressive` | 84% |
| `rtk read <file> --max-lines 120` | 95% |
| `rtk smart <file>` | 99.9%, two lines, no model call |

First contact with an unknown file: `rtk smart <file>`.

## Escape hatch

`JCODE_GUARD_OFF=1` in a command passes it through untouched. Use it when you
genuinely need unfiltered output, and say why in your report.

## Do not

- Do not run the full upstream test suites of BlockCell, jcode or Graphicle. Run
  only tests for code you changed plus the new integration tests.
- Do not reach for a demoted tool on principle. The demoted list and the reasons
  are in TOOL-POLICY.md; reach for one on evidence.
- Do not install anything not already on this host.
