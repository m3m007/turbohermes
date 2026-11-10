# Prompt overlay — TurboHermes implementation order

This is the owner's implementation order for this project. It is the authority.
It overrides default behaviour, including any general preference for building
things yourself.

## The one rule that governs everything

**Reuse before write.** Proven, tested code on GitHub is the foundation. The job
is to assemble, fork, copy and integrate it. The job is not to rewrite it.

For every piece of functionality, use this order and stop at the first that works:

1. **Use as is.** Run the upstream project unmodified and talk to it through its
   interface.
2. **Fork and modify.** Fork the repository and change only what is needed.
3. **Copy in.** Copy specific files or modules from an upstream project, keep the
   licence, and note the source path and commit.
4. **Write new code.** Only for the components listed below, and only when steps
   1 to 3 do not fit.

## Forbidden

- Writing a new crate, module or clone that reimplements BlockCell, jcode or
  Graphicle, or any part of them.
- Rewriting working upstream code in your own style.
- Running the full upstream test suites of BlockCell, jcode or Graphicle. Run
  only tests for code you changed and the integration tests for the new
  interfaces.
- Installing or downloading anything beyond the packages the task needs.
- Replacing anything with a scratch or greenfield version because it seems
  simpler, cleaner or faster.
- Changing the component split, storage choices, phase order or interfaces.

## What is new code

Only these, and each first checks for existing primitives (SQLite and FTS5,
rusqlite, tree-sitter, jcode's memory code, BlockCell's storage and policy code,
existing crates for BM25, embeddings and event logs):

- TurboHermes Memory service (records, scope, provenance, status lifecycle,
  context compiler)
- Telemetry: append-only event log, trace identifiers, outcome collector
- DecisionProvider interface with Jev, Clef, Clef-Flash and deterministic adapters
- TrajectoryCritic interface with the OpenHands Critic adapter
- Thin adapters between BlockCell, jcode and Graphicle (task and trace identifier
  propagation)
- Evaluation corpus tooling

Everything else comes from the three upstream projects.

## Upstream locations on this host

- `/home/hermes/projects/upstream/blockcell` — pinned `5ca38be`, MIT. FORKED.
  The working tree is this repository.
- `/home/hermes/projects/upstream/jcode` — pinned `47e426e5`, MIT. Coding
  subsystem, launched and supervised, never merged in.
- `/home/hermes/projects/upstream/graphicle` — pinned `ddf11b1`. Code
  intelligence, consumed as a release binary, never built from source.

Tool documentation with syntax and measured behaviour:
`/home/hermes/projects/docs-store/<tool>/dossier.md`.

## Working method

- Work on a branch. Never work on the main branch.
- Checkpoint commit after every step.
- Self-check at the end of each phase: compare lines of new code against lines
  copied or forked, and check that no new crate duplicates a function of
  BlockCell, jcode or Graphicle. Record the numbers in the phase report.
- Phase report at the end of each phase: what was reused, what was changed in
  upstream code, what is new, anything unsure, what the next phase needs.

## Stop and ask only when

- About to create a new crate or module that overlaps a listed project.
- The document and the real code conflict, and the design must change.
- Something needs installing that is not already in the environment.
- An action is destructive or cannot be undone.

For anything else, decide inside these rules and note it in the phase report.

## Current scope

Phases 1, 2 and 3 of the architecture document in one run. Stop after Phase 3
with the phase reports.
