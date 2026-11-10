# TASK — TurboHermes Phase 1: fork identity, then enhance by COPYING existing code

You are working in `/home/hermes/projects/turbohermes-fork`, a clone of the fork
`m3m007/turbohermes` (parent `blockcell-labs/blockcell`, pinned commit
5ca38be861b4a3e8f7a1e8d332e242b42ed25710). This is Phase 1 of the owner's architecture
document, quoted exactly at the bottom.

## THE ONE RULE THAT OVERRIDES EVERYTHING ELSE

Owner ruling, verbatim: "we enhance them with code WHICH MOSTLY ALREADY EXISTS in other
projects and needs to be inspected and COPIED not developed from scratch."

So: **write no new algorithm, no new module, and no new abstraction that an existing
project already implements.** Before you write any non-trivial code, run the
existing-tool check: search the three upstreams for the same capability, read the file
that implements it, and copy or adapt it with the source path recorded. If you cannot
find a primitive, say so in your report with the search space you checked, and only then
propose the smallest possible new code.

The three upstreams, all on this host:
- `/home/hermes/projects/upstream/blockcell` (pinned, 2,654 tests passing) — the runtime
  you are enhancing. Its policy surface is `crates/core/src/{tool_policy,path_policy,
  capability,evolution_boundary,agent_identity,budget,scope,task_contract}.rs`.
- `/home/hermes/projects/upstream/jcode` (pinned, 453 tests passing) — the coding harness.
- `/home/hermes/projects/upstream/graphicle` (pinned) — code intelligence, consumed as a
  release binary, NOT built from source.

There is also `/home/hermes/projects/turbohermes`, a greenfield workspace with four crates
(turbohermes-core, turbohermes-memory, turbohermes-decisions, turbohermes-hygiene).
**Treat those four crates as a source of ideas to copy, not as code to carry over.** The
owner has ruled that greenfield code where a proven tested primitive exists is a defect.
Where one of those four crates duplicates something in the upstreams, the upstream version
wins and the crate is dropped.

## PHASE 1 SCOPE, EXACTLY

The owner's document, Phase 1:

    1. Fork blockcell-labs/blockcell.                      [DONE — this clone]
    2. Rename package, binary, config paths and user-facing
       branding systematically.
    3. Keep upstream history and define a regular upstream
       merge/rebase process.
    4. Preserve existing BlockCell functionality initially.
    5. Add TurboHermes-specific integration crates only where
       needed.
    6. Add CI for formatting, clippy, unit tests, integration
       tests and licence checks.

    Do not combine this phase with a wholesale architecture rewrite.

Items 2 through 6 are yours. Item 4 is a constraint on 2, not a separate task: nothing may
break.

## WHAT TO DO, IN ORDER

**Step A — baseline first.** Run `cargo build` and `cargo test` in the fork BEFORE changing
anything, and record the exact test count. That number is the pass mark for everything
after. If the build is still running when you start, wait for it; do not edit under it.

**Step B — the systematic rename.** The rename targets, measured just now:
- 11 `name = "blockcell*"` entries: `blockcell`, `blockcell-agent`, `blockcell-channels`,
  `blockcell-core`, `blockcell-providers`, `blockcell-scheduler`, `blockcell-skills`,
  `blockcell-storage`, `blockcell-tools`, `blockcell-updater`, plus `bin/blockcell`.
- 5,049 case-insensitive mentions of "blockcell" across tracked files, including config
  paths and user-facing branding.

Rename to `turbohermes` / `turbohermes-*`. Be exhaustive but mechanical: no behaviour
changes ride along with the rename. Keep upstream history intact, which means no rebase and
no squash: commit the rename as a single reviewable change on top of the fork point.
Watch for: string literals that are not identifiers (config file names, CLI help text,
error messages, docs), file and directory names, and anything a user would type. A rename
that leaves `blockcell` in a config path is not finished.

**Step C — upstream merge process.** Define it in the repo, not in chat: a short
`docs/architecture/UPSTREAM-SYNC.md` with the exact commands to fetch
`blockcell-labs/blockcell`, merge or rebase, and what to do when the rename conflicts.
The owner's document asks for a regular process, so name the cadence and the owner of each
step.

**Step D — integration crates, only where needed.** Phase 1 item 5 says "only where
needed", which means the default answer is none. Do not add crates to fill out a plan. If
you add one, its brief must name the upstream file it copies from, and it must not
duplicate anything blockcell, jcode or graphicle already does.

**Step E — CI.** `.github/workflows` covering: `cargo fmt --check`, `cargo clippy
--all-targets -- -D warnings`, `cargo test`, and a licence check. For the licence check,
use what already exists on this host: `cargo-deny` is installed and configured at
`/home/hermes/projects/turbohermes/deny.toml`. Copy that config rather than writing a new
one. CI must run on the fork, not locally.

## DONE-CHECK

All of these, in the fork, with a clean working tree:

1. `cargo build` succeeds.
2. `cargo test` passes with a test count >= the Step A baseline, and you state both
   numbers. A drop is a failure, not a rounding note.
3. `grep -ri blockcell --include="*.rs" --include="*.toml" --include="*.md"` returns
   nothing except the deliberate upstream-sync documentation and any third-party notice
   that must be preserved.
4. `cargo fmt --check` is clean.
5. `cargo clippy --all-targets` has no NEW warnings versus the baseline. Record the
   baseline count and the final count; do not silence warnings to make this pass.
6. CI is pushed and green, or you report exactly which job fails and why.
7. Everything is committed and pushed to `origin/main` of `m3m007/turbohermes`.

## REPORTING RULES

- Report only what you observed: the command and its output. Never a plausible number.
- Every deviation between this contract and what you did is one line, before the work.
- If you write any code that is not copied or adapted from an existing project, name the
  file, say what you searched, and say why no primitive existed. That is the single most
  important part of your report.
- Stop after three failed attempts on the same error; record what was tried and report.
