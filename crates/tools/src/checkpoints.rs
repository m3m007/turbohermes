//! Filesystem checkpoints.
//!
//! Ported from Moltis `crates/tools/src/checkpoints.rs`
//! (commit 8a12400dc10551dfb9a6b568414b91a343ef2d5d, MIT).
//!
//! Before a built-in tool mutates a file or directory, a checkpoint of the
//! previous contents is copied into `<data_dir>/checkpoints/<id>/snapshot`.
//! A restore removes whatever is at the source path now and writes the snapshot
//! back, so paths created after the checkpoint disappear.
//!
//! Deviations from upstream are listed at the bottom of this file and in the
//! work order: the `moltis_agents::tool_registry::AgentTool` trait is
//! `blockcell_tools::Tool`, `Error::message` is `Error::Tool`, and restore goes
//! through a path restriction that upstream does not have.

use std::{
    fs,
    path::{Path, PathBuf},
};

use {
    async_trait::async_trait,
    serde::{Deserialize, Serialize},
    serde_json::{json, Value},
    uuid::Uuid,
};

use blockcell_core::{Error, Result};

use crate::{Tool, ToolContext, ToolSchema};

const DEFAULT_LIST_LIMIT: usize = 20;
const MAX_LIST_LIMIT: usize = 100;
/// Prune when checkpoint count exceeds this threshold.
const CLEANUP_CHECKPOINT_THRESHOLD: usize = 500;
/// Fraction of checkpoints to remove when pruning.
const CLEANUP_PRUNE_RATIO: f64 = 0.2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointSourceKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub id: String,
    pub created_at: i64,
    pub reason: String,
    pub source_path: String,
    pub source_kind: Option<CheckpointSourceKind>,
    pub existed: bool,
    pub backup_path: Option<String>,
}

impl CheckpointRecord {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "createdAt": self.created_at,
            "reason": self.reason,
            "sourcePath": self.source_path,
            "sourceKind": self.source_kind,
            "existed": self.existed,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointManager {
    base_dir: PathBuf,
}

impl CheckpointManager {
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            base_dir: data_dir.join("checkpoints"),
        }
    }

    pub async fn checkpoint_path(&self, source: &Path, reason: &str) -> Result<CheckpointRecord> {
        let base_dir = self.base_dir.clone();
        let source = source.to_path_buf();
        let reason = reason.to_string();

        tokio::task::spawn_blocking(move || checkpoint_path_blocking(&base_dir, &source, &reason))
            .await
            .map_err(|error| Error::Tool(format!("checkpoint task failed: {error}")))?
    }

    pub async fn list(
        &self,
        limit: usize,
        path_contains: Option<&str>,
    ) -> Result<Vec<CheckpointRecord>> {
        let base_dir = self.base_dir.clone();
        let path_contains = path_contains.map(|value| value.to_lowercase());
        let limit = limit.clamp(1, MAX_LIST_LIMIT);

        tokio::task::spawn_blocking(move || {
            let mut items = read_all_manifests(&base_dir)?;
            if let Some(filter) = path_contains {
                items.retain(|item| item.source_path.to_lowercase().contains(&filter));
            }
            items.sort_by(|lhs, rhs| {
                rhs.created_at
                    .cmp(&lhs.created_at)
                    .then_with(|| lhs.id.cmp(&rhs.id))
            });
            items.truncate(limit);
            Ok(items)
        })
        .await
        .map_err(|error| Error::Tool(format!("checkpoint list task failed: {error}")))?
    }

    pub async fn restore(&self, id: &str) -> Result<CheckpointRecord> {
        let base_dir = self.base_dir.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || restore_checkpoint_blocking(&base_dir, &id))
            .await
            .map_err(|error| Error::Tool(format!("checkpoint restore task failed: {error}")))?
    }

    /// Restore a checkpoint, but only when the checkpointed source path is
    /// inside one of `allowed_roots`.
    ///
    /// This is the path restriction required by work order v7 section 2.2; it
    /// does not exist upstream. Without it a checkpoint manifest could name any
    /// absolute path on the host and a restore would overwrite it.
    pub async fn restore_within(
        &self,
        id: &str,
        allowed_roots: &[PathBuf],
    ) -> Result<CheckpointRecord> {
        validate_checkpoint_id(id)?;
        let base_dir = self.base_dir.clone();
        let id_owned = id.to_string();
        let roots = allowed_roots.to_vec();

        tokio::task::spawn_blocking(move || {
            let checkpoint_dir = base_dir.join(&id_owned);
            let record = read_manifest(&checkpoint_dir)?;
            let source = PathBuf::from(&record.source_path);
            let canonical_source = source.canonicalize().unwrap_or(source.clone());

            let allowed = roots.iter().any(|root| {
                let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
                canonical_source == canonical_root || canonical_source.starts_with(&canonical_root)
            });
            if !allowed {
                return Err(Error::Tool(format!(
                    "refusing to restore checkpoint '{}': source path '{}' is outside the allowed roots",
                    id_owned,
                    record.source_path
                )));
            }

            restore_checkpoint_blocking(&base_dir, &id_owned)
        })
        .await
        .map_err(|error| Error::Tool(format!("checkpoint restore task failed: {error}")))?
    }
}

// ── Turn index ──────────────────────────────────────────────────────────────

/// A group of checkpoints created during a single agent turn (run).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub run_id: String,
    pub session_key: String,
    pub created_at: i64,
    pub checkpoint_ids: Vec<String>,
    /// Source paths for display (parallel to checkpoint_ids).
    pub source_paths: Vec<String>,
}

impl CheckpointManager {
    /// Path to the turns index file.
    fn turns_path(&self) -> PathBuf {
        self.base_dir.join("turns.jsonl")
    }

    /// Append a turn record to the index.
    pub fn append_turn(&self, record: &TurnRecord) -> Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        let mut line = serde_json::to_string(record)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.turns_path())?;
        std::io::Write::write_all(&mut file, line.as_bytes())?;
        Ok(())
    }

    /// Read turn records (newest first), optionally filtered by session key.
    pub fn read_turns(&self, limit: usize, session_key: Option<&str>) -> Result<Vec<TurnRecord>> {
        let path = self.turns_path();
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut turns: Vec<TurnRecord> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .filter(|turn: &TurnRecord| session_key.is_none_or(|sk| turn.session_key == sk))
            .collect();
        turns.sort_by_key(|turn| std::cmp::Reverse(turn.created_at));
        turns.truncate(limit);
        Ok(turns)
    }

    /// Remove old checkpoints when the total count exceeds the threshold.
    ///
    /// Deletes the oldest checkpoints (by `created_at`) until the count is
    /// under the threshold. Also removes corresponding turn records.
    pub async fn cleanup_if_needed(&self) -> Result<usize> {
        let base_dir = self.base_dir.clone();
        tokio::task::spawn_blocking(move || cleanup_blocking(&base_dir))
            .await
            .map_err(|e| Error::Tool(format!("checkpoint cleanup failed: {e}")))?
    }
}

fn cleanup_blocking(base_dir: &Path) -> Result<usize> {
    let mut all = read_all_manifests(base_dir)?;
    if all.len() <= CLEANUP_CHECKPOINT_THRESHOLD {
        return Ok(0);
    }

    // Sort oldest first, delete the oldest CLEANUP_PRUNE_RATIO fraction.
    all.sort_by_key(|r| r.created_at);
    let to_remove = (all.len() as f64 * CLEANUP_PRUNE_RATIO).ceil() as usize;
    let removed_ids: std::collections::HashSet<String> =
        all.iter().take(to_remove).map(|r| r.id.clone()).collect();

    let mut deleted = 0;
    for id in &removed_ids {
        let dir = base_dir.join(id);
        if dir.is_dir() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                tracing::warn!(checkpoint = %id, error = %e, "failed to remove old checkpoint");
            } else {
                deleted += 1;
            }
        }
    }

    // Rewrite turns.jsonl without references to deleted checkpoints.
    // Upstream uses a let-chain here; this crate is on edition 2021, so the
    // same condition is spelled out with nested ifs.
    let turns_path = base_dir.join("turns.jsonl");
    if turns_path.exists() {
        if let Ok(content) = fs::read_to_string(&turns_path) {
            let filtered: Vec<String> = content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| {
                    let mut turn: TurnRecord = serde_json::from_str(line).ok()?;
                    let orig_len = turn.checkpoint_ids.len();
                    let mut new_ids = Vec::new();
                    let mut new_paths = Vec::new();
                    for (id, path) in turn.checkpoint_ids.into_iter().zip(turn.source_paths) {
                        if !removed_ids.contains(&id) {
                            new_ids.push(id);
                            new_paths.push(path);
                        }
                    }
                    if new_ids.is_empty() && orig_len > 0 {
                        return None; // entire turn pruned
                    }
                    turn.checkpoint_ids = new_ids;
                    turn.source_paths = new_paths;
                    serde_json::to_string(&turn).ok()
                })
                .collect();
            let _ = fs::write(&turns_path, filtered.join("\n") + "\n");
        }
    }

    if deleted > 0 {
        tracing::info!(
            deleted,
            remaining = all.len() - deleted,
            "pruned old checkpoints"
        );
    }
    Ok(deleted)
}

/// `checkpoints_list` — list recent automatic checkpoints.
///
/// The checkpoint store lives at `<base>/checkpoints` (Moltis layout); the base
/// comes from the tool context so every caller shares one store.
pub struct CheckpointsListTool;

impl CheckpointsListTool {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Manager rooted at `<data_dir>/checkpoints`, exactly as upstream builds it.
    fn manager_for(ctx: &ToolContext) -> CheckpointManager {
        CheckpointManager::new(ctx.base.clone())
    }
}

impl Default for CheckpointsListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CheckpointsListTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "checkpoints_list".to_string(),
            description:
                "List recent automatic checkpoints created before built-in file mutations."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum checkpoints returned (default: 20, max: 100)."
                    },
                    "path_contains": {
                        "type": "string",
                        "description": "Optional case-insensitive substring filter on the source path."
                    }
                }
            }),
        }
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execution_environment(&self, _params: &Value) -> crate::ExecutionEnvironment {
        crate::ExecutionEnvironment::WorkspaceWrite
    }

    async fn execute(&self, ctx: ToolContext, params: Value) -> Result<Value> {
        let manager = Self::manager_for(&ctx);
        let limit = u64_param(&params, "limit", DEFAULT_LIST_LIMIT as u64) as usize;
        let path_contains = str_param(&params, "path_contains");
        let checkpoints = manager.list(limit, path_contains).await?;

        Ok(json!({
            "count": checkpoints.len(),
            "checkpoints": checkpoints.into_iter().map(|item| item.to_json()).collect::<Vec<_>>(),
        }))
    }
}

/// `checkpoint_restore` — restore a file mutation from a checkpoint.
///
/// Unlike upstream, the restore is restricted to the workspace plus any
/// sandbox writable roots approved for the call (work order v7 section 2.2).
pub struct CheckpointRestoreTool;

impl CheckpointRestoreTool {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    fn manager_for(ctx: &ToolContext) -> CheckpointManager {
        CheckpointManager::new(ctx.base.clone())
    }
}

impl Default for CheckpointRestoreTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CheckpointRestoreTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "checkpoint_restore".to_string(),
            description:
                "Restore a built-in file or directory mutation from an automatic checkpoint."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Checkpoint ID returned by checkpoints_list or a tool result."
                    }
                },
                "required": ["id"]
            }),
        }
    }

    fn validate(&self, params: &Value) -> Result<()> {
        require_str(params, "id")?;
        Ok(())
    }

    fn execution_environment(&self, _params: &Value) -> crate::ExecutionEnvironment {
        crate::ExecutionEnvironment::WorkspaceWrite
    }

    async fn execute(&self, ctx: ToolContext, params: Value) -> Result<Value> {
        let id = require_str(&params, "id")?;
        let manager = Self::manager_for(&ctx);
        // Work order v7 section 2.2: restore is restricted to the workspace and
        // any roots the runtime approved as writable for this call.
        let mut allowed_roots = vec![ctx.workspace.clone()];
        if let Some(roots) = ctx.sandbox_writable_roots.as_ref() {
            allowed_roots.extend(roots.iter().cloned());
        }
        let checkpoint = manager.restore_within(id, &allowed_roots).await?;

        Ok(json!({
            "restored": true,
            "checkpoint": checkpoint.to_json(),
        }))
    }
}

fn checkpoint_path_blocking(
    base_dir: &Path,
    source: &Path,
    reason: &str,
) -> Result<CheckpointRecord> {
    let id = Uuid::new_v4().simple().to_string();
    let checkpoint_dir = base_dir.join(&id);
    fs::create_dir_all(&checkpoint_dir)?;

    let metadata = match fs::symlink_metadata(source) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };

    let mut record = CheckpointRecord {
        id,
        created_at: chrono::Utc::now().timestamp(),
        reason: reason.to_string(),
        source_path: source.to_string_lossy().into_owned(),
        source_kind: None,
        existed: metadata.is_some(),
        backup_path: None,
    };

    if let Some(metadata) = metadata {
        if metadata.file_type().is_symlink() {
            return Err(Error::Tool(format!(
                "refusing to checkpoint symlink path '{}'",
                source.display()
            )));
        }

        let snapshot_root = checkpoint_dir.join("snapshot");
        match classify_source_kind(&metadata, source)? {
            CheckpointSourceKind::File => {
                let backup = snapshot_root.join("file");
                if let Some(parent) = backup.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(source, &backup)?;
                record.source_kind = Some(CheckpointSourceKind::File);
                record.backup_path = Some("snapshot/file".to_string());
            }
            CheckpointSourceKind::Directory => {
                let backup = snapshot_root.join("dir");
                copy_dir_recursive(source, &backup)?;
                record.source_kind = Some(CheckpointSourceKind::Directory);
                record.backup_path = Some("snapshot/dir".to_string());
            }
        }
    }

    write_manifest(&checkpoint_dir, &record)?;
    Ok(record)
}

fn restore_checkpoint_blocking(base_dir: &Path, id: &str) -> Result<CheckpointRecord> {
    validate_checkpoint_id(id)?;
    let checkpoint_dir = base_dir.join(id);
    let record = read_manifest(&checkpoint_dir)?;
    let source = PathBuf::from(&record.source_path);

    remove_existing_path(&source)?;

    if record.existed {
        let backup_rel = record
            .backup_path
            .as_ref()
            .ok_or_else(|| Error::Tool("checkpoint is missing backup data".to_string()))?;
        let backup = checkpoint_dir.join(backup_rel);
        match record
            .source_kind
            .ok_or_else(|| Error::Tool("checkpoint is missing source kind".to_string()))?
        {
            CheckpointSourceKind::File => {
                if let Some(parent) = source.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&backup, &source)?;
            }
            CheckpointSourceKind::Directory => {
                copy_dir_recursive(&backup, &source)?;
            }
        }
    }

    Ok(record)
}

fn validate_checkpoint_id(id: &str) -> Result<()> {
    let is_valid = id.len() == 32
        && id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !is_valid {
        return Err(Error::Tool(format!("invalid checkpoint id '{id}'")));
    }
    Ok(())
}

fn classify_source_kind(metadata: &fs::Metadata, source: &Path) -> Result<CheckpointSourceKind> {
    if metadata.is_file() {
        return Ok(CheckpointSourceKind::File);
    }
    if metadata.is_dir() {
        return Ok(CheckpointSourceKind::Directory);
    }
    Err(Error::Tool(format!(
        "unsupported checkpoint target '{}'",
        source.display()
    )))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&src_path)?;

        if metadata.file_type().is_symlink() {
            return Err(Error::Tool(format!(
                "refusing to checkpoint symlink path '{}'",
                src_path.display()
            )));
        }

        if metadata.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src_path, &dst_path)?;
        } else {
            return Err(Error::Tool(format!(
                "unsupported checkpoint entry '{}'",
                src_path.display()
            )));
        }
    }
    Ok(())
}

fn remove_existing_path(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    if metadata.file_type().is_symlink() {
        fs::remove_file(path)?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }

    Ok(())
}

fn write_manifest(checkpoint_dir: &Path, record: &CheckpointRecord) -> Result<()> {
    let manifest_path = checkpoint_dir.join("manifest.json");
    let payload = serde_json::to_vec_pretty(record)?;
    fs::write(manifest_path, payload)?;
    Ok(())
}

fn read_manifest(checkpoint_dir: &Path) -> Result<CheckpointRecord> {
    let manifest_path = checkpoint_dir.join("manifest.json");
    let payload = fs::read(&manifest_path)?;
    Ok(serde_json::from_slice(&payload)?)
}

fn read_all_manifests(base_dir: &Path) -> Result<Vec<CheckpointRecord>> {
    let mut items = Vec::new();
    let entries = match fs::read_dir(base_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(items),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let checkpoint_dir = entry.path();
        match read_manifest(&checkpoint_dir) {
            Ok(record) => items.push(record),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    checkpoint_dir = %checkpoint_dir.display(),
                    "failed to load checkpoint manifest"
                );
            }
        }
    }

    Ok(items)
}

// ── Parameter helpers (ported from Moltis crates/tools/src/params.rs) ────────

/// Extract a trimmed, non-empty `&str` from a JSON object field.
///
/// Returns `None` when the key is absent, null, not a string, empty,
/// or whitespace-only.
fn str_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// Like [`str_param`] but returns an error when missing.
fn require_str<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    str_param(params, key).ok_or_else(|| Error::Tool(format!("missing required parameter: {key}")))
}

/// Extract a `u64`, defaulting to `default` when absent.
fn u64_param(params: &Value, key: &str, default: u64) -> u64 {
    params.get(key).and_then(Value::as_u64).unwrap_or(default)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn checkpoint_round_trip_restores_file_content() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf());
        let path = tmp.path().join("target.txt");
        fs::write(&path, "before\n").unwrap();

        let checkpoint = manager.checkpoint_path(&path, "test.file").await.unwrap();
        fs::write(&path, "after\n").unwrap();

        manager.restore(&checkpoint.id).await.unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "before\n");
    }

    #[tokio::test]
    async fn checkpoint_round_trip_restores_directory_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf());
        let dir = tmp.path().join("skill");
        fs::create_dir_all(dir.join("templates")).unwrap();
        fs::write(dir.join("SKILL.md"), "v1\n").unwrap();
        fs::write(dir.join("templates/prompt.txt"), "hello\n").unwrap();

        let checkpoint = manager.checkpoint_path(&dir, "test.dir").await.unwrap();
        fs::write(dir.join("SKILL.md"), "v2\n").unwrap();
        fs::remove_file(dir.join("templates/prompt.txt")).unwrap();
        fs::write(dir.join("notes.txt"), "new\n").unwrap();

        manager.restore(&checkpoint.id).await.unwrap();

        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), "v1\n");
        assert_eq!(
            fs::read_to_string(dir.join("templates/prompt.txt")).unwrap(),
            "hello\n"
        );
        assert!(!dir.join("notes.txt").exists());
    }

    #[tokio::test]
    async fn restore_removes_paths_that_did_not_exist_at_checkpoint_time() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf());
        let path = tmp.path().join("created-later.txt");

        let checkpoint = manager.checkpoint_path(&path, "test.absent").await.unwrap();
        fs::write(&path, "hello\n").unwrap();

        manager.restore(&checkpoint.id).await.unwrap();

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn checkpoints_list_filters_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf());
        let alpha = tmp.path().join("alpha.txt");
        let beta = tmp.path().join("beta.txt");
        fs::write(&alpha, "alpha\n").unwrap();
        fs::write(&beta, "beta\n").unwrap();

        manager.checkpoint_path(&alpha, "alpha").await.unwrap();
        manager.checkpoint_path(&beta, "beta").await.unwrap();

        let filtered = manager.list(20, Some("beta")).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].source_path.ends_with("beta.txt"));
    }

    #[tokio::test]
    async fn restore_rejects_non_checkpoint_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf());

        let error = manager.restore("../../etc/passwd").await.unwrap_err();
        assert!(error.to_string().contains("invalid checkpoint id"));
    }

    /// Crash-recovery simulation: the manager that created the checkpoint is
    /// dropped, a fresh manager is built over the same base dir (as a restarted
    /// process would), and the restore still recovers the original content.
    #[tokio::test]
    async fn checkpoint_survives_manager_drop_and_new_manager_restores() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let path = tmp.path().join("durable.txt");
        fs::write(&path, "original\n").unwrap();

        let id = {
            let manager = CheckpointManager::new(data_dir.clone());
            let checkpoint = manager.checkpoint_path(&path, "durability").await.unwrap();
            fs::write(&path, "mutated\n").unwrap();
            // Manager dropped here: nothing is cached in memory, the manifest on
            // disk is the only source of truth.
            checkpoint.id
        };

        let reloaded = CheckpointManager::new(data_dir);
        let restored = reloaded.restore(&id).await.unwrap();
        assert_eq!(restored.id, id);
        assert_eq!(fs::read_to_string(&path).unwrap(), "original\n");
    }

    #[tokio::test]
    async fn restore_within_refuses_source_path_outside_allowed_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().join("data").to_path_buf());
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let target = outside.join("secret.txt");
        fs::write(&target, "keep\n").unwrap();

        let checkpoint = manager.checkpoint_path(&target, "outside").await.unwrap();

        // Allowed root is a sibling directory, not the parent of `outside`.
        let allowed_root = tmp.path().join("data");
        let error = manager
            .restore_within(&checkpoint.id, std::slice::from_ref(&allowed_root))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("outside the allowed roots"));
        // The source file was not touched.
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
    }
}
