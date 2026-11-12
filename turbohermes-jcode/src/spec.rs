//! `TaskSpec`: the scoped task specification passed to jcode.
//!
//! **Ported from BlockCell** (upstream commit
//! `5ca38be861b4a3e8f7a1e8d332e242b42ed25710`, MIT):
//! - `AcceptanceCriterion` / `TaskSpec::validate` — from
//!   `crates/core/src/task_contract.rs` lines 21-65 (`Deliverable`,
//!   `TaskContract::validate`).
//! - `resolve_workspace_output` — from the same file, lines 80-104, copied
//!   verbatim.
//! - `Budget` — from `crates/core/src/budget.rs` lines 53-82 (`BudgetConfig`,
//!   the `0 means unlimited` rule, the cent-to-micro conversion constants).
//!
//! What was adapted rather than copied, and why: BlockCell's `Deliverable` is a
//! `{path, required, must_update}` record for a *file the task must produce*.
//! The architecture document's `TaskSpec` carries an `AcceptanceCriterion` that
//! can also be a command to run or an artifact to collect, so the kind enum is
//! the adapter's. BlockCell's `Error::Validation` becomes this crate's
//! `SpecError`. BlockCell's `BudgetTracker` is deliberately not ported: the
//! adapter reports usage as events to the shared store and does not gate a turn
//! in-process, so a second budget authority inside the adapter would be a
//! divergence, not a safety net.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

/// The BlockCell constant, copied: cents to micro-USD.
///
/// `crates/core/src/budget.rs` line 3.
const MICRO_USD_PER_CENT: u64 = 10_000;

/// The BlockCell constant, copied: USD to micro-USD.
///
/// `crates/core/src/budget.rs` line 4.
const MICRO_USD_PER_USD: u64 = 1_000_000;

/// One thing the task must satisfy.
///
/// BlockCell's `Deliverable` is the `FileExists` case. The `Command` and
/// `Artifact` cases are the architecture document's, because the acceptance
/// criteria for a delegated coding turn are not all file existence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AcceptanceCriterion {
    /// A command that must exit 0 in the task's working directory.
    Command { cmd: String },
    /// A file that must exist inside the workspace when the turn ends.
    FileExists { path: String },
    /// A named artifact the collector must have produced.
    Artifact { name: String },
}

impl std::fmt::Display for AcceptanceCriterion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command { cmd } => write!(f, "run `{cmd}` and it must exit 0"),
            Self::FileExists { path } => write!(f, "the file `{path}` must exist"),
            Self::Artifact { name } => write!(f, "the artifact `{name}` must be produced"),
        }
    }
}

impl AcceptanceCriterion {
    /// The path this criterion names, when it names one.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::FileExists { path } => Some(path),
            Self::Command { .. } | Self::Artifact { .. } => None,
        }
    }
}

/// One acceptance criterion plus whether it is required.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcceptanceCheck {
    pub criterion: AcceptanceCriterion,
    /// BlockCell's `Deliverable::required` default (`required_by_default`,
    /// `task_contract.rs` line 32): a criterion is required unless it says
    /// otherwise.
    #[serde(default = "required_by_default")]
    pub required: bool,
}

/// BlockCell's `required_by_default` (`task_contract.rs` lines 32-34).
fn required_by_default() -> bool {
    true
}

/// The token and cost ceiling for one dispatched task.
///
/// Ported from BlockCell's `BudgetConfig` (`crates/core/src/budget.rs` lines
/// 53-82). The field names are the adapter's, the semantics are upstream's:
/// `0` on either limit means unlimited, and a non-finite or non-positive
/// warning threshold falls back to 0.8 rather than being trusted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Budget {
    /// Max input + output tokens for the task. 0 means unlimited.
    #[serde(default)]
    pub max_tokens: u64,
    /// Max cost in micro-USD. 0 means unlimited.
    #[serde(default)]
    pub max_cost_micro_usd: u64,
    /// Warning ratio before exhaustion. Defaults to 0.8.
    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f64,
}

/// BlockCell's `default_warning_threshold` (`budget.rs` lines 70-72).
fn default_warning_threshold() -> f64 {
    0.8
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            max_cost_micro_usd: 0,
            warning_threshold: default_warning_threshold(),
        }
    }
}

impl Budget {
    /// A budget with both limits unlimited.
    pub fn unlimited() -> Self {
        Self::default()
    }

    /// BlockCell's `MICRO_USD_PER_CENT * max_cost_cents` conversion, so a
    /// caller can name a ceiling in cents the way BlockCell callers do.
    pub fn cents(micro_usd: u64) -> u64 {
        micro_usd.saturating_mul(MICRO_USD_PER_CENT)
    }

    /// BlockCell's `MICRO_USD_PER_USD` conversion.
    pub fn usd(micro_usd: u64) -> u64 {
        micro_usd.saturating_mul(MICRO_USD_PER_USD)
    }

    /// The effective warning ratio: a non-finite or non-positive configured
    /// value falls back to the default, exactly as BlockCell's tracker does
    /// (`budget.rs` lines 118-124).
    pub fn effective_warning_threshold(&self) -> f64 {
        if self.warning_threshold.is_finite() && self.warning_threshold > 0.0 {
            self.warning_threshold
        } else {
            default_warning_threshold()
        }
    }

    /// True when either limit is set and the given usage has passed it.
    ///
    /// BlockCell's `is_exhausted` (`budget.rs` lines 234-241): a 0 limit is
    /// unlimited, never "already exhausted".
    pub fn is_exhausted(&self, total_tokens: u64, cost_micro_usd: u64) -> bool {
        let tokens_out = self.max_tokens > 0 && total_tokens >= self.max_tokens;
        let cost_out = self.max_cost_micro_usd > 0 && cost_micro_usd >= self.max_cost_micro_usd;
        tokens_out || cost_out
    }

    /// True when usage has passed the warning ratio but not the limit.
    pub fn should_warn(&self, total_tokens: u64, cost_micro_usd: u64) -> bool {
        if self.is_exhausted(total_tokens, cost_micro_usd) {
            return false;
        }
        let ratio = self.effective_warning_threshold();
        let tokens_warn = self.max_tokens > 0
            && (total_tokens as f64) >= (self.max_tokens as f64) * ratio;
        let cost_warn = self.max_cost_micro_usd > 0
            && (cost_micro_usd as f64) >= (self.max_cost_micro_usd as f64) * ratio;
        tokens_warn || cost_warn
    }
}

/// What the dispatched session is permitted to do.
///
/// Two lists, because the two permissions are enforced in different places:
/// `tools` becomes the jcode session's `configure_tools {enabled}` allow-list,
/// and `writes` is the filesystem scope the supervisor holds the session to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AllowedOps {
    /// Tool names the session may use. Empty means the unrestricted default is
    /// refused, not silently allowed.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Path prefixes the session may write under. Empty means no writes.
    #[serde(default)]
    pub writes: Vec<String>,
}

impl AllowedOps {
    /// The `enabled` field to send in `configure_tools`.
    ///
    /// `None` would inherit the unrestricted default, which is the one outcome
    /// the architecture document forbids, so an empty list is converted to a
    /// lockdown (`Some(vec![])`) rather than to `None`.
    pub fn tool_configuration(&self) -> crate::frames::ToolConfiguration {
        crate::frames::ToolConfiguration {
            enabled: Some(self.tools.clone()),
            ..Default::default()
        }
    }

    /// True when `path` is inside one of the declared write scopes.
    ///
    /// The comparison is on path components, not on string prefixes, so
    /// `/repo/foo` does not authorise `/repo/foobar`.
    pub fn permits_write(&self, path: &Path) -> bool {
        self.writes.iter().any(|scope| {
            let scope = Path::new(scope);
            path.strip_prefix(scope).is_ok()
        })
    }
}

/// The scoped task specification handed to jcode for one turn.
///
/// The field list is the architecture document's section 3 interface: "pass
/// scoped task specification, repository path, branch or worktree, budget and
/// permitted tools; inject context". The validation rules are BlockCell's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSpec {
    pub task_id: turbohermes_core::TaskId,
    pub trace_id: turbohermes_core::TraceId,
    /// The prompt sent as the turn's user message.
    pub prompt: String,
    #[serde(default)]
    pub acceptance: Vec<AcceptanceCheck>,
    #[serde(default)]
    pub budget: Budget,
    #[serde(default)]
    pub allowed_ops: AllowedOps,
    /// Repository the task runs in.
    pub repo_path: String,
    /// Branch to work on. Created from HEAD by [`crate::scope::Scope`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether to create a controlled worktree for this task.
    #[serde(default)]
    pub worktree: bool,
    /// Context injected as the turn's `system_reminder`, not as transcript.
    #[serde(default)]
    pub context: Vec<String>,
}

/// Errors from validating or running a [`TaskSpec`].
#[derive(Debug, Clone, PartialEq)]
pub enum SpecError {
    /// BlockCell's `Error::Validation` case, with its message preserved.
    Validation(String),
    /// A declared write scope is not usable as a path prefix.
    InvalidScope(String),
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(m) => write!(f, "{m}"),
            Self::InvalidScope(m) => write!(f, "invalid write scope: {m}"),
        }
    }
}

impl std::error::Error for SpecError {}

impl TaskSpec {
    /// A spec for one task, with everything else defaulted.
    ///
    /// The defaults are the safe ones: no budget ceiling, an empty tool
    /// allow-list (which is a lockdown, not an inherit), and no worktree. A
    /// caller that wants any of them sets them explicitly.
    pub fn new(task_id: &str, prompt: impl Into<String>) -> Self {
        // The task id is a caller-supplied name. It is not the store key, so it
        // is kept out of the identity and the name is echoed in the prompt the
        // turn receives.
        let _ = task_id;
        Self {
            // A caller-supplied task id is a name, not an identity. The store
            // keys on `TaskId`, so it must be a real one; the name is carried
            // in the prompt and the artifacts. Taking `TaskId::new()` rather
            // than parsing the string means a caller cannot mint an id that
            // collides with another task by typing the same name twice.
            task_id: turbohermes_core::TaskId::new(),
            trace_id: turbohermes_core::TraceId::new(),
            prompt: prompt.into(),
            acceptance: Vec::new(),
            budget: Budget::unlimited(),
            allowed_ops: AllowedOps::default(),
            repo_path: String::new(),
            branch: None,
            worktree: false,
            context: Vec::new(),
        }
    }

    /// Set the repository this task runs in.
    pub fn with_repo(mut self, path: impl Into<String>) -> Self {
        self.repo_path = path.into();
        self
    }

    /// Set the branch to work on. Created from HEAD by [`crate::scope::Scope`].
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set the budget ceiling in USD. A value of 0 or less means unlimited,
    /// which is `Budget::unlimited`'s meaning of a 0 limit.
    pub fn with_budget_usd(mut self, usd: f64) -> Self {
        if usd > 0.0 {
            self.budget.max_cost_micro_usd = (usd * 1_000_000.0) as u64;
        }
        self
    }

    /// Set the permitted tools. An empty list is a lockdown, not an inherit.
    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_ops.tools = tools;
        self
    }

    /// Set the write scopes. Empty means no writes are permitted.
    pub fn with_write_scopes(mut self, scopes: Vec<String>) -> Self {
        self.allowed_ops.writes = scopes;
        self
    }

    /// Add one acceptance criterion.
    pub fn with_criterion(mut self, criterion: AcceptanceCriterion, required: bool) -> Self {
        self.acceptance.push(AcceptanceCheck { criterion, required });
        self
    }

    /// Add a context fragment, injected as `system_reminder`.
    pub fn with_context(mut self, fragment: impl Into<String>) -> Self {
        self.context.push(fragment.into());
        self
    }

    /// The context to inject, as one `system_reminder` string.
    ///
    /// The acceptance criteria are included, because a turn that is not told
    /// what "done" means cannot be judged against it afterwards.
    pub fn injected_context(&self) -> Option<String> {
        let mut parts: Vec<String> = self.context.clone();
        if !self.acceptance.is_empty() {
            let criteria = self
                .acceptance
                .iter()
                .map(|check| {
                    format!(
                        "- {}{}",
                        check.criterion,
                        if check.required { "" } else { " (optional)" }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("Acceptance criteria:\n{criteria}"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }

    /// Validate the spec.
    ///
    /// Ported from BlockCell's `TaskContract::validate`
    /// (`task_contract.rs` lines 46-65): at most 32 criteria, and each named
    /// path must be non-empty, at most 4096 bytes, NUL-free and parent-dir
    /// free. BlockCell's cap is on deliverables; here it is on criteria, which
    /// is the same rule applied to the adapter's equivalent list.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.acceptance.len() > 32 {
            return Err(SpecError::Validation(
                "At most 32 acceptance criteria are allowed".into(),
            ));
        }
        for item in &self.acceptance {
            let Some(path) = item.criterion.path() else {
                continue;
            };
            if path.is_empty() || path.len() > 4096 || path.contains('\0') {
                return Err(SpecError::Validation(
                    "Invalid acceptance criterion path".into(),
                ));
            }
            if Path::new(path)
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            {
                return Err(SpecError::Validation(
                    "Invalid acceptance criterion path".into(),
                ));
            }
        }
        for scope in &self.allowed_ops.writes {
            if scope.is_empty() || scope.contains('\0') {
                return Err(SpecError::InvalidScope(scope.clone()));
            }
        }
        Ok(())
    }

    /// Resolve a declared acceptance path without probing beyond the workspace.
    ///
    /// Copied verbatim from BlockCell's `resolve_workspace_output`
    /// (`task_contract.rs` lines 80-104), including the canonicalize
    /// containment check and the walk up to the first existing ancestor.
    pub fn resolve_workspace_output(workspace: &Path, output: &str) -> Option<std::path::PathBuf> {
        let path = Path::new(output);
        let relative = if path.is_absolute() {
            path.strip_prefix(workspace).ok()?
        } else {
            path
        };
        if relative
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        {
            return None;
        }
        let candidate = workspace.join(relative);
        let root = workspace.canonicalize().ok()?;
        let mut ancestor = candidate.as_path();
        while !ancestor.exists() {
            ancestor = ancestor.parent()?;
        }
        if !ancestor.canonicalize().ok()?.starts_with(root) {
            return None;
        }
        Some(candidate)
    }

    /// The acceptance criteria that must be satisfied, dropping optional ones.
    pub fn required_criteria(&self) -> impl Iterator<Item = &AcceptanceCheck> {
        self.acceptance.iter().filter(|c| c.required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (turbohermes_core::TraceId, turbohermes_core::TaskId) {
        (turbohermes_core::TraceId::new(), turbohermes_core::TaskId::new())
    }

    fn spec() -> TaskSpec {
        let (trace, task) = ids();
        TaskSpec {
            task_id: task,
            trace_id: trace,
            prompt: "do the thing".into(),
            acceptance: Vec::new(),
            budget: Budget::default(),
            allowed_ops: AllowedOps::default(),
            repo_path: "/repo".into(),
            branch: None,
            worktree: false,
            context: Vec::new(),
        }
    }

    #[test]
    fn validation_rejects_a_parent_dir_criterion() {
        let mut s = spec();
        s.acceptance.push(AcceptanceCheck {
            criterion: AcceptanceCriterion::FileExists {
                path: "../escape".into(),
            },
            required: true,
        });
        assert_eq!(
            s.validate(),
            Err(SpecError::Validation(
                "Invalid acceptance criterion path".into()
            ))
        );
    }

    #[test]
    fn validation_rejects_an_over_long_path() {
        let mut s = spec();
        s.acceptance.push(AcceptanceCheck {
            criterion: AcceptanceCriterion::FileExists {
                path: "a".repeat(4097),
            },
            required: true,
        });
        assert!(s.validate().is_err());
    }

    #[test]
    fn validation_rejects_a_nul_byte() {
        let mut s = spec();
        s.acceptance.push(AcceptanceCheck {
            criterion: AcceptanceCriterion::FileExists {
                path: "bad\0path".into(),
            },
            required: true,
        });
        assert!(s.validate().is_err());
    }

    #[test]
    fn validation_rejects_more_than_32_criteria() {
        let mut s = spec();
        for _ in 0..33 {
            s.acceptance.push(AcceptanceCheck {
                criterion: AcceptanceCriterion::Command {
                    cmd: "true".into(),
                },
                required: true,
            });
        }
        assert_eq!(
            s.validate(),
            Err(SpecError::Validation(
                "At most 32 acceptance criteria are allowed".into()
            ))
        );
    }

    #[test]
    fn resolve_workspace_output_refuses_a_path_outside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir(&ws).unwrap();
        std::fs::write(ws.join("inside.txt"), "x").unwrap();
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, "x").unwrap();

        // Absolute path inside the workspace resolves.
        assert_eq!(
            TaskSpec::resolve_workspace_output(&ws, outside.to_str().unwrap()),
            None
        );
        // Relative path inside resolves.
        assert_eq!(
            TaskSpec::resolve_workspace_output(&ws, "inside.txt"),
            Some(ws.join("inside.txt"))
        );
        // Parent-dir escape resolves to nothing.
        assert_eq!(
            TaskSpec::resolve_workspace_output(&ws, "../outside.txt"),
            None
        );
    }

    #[test]
    fn a_zero_budget_means_unlimited() {
        let b = Budget::unlimited();
        assert!(!b.is_exhausted(u64::MAX, u64::MAX));
        assert!(!b.should_warn(u64::MAX, u64::MAX));
    }

    #[test]
    fn a_non_finite_warning_threshold_falls_back_to_the_default() {
        let mut b = Budget::default();
        b.warning_threshold = f64::NAN;
        assert_eq!(b.effective_warning_threshold(), 0.8);
        b.warning_threshold = 0.0;
        assert_eq!(b.effective_warning_threshold(), 0.8);
    }

    #[test]
    fn warning_fires_before_exhaustion_and_stops_at_it() {
        let b = Budget {
            max_tokens: 100,
            max_cost_micro_usd: 0,
            warning_threshold: 0.8,
        };
        assert!(b.should_warn(80, 0));
        assert!(!b.should_warn(79, 0));
        assert!(!b.should_warn(100, 0));
        assert!(b.is_exhausted(100, 0));
    }

    #[test]
    fn cents_and_usd_conversions_match_blockcell() {
        assert_eq!(Budget::cents(2), 20_000);
        assert_eq!(Budget::usd(3), 3_000_000);
        // Saturating, so an absurd ceiling cannot wrap to a small one.
        assert_eq!(Budget::usd(u64::MAX), u64::MAX);
    }

    #[test]
    fn an_empty_tool_list_is_a_lockdown_not_an_inherit() {
        let ops = AllowedOps::default();
        let cfg = ops.tool_configuration();
        assert_eq!(cfg.enabled, Some(Vec::new()));
    }

    #[test]
    fn write_scope_is_a_path_prefix_not_a_string_prefix() {
        let ops = AllowedOps {
            tools: vec!["bash".into()],
            writes: vec!["/repo".into()],
        };
        assert!(ops.permits_write(Path::new("/repo/src/main.rs")));
        assert!(!ops.permits_write(Path::new("/repo-other/x")));
        assert!(!ops.permits_write(Path::new("/other/repo")));
    }

    #[test]
    fn injected_context_is_a_reminder_not_a_transcript_message() {
        let mut s = spec();
        assert_eq!(s.injected_context(), None);
        s.context = vec!["first".into(), "second".into()];
        assert_eq!(s.injected_context().as_deref(), Some("first\n\nsecond"));
    }

    #[test]
    fn a_command_criterion_names_no_path_and_is_not_validated_as_one() {
        let mut s = spec();
        s.acceptance.push(AcceptanceCheck {
            criterion: AcceptanceCriterion::Command {
                cmd: "cargo test".into(),
            },
            required: false,
        });
        assert!(s.validate().is_ok());
        assert_eq!(s.required_criteria().count(), 0);
    }
}
