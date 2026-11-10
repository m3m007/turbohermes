# Preferred tools — TurboHermes fork workspace

Loaded into the static, cacheable part of the system prompt. This file is the
tool-selection doctrine. It advises; the mechanical enforcement lives in the
pre_tool guard, which REWRITES rather than blocks. Keep it short: it is paid for
on every request.

## The core set

**rtk** for output, **agentgrep** for location, **graphicle** for dependency
questions, **fmt + clippy** for quality, the **three scanners** for secrets,
**cargo deny** as the gate. Seven tools, four of them mechanical. Everything else
is situational: reach for it on evidence, not on principle.

## Reading code: never read a big file whole

Measured on this host against an 84,739-byte file:

| What | Bytes | Cut |
|---|---|---|
| `Read store.rs` whole | 84,739 | — |
| `rtk read store.rs -l aggressive` | 13,531 | 84% |
| `rtk read store.rs --max-lines 120` | 3,996 | 95% |
| `rtk smart store.rs` | ~200 | 99.9% |

- First contact with an unknown file: `rtk smart <file>`. Two lines, no model call.
- Structure without bodies: `rtk read <file> -l aggressive` (signatures only).
- `--max-lines N` keeps imports and signatures; it is NOT the first N lines, that
  is `--head-lines`.
- If a filter elided something, `rtk recall <hash>` returns the cut part.
- The pre_tool guard blocks a whole-file read over 400 lines with no prior
  agentgrep this session. Run agentgrep first, then Read with offset and limit.

## Locating code

- `agentgrep <pattern>` is AST-aware and native; it has no MCP tax. Prefer it over
  grep for "where is this defined / called".
- `ast-grep -p '<pattern>'` for structural patterns across a tree.
- `rg` for plain text when the pattern is genuinely textual.

## Dependency and impact questions

`graphicle` answers "what depends on this, what breaks". Use its MCP tools
(`query_graph`, `get_node`, `get_neighbors`, `get_community`, `god_nodes`,
`graph_stats`, `shortest_path`) rather than reading files to answer a dependency
question. Token budget 2000.

## Quality gates, run them yourself

- `cargo fmt --check` before you say a change is done.
- `cargo clippy --all-targets` and report the warning count. Do not silence
  warnings to make a gate pass.
- Secret scan with `gitleaks`, `ripsecrets` or `kingfisher` before any commit.
- `cargo deny check` for licences and advisories.

## Tool documentation

Full syntax and measured behaviour for every tool on this host lives in
`/home/hermes/projects/docs-store/<tool>/dossier.md`. Read the dossier before
using a tool whose syntax you are unsure of. Reading the dossier is cheaper than
a failed command, and a failed command is cheaper than a wrong assumption.

## Do not

- Do not run the full upstream test suites of BlockCell, jcode or Graphicle. Run
  only tests for code you changed plus the new integration tests.
- Do not read a large file whole to answer a question a tool answers directly.
- Do not install anything not already on this host.
