//! Pre-mutation checkpoints for file-mutating tools.
//!
//! Ported from Moltis `crates/tools/src/auto_checkpoint.rs` at commit
//! `8a12400dc10551dfb9a6b568414b91a343ef2d5d` (MIT). Moltis hooks
//! `BeforeToolCall` for the tools `Write`, `Edit` and `MultiEdit` and groups the
//! resulting checkpoints by agent run. This module keeps that shape and maps it
//! onto BlockCell's tool names and admission pipeline:
//!
//! | Moltis | BlockCell |
//! |---|---|
//! | `Write`, `Edit`, `MultiEdit` | `write_file`, `edit_file`, `file_ops`, `office_write` |
//! | `BeforeToolCall` hook | call from `run_tool_pipeline` after admission, before execution |
//! | `run_id` | `AgentRuntime::current_turn_id` |
//! | no blocking on snapshot failure | blocked: the mutation does not run without a snapshot |
//!
//! The blocking divergence is deliberate. Work order v7 section 4.6 and the
//! architecture's transaction protocol (checkpoint before execute, restore or
//! commit after verification) require the snapshot to exist before the write.
//! Moltis logs a warning and proceeds (`auto_checkpoint.rs:131-141`); that
//! policy is not ported.

use std::collections::HashMap;
use std::path::PathBuf;

use blockcell_core::Result;
use blockcell_tools::checkpoints::{CheckpointManager, CheckpointRecord, TurnRecord};
use serde_json::Value;

/// File-mutating tools whose target can be addressed before execution.
///
/// `exec` and `shell` are absent on purpose: their effects are opaque, and
/// Moltis does not snapshot them either. They remain a recorded gap.
const CHECKPOINT_TOOLS: &[&str] = &["write_file", "edit_file", "file_ops", "office_write"];

/// In-progress turn state for a single session, as in Moltis's `ActiveTurn`.
struct ActiveTurn {
    run_id: String,
    session_key: String,
    checkpoint_ids: Vec<String>,
    source_paths: Vec<String>,
    created_at: i64,
}

/// Runtime-owned checkpointer. One instance per `AgentRuntime`.
pub(crate) struct MutationCheckpointer {
    manager: CheckpointManager,
    active: HashMap<String, ActiveTurn>,
}

impl MutationCheckpointer {
    pub(crate) fn new(base: PathBuf) -> Self {
        Self {
            manager: CheckpointManager::new(base),
            active: HashMap::new(),
        }
    }

    /// Target paths a tool call is about to mutate.
    ///
    /// These are the BlockCell equivalents of Moltis's `extract_file_path`.
    /// `file_ops` carries several actions, so the path set depends on `action`.
    pub(crate) fn targets_for(tool_name: &str, args: &Value) -> Vec<String> {
        if !CHECKPOINT_TOOLS.contains(&tool_name) {
            return Vec::new();
        }
        let mut paths: Vec<String> = Vec::new();
        let mut push = |value: Option<&str>| {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                if !paths.iter().any(|existing| existing == value) {
                    paths.push(value.to_string());
                }
            }
        };

        match tool_name {
            "write_file" => {
                push(args.get("path").and_then(Value::as_str));
                if let Some(files) = args.get("files").and_then(Value::as_array) {
                    for file in files {
                        push(file.get("path").and_then(Value::as_str));
                    }
                }
            }
            "edit_file" => push(args.get("path").and_then(Value::as_str)),
            "office_write" => {
                if args.get("action").and_then(Value::as_str) != Some("info") {
                    push(args.get("output_path").and_then(Value::as_str));
                }
            }
            "file_ops" => match args.get("action").and_then(Value::as_str).unwrap_or("") {
                // The source disappears, so it must be restorable.
                "delete" | "rename" | "move" => {
                    push(args.get("path").and_then(Value::as_str));
                    push(args.get("destination").and_then(Value::as_str));
                }
                // The destination is created; a checkpoint with existed=false
                // lets restore remove it.
                "copy" | "compress" => {
                    push(args.get("destination").and_then(Value::as_str));
                }
                // Without an explicit destination the extracted path is derived
                // from the archive name; that derivation is not ported, so such
                // calls stay unsnapshotted and are recorded as a gap.
                "decompress" => push(args.get("destination").and_then(Value::as_str)),
                _ => {}
            },
            _ => {}
        }
        paths
    }

    /// Snapshot every target, then record the ids in the current turn.
    ///
    /// Returns the checkpoint ids. An error means at least one target could not
    /// be snapshotted and the caller must not run the mutation.
    pub(crate) async fn checkpoint_before(
        &mut self,
        tool_name: &str,
        action: Option<&str>,
        targets: &[PathBuf],
        turn_id: &str,
        session_key: &str,
    ) -> Result<Vec<String>> {
        let reason = match action {
            Some(action) => format!("{tool_name}:{action}"),
            None => tool_name.to_string(),
        };

        let mut ids = Vec::with_capacity(targets.len());
        for target in targets {
            let record = self.manager.checkpoint_path(target, &reason).await?;
            self.record_checkpoint(turn_id, session_key, &record);
            ids.push(record.id);
        }
        Ok(ids)
    }

    fn record_checkpoint(&mut self, turn_id: &str, session_key: &str, record: &CheckpointRecord) {
        let same_run = self
            .active
            .get(session_key)
            .is_some_and(|turn| turn.run_id == turn_id);
        if !same_run {
            if let Some(previous) = self.active.remove(session_key) {
                self.flush(previous);
            }
        }

        let now = chrono::Utc::now().timestamp();
        let turn = self
            .active
            .entry(session_key.to_string())
            .or_insert_with(|| ActiveTurn {
                run_id: turn_id.to_string(),
                session_key: session_key.to_string(),
                checkpoint_ids: Vec::new(),
                source_paths: Vec::new(),
                created_at: now,
            });
        turn.checkpoint_ids.push(record.id.clone());
        turn.source_paths.push(record.source_path.clone());
    }

    /// Write a finished turn to the index. Moltis does this on `AgentEnd`; this
    /// port writes it when the turn id changes, so the last turn is flushed by
    /// the next mutation rather than by a turn-end hook. The checkpoint
    /// manifests themselves are durable on disk either way.
    fn flush(&self, turn: ActiveTurn) {
        if turn.checkpoint_ids.is_empty() {
            return;
        }
        let record = TurnRecord {
            run_id: turn.run_id,
            session_key: turn.session_key,
            created_at: turn.created_at,
            checkpoint_ids: turn.checkpoint_ids,
            source_paths: turn.source_paths,
        };
        if let Err(error) = self.manager.append_turn(&record) {
            tracing::warn!(error = %error, "failed to write checkpoint turn record");
        }
    }
}

impl Clone for MutationCheckpointer {
    fn clone(&self) -> Self {
        // Active turns are per-turn state; a cloned runtime starts a fresh index.
        Self {
            manager: self.manager.clone(),
            active: HashMap::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn write_file_targets_include_path_and_files() {
        assert_eq!(
            MutationCheckpointer::targets_for(
                "write_file",
                &json!({"path": "a.txt", "content": "x"})
            ),
            vec!["a.txt".to_string()]
        );
        assert_eq!(
            MutationCheckpointer::targets_for(
                "write_file",
                &json!({"files": [{"path": "a.txt", "content": "x"}, {"path": "b.txt", "content": "y"}]})
            ),
            vec!["a.txt".to_string(), "b.txt".to_string()]
        );
    }

    #[test]
    fn file_ops_targets_follow_the_action() {
        assert_eq!(
            MutationCheckpointer::targets_for(
                "file_ops",
                &json!({"action": "delete", "path": "gone.txt"})
            ),
            vec!["gone.txt".to_string()]
        );
        assert_eq!(
            MutationCheckpointer::targets_for(
                "file_ops",
                &json!({"action": "rename", "path": "old.txt", "destination": "new.txt"})
            ),
            vec!["old.txt".to_string(), "new.txt".to_string()]
        );
        assert_eq!(
            MutationCheckpointer::targets_for(
                "file_ops",
                &json!({"action": "compress", "destination": "bundle.zip", "path": "src"})
            ),
            vec!["bundle.zip".to_string()]
        );
        assert!(MutationCheckpointer::targets_for(
            "file_ops",
            &json!({"action": "read_pdf", "path": "doc.pdf"})
        )
        .is_empty());
    }

    #[test]
    fn opaque_and_read_only_tools_have_no_targets() {
        for tool in [
            "exec",
            "shell",
            "read_file",
            "list_dir",
            "checkpoint_restore",
        ] {
            assert!(
                MutationCheckpointer::targets_for(tool, &json!({"command": "rm -rf x"})).is_empty(),
                "{tool} must not be checkpointed"
            );
        }
    }

    #[test]
    fn office_write_info_is_not_a_mutation() {
        assert!(
            MutationCheckpointer::targets_for("office_write", &json!({"action": "info"}))
                .is_empty()
        );
        assert_eq!(
            MutationCheckpointer::targets_for(
                "office_write",
                &json!({"action": "create_docx", "output_path": "out.docx"})
            ),
            vec!["out.docx".to_string()]
        );
    }
}
