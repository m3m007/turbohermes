# Phase 3 report — jcode integration

Authority: work order v7 section 9, "Phase 3 jcode integration."

## What was built

`turbohermes-jcode`: a narrow typed client over jcode's harness API bridge, a
controlled worktree scope, lifecycle capture into `turbohermes-core`'s event
store, typed artifacts, and SQLite claim reconciliation after timeout, crash and
restart. Branch `phase-3-jcode-adapter`.

## Measured line counts (`wc -l src/*.rs tests/*.rs`, 2026-11-10)

Ported from a pinned upstream (9 `port`, 4 `port-adapt` rows in REUSE_MAP.md):

```
   439 src/scope.rs       (jcode-sdk worktrees.rs)
   628 src/spec.rs        (BlockCell task_contract.rs + budget.rs)
   671 src/transport.rs   (jcode-harness-api client.rs + sockets.rs)
   691 src/reconcile.rs   (beads claim.go + lease.go)
   727 src/frames.rs      (jcode-harness-api requests.rs + events.rs)
```

Glue, with no upstream counterpart (5 `glue` rows):

```
   455 src/artifacts.rs
   608 src/events.rs
   698 src/client.rs
```

Plus the CLI and its tests: `482 src/main.rs`, `71 src/lib.rs`,
`711 tests/integration.rs`.

Glue is 1,761 lines against 3,156 ported: the glue is what connects the two
worlds, and it is the part with no upstream to lean on.

## Commands run

```
$ cargo check --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.45s

$ cargo test
test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

72 unit tests and 7 integration tests: cancellation, restart, duplicate
dispatch, worktree isolation, event store ordering, and three CLI round-trips
(`dispatch`, `reconcile`, `inspect`).

## What was learned the hard way

`send_message` never has a reply frame. The reply is the event stream, so
`send` returns immediately and the turn is read through `collect_turn`. A stub
that answered the send with a reply deadlocked every test that drove it.

`token_usage` is the only authoritative cost source upstream gives, and it
carries token counts and no price. The cost artifact therefore reports zero and
says so, rather than a number this crate invented.

`reconcile` is read-only. A supervisor runs it against a ledger a live dispatch
is still writing to without disturbing anything; recording the verdict is a
separate, explicit step. The CLI does both, and skips `StillRunning` so a live
claim is not closed by a read.
