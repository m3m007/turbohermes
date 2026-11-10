//! Acceptance test for the mutation-safety ticket: kill the agent after a
//! mutation and restore the previous state from the durable checkpoint.
//!
//! The parent test re-execs this test binary as a child. The child creates a
//! checkpoint, mutates the target, then calls `std::process::abort()` so no
//! in-process state survives. The parent then opens a fresh `CheckpointManager`
//! over the same directory and restores the checkpoint using only what is on
//! disk.

use std::path::PathBuf;
use std::process::Command;

use blockcell_tools::checkpoints::CheckpointManager;

const CHILD_ENV: &str = "BLOCKCELL_CHECKPOINT_CRASH_CHILD";
const BASE_ENV: &str = "BLOCKCELL_CHECKPOINT_CRASH_BASE";
const TARGET_ENV: &str = "BLOCKCELL_CHECKPOINT_CRASH_TARGET";

#[test]
fn crash_child_aborts_after_the_mutation() {
    if std::env::var(CHILD_ENV).is_err() {
        return;
    }
    let base = PathBuf::from(std::env::var(BASE_ENV).expect("base env"));
    let target = PathBuf::from(std::env::var(TARGET_ENV).expect("target env"));
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let manager = CheckpointManager::new(base);

    runtime.block_on(async {
        let record = manager
            .checkpoint_path(&target, "write_file")
            .await
            .expect("checkpoint before mutation");
        std::fs::write(&target, "after\n").expect("mutate after checkpoint");
        println!("CHECKPOINT_ID={}", record.id);
        // Hard kill: no destructor, no flush, no graceful shutdown.
        std::process::abort();
    });
}

#[test]
fn a_killed_agent_leaves_a_restorable_checkpoint() {
    if std::env::var(CHILD_ENV).is_ok() {
        return; // this process is the child
    }

    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("data");
    std::fs::create_dir_all(&base).expect("base dir");
    let target = temp.path().join("target.txt");
    std::fs::write(&target, "before\n").expect("seed target");

    let status = Command::new(std::env::current_exe().expect("current exe"))
        .args([
            "--exact",
            "crash_child_aborts_after_the_mutation",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .env(BASE_ENV, &base)
        .env(TARGET_ENV, &target)
        .status()
        .expect("spawn child");
    assert!(
        !status.success(),
        "the child must die without a graceful shutdown"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("target after crash"),
        "after\n",
        "the mutation happened before the kill"
    );

    // Recovery reads only the on-disk manifests.
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let manager = CheckpointManager::new(base);
    let checkpoints = runtime
        .block_on(manager.list(10, None))
        .expect("list durable checkpoints");
    assert_eq!(checkpoints.len(), 1, "one durable checkpoint survived");
    assert_eq!(checkpoints[0].reason, "write_file");

    runtime
        .block_on(manager.restore(&checkpoints[0].id))
        .expect("restore the checkpoint after the crash");
    assert_eq!(
        std::fs::read_to_string(&target).expect("target after restore"),
        "before\n",
        "recovery restores the previous state"
    );
}
