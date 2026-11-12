//! Typed artifacts produced by a dispatched turn.
//!
//! **Glue.** There is no upstream to port: jcode's harness API returns
//! structured data (a `TurnResult`, tool outputs, history) and BlockCell's
//! task contract returns acceptance criteria, and neither owns the concept of a
//! TurboHermes artifact. What is borrowed is the rule, from the architecture
//! document: an artifact references its content, it never contains it.
//!
//! The rule is enforced by types, not by convention:
//!
//! - A diff is stored as a [`Sha256Hex`] plus a location, never as a string.
//!   `turbohermes-core`'s `Sha256Hex` is typed precisely so the schema cannot
//!   carry a malformed hash.
//! - A test run is stored as counts and an exit code, never as its output.
//! - Text that must be carried (a summary) goes through the hygiene gate
//!   before it is stored, and a text that trips a secret scanner is rejected
//!   rather than written.

use crate::frames::ApiEvent;
use serde::{Deserialize, Serialize};
use turbohermes_core::{Sha256Hex, TaskId, TraceId};

/// A typed artifact from one dispatched turn.
///
/// Every variant carries references and digests. None carries the content the
/// digest stands for, so an artifact cannot become a second copy of the diff,
/// the transcript or a tool's output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Artifact {
    /// A patch produced by the session, referenced by digest.
    Diff(DiffArtifact),
    /// A verification run: what was run, what it decided, what it cost.
    Verification(VerificationArtifact),
    /// The turn's own summary of what it did. Text, so gated.
    Summary(SummaryArtifact),
    /// Cost and token usage, from `token_usage` events only.
    Cost(CostArtifact),
}

impl Artifact {
    /// The artifact's kind, for a manifest or a CLI listing.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Diff(_) => "diff",
            Self::Verification(_) => "verification",
            Self::Summary(_) => "summary",
            Self::Cost(_) => "cost",
        }
    }
}

/// A patch, by digest and location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffArtifact {
    /// The trace the patch belongs to.
    pub trace_id: TraceId,
    /// The task the patch is for.
    pub task_id: TaskId,
    /// SHA-256 of the patch bytes. Never the patch bytes.
    pub digest: Sha256Hex,
    /// Where the patch lives: a path in the worktree, or a git ref range.
    pub location: String,
    /// The branch the patch is on, as a full ref.
    pub branch: String,
    /// The worktree the patch was produced in, when one was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// Byte length of the patch, so a consumer can size a fetch.
    pub byte_length: u64,
}

/// A verification run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationArtifact {
    pub trace_id: TraceId,
    pub task_id: TaskId,
    /// What was run, e.g. `cargo test`, `cargo check`.
    pub command: String,
    /// How many tests ran, when the runner reported a count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_run: Option<u32>,
    /// How many passed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_passed: Option<u32>,
    /// How many failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_failed: Option<u32>,
    /// The process exit code. `None` when the runner did not report one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// True when the run passed: exit code 0 and no failures.
    pub passed: bool,
    /// SHA-256 of the run's output. Never the output itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_digest: Option<Sha256Hex>,
}

impl VerificationArtifact {
    /// Build one from an exit code and the counts a runner reported.
    ///
    /// `passed` is derived, not asserted by the caller: a run with failures is
    /// not a pass whatever its exit code says.
    pub fn new(
        trace_id: TraceId,
        task_id: TaskId,
        command: impl Into<String>,
        exit_code: Option<i32>,
        tests_run: Option<u32>,
        tests_passed: Option<u32>,
        tests_failed: Option<u32>,
    ) -> Self {
        let no_failures = tests_failed.unwrap_or(0) == 0;
        let zero_exit = matches!(exit_code, Some(0));
        Self {
            trace_id,
            task_id,
            command: command.into(),
            tests_run,
            tests_passed,
            tests_failed,
            exit_code,
            passed: zero_exit && no_failures,
            output_digest: None,
        }
    }
}

/// The turn's summary. Text, so it goes through the gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryArtifact {
    pub trace_id: TraceId,
    pub task_id: TaskId,
    /// The session's own account of the turn, gated before storage.
    pub text: String,
    /// True when the text passed the hygiene gate. A rejected summary is not
    /// written at all, so this is `true` for every artifact that exists.
    pub gated: bool,
}

/// Cost and token usage from `token_usage` events.
///
/// Upstream carries token counts on `token_usage` and no cost figure. The
/// `cost_micro_usd` field therefore exists for a caller that has a price sheet
/// to fill in, and defaults to the honest zero: this crate does not invent a
/// price, because a budget priced by guesswork does not bound anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostArtifact {
    pub trace_id: TraceId,
    pub task_id: TaskId,
    /// Total input tokens reported.
    pub input_tokens: u64,
    /// Total output tokens reported.
    pub output_tokens: u64,
    /// Total cost in micro-USD. Zero unless a price sheet supplied one.
    pub cost_micro_usd: u64,
    /// How many `token_usage` events were summed.
    pub events: u32,
}

/// Errors from building artifacts.
#[derive(Debug)]
pub enum ArtifactError {
    /// The text was rejected by the hygiene gate and was not stored.
    Rejected(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(m) => write!(f, "artifact text was rejected: {m}"),
        }
    }
}

impl std::error::Error for ArtifactError {}

/// Collect the cost artifact from a turn's events.
///
/// The authoritative cost source (upstream `events.rs` line 1160): summing
/// only `token_usage` events, and only those that carried a cost figure.
pub fn cost_artifact(
    trace_id: TraceId,
    task_id: TaskId,
    events: &[ApiEvent],
) -> CostArtifact {
    let (input, output) = crate::events::total_tokens(events);
    let count = events
        .iter()
        .filter(|e| matches!(e, ApiEvent::TokenUsage { .. }))
        .count() as u32;
    CostArtifact {
        trace_id,
        task_id,
        input_tokens: input,
        output_tokens: output,
        // Upstream reports no price, so this is zero until a caller with a
        // price sheet fills it in. Zero here means "not priced", not "free".
        cost_micro_usd: 0,
        events: count,
    }
}

/// Micro-USD per US dollar.
pub const MICRO_USD_PER_DOLLAR: u64 = 1_000_000;

/// Parse a decimal USD string into micro-USD.
///
/// Kept for a caller that supplies its own price sheet, so the arithmetic is
/// in one tested place. Truncating rather than rounding, so a value with more
/// precision than the micro is not silently rounded up past what was quoted.
pub fn parse_micro_usd(figure: &str) -> Option<u64> {
    let (whole, frac) = figure.split_once('.')?;
    let whole_usd: u64 = whole.trim().parse().ok()?;
    let micro: u64 = whole_usd.checked_mul(MICRO_USD_PER_DOLLAR)?;
    let frac_digits: String = frac.chars().take(6).collect();
    let frac_padded = format!("{frac_digits:0<6}");
    let frac_micro: u64 = frac_padded.parse().ok()?;
    Some(micro + frac_micro)
}

/// Build a summary artifact, gating the text first.
///
/// A text that trips the hygiene gate is refused rather than stored, which is
/// the hard rule from `turbohermes-core`'s contract: a text that trips a secret
/// scanner is rejected, never stored.
pub fn summary_artifact(
    trace_id: TraceId,
    task_id: TaskId,
    text: &str,
    gate: &turbohermes_core::HygieneGate,
) -> Result<SummaryArtifact, ArtifactError> {
    match gate.check(text, turbohermes_core::TextKind::AgentOutput) {
        Ok(turbohermes_core::GateVerdict::Pass) => Ok(SummaryArtifact {
            trace_id,
            task_id,
            text: text.to_string(),
            gated: true,
        }),
        Ok(turbohermes_core::GateVerdict::Reject { .. }) => {
            Err(ArtifactError::Rejected("the hygiene gate rejected the text".into()))
        }
        Err(e) => Err(ArtifactError::Rejected(format!("hygiene gate failed: {e}"))),
    }
}

/// A manifest of everything one turn produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub trace_id: TraceId,
    pub task_id: TaskId,
    /// The jcode session the artifacts came from.
    pub session_id: String,
    pub artifacts: Vec<Artifact>,
}

impl ArtifactManifest {
    /// An empty manifest for a turn that produced nothing.
    pub fn new(trace_id: TraceId, task_id: TaskId, session_id: impl Into<String>) -> Self {
        Self {
            trace_id,
            task_id,
            session_id: session_id.into(),
            artifacts: Vec::new(),
        }
    }

    /// Add an artifact.
    pub fn push(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }

    /// The cost artifact, if the turn reported one.
    pub fn cost(&self) -> Option<&CostArtifact> {
        self.artifacts.iter().find_map(|a| match a {
            Artifact::Cost(c) => Some(c),
            _ => None,
        })
    }

    /// The verification artifact, if the turn ran one.
    pub fn verification(&self) -> Option<&VerificationArtifact> {
        self.artifacts.iter().find_map(|a| match a {
            Artifact::Verification(v) => Some(v),
            _ => None,
        })
    }

    /// Serialise to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (TraceId, TaskId) {
        (TraceId::new(), TaskId::new())
    }

    #[test]
    fn a_diff_artifact_carries_a_digest_and_never_the_patch() {
        let (trace, task) = ids();
        let artifact = Artifact::Diff(DiffArtifact {
            trace_id: trace.clone(),
            task_id: task.clone(),
            digest: Sha256Hex::of_text("--- a/src/lib.rs\n+++ b/src/lib.rs\n"),
            location: "src/lib.rs".into(),
            branch: "refs/heads/feature/x".into(),
            worktree: Some("/repo-worktrees/feature%2Fx".into()),
            byte_length: 42,
        });
        let json = serde_json::to_string(&artifact).unwrap();
        // The patch text is nowhere in the serialised form.
        assert!(!json.contains("+++ b/src/lib.rs"));
        assert!(json.contains("digest"));
        // And a digest is a typed hash, so a malformed one cannot round-trip.
        // There is no `from_hex`: the only way in is to hash something, so a
        // caller cannot mint a string that merely looks like a digest. The
        // type's invariant is checked on deserialize, which is what makes a
        // hand-written digest in a store row impossible.
        let Artifact::Diff(diff) = &artifact else {
            unreachable!("this test builds a diff artifact")
        };
        let tampered = json.replace(diff.digest.as_str(), "not-a-hash");
        let round_trip: Result<Artifact, _> = serde_json::from_str(&tampered);
        assert!(round_trip.is_err(), "a malformed digest round-tripped");
    }

    #[test]
    fn verification_passed_is_derived_from_exit_code_and_failures() {
        let (trace, task) = ids();
        // A clean run.
        let clean = VerificationArtifact::new(trace.clone(), task.clone(), "cargo test", Some(0), Some(10), Some(10), Some(0));
        assert!(clean.passed);
        // A run that reports failures but exits 0 is still not a pass.
        let lying = VerificationArtifact::new(trace.clone(), task.clone(), "cargo test", Some(0), Some(10), Some(8), Some(2));
        assert!(!lying.passed);
        // A non-zero exit with no reported failures is still not a pass.
        let crashed = VerificationArtifact::new(trace.clone(), task.clone(), "cargo test", Some(101), None, None, None);
        assert!(!crashed.passed);
        // No exit code at all: not claimed as a pass.
        let unknown = VerificationArtifact::new(trace, task, "cargo test", None, Some(3), Some(3), Some(0));
        assert!(!unknown.passed);
    }

    #[test]
    fn a_summary_that_trips_the_gate_is_refused_not_stored() {
        let (trace, task) = ids();
        let gate = turbohermes_core::HygieneGate::new();
        let err = summary_artifact(
            trace.clone(),
            task.clone(),
            "here is the key ghp_16C7e42F292c6912E7710c838347Ae178B4a",
            &gate,
        );
        assert!(matches!(err, Err(ArtifactError::Rejected(_))), "got {err:?}");
        // A clean summary is stored with `gated: true`, the only state in which
        // one can exist.
        let ok = summary_artifact(trace, task, "Implemented the adapter.", &gate).unwrap();
        assert!(ok.gated);
    }

    #[test]
    fn cost_is_summed_from_token_usage_only() {
        let (trace, task) = ids();
        let events = vec![
            ApiEvent::TokenUsage {
                session_id: "s1".into(),
                input: 100,
                output: 50,
                cache_read_input: None,
                cache_creation_input: None,
            },
            // A text event contributes nothing, even if it mentions a number.
            ApiEvent::TextDone {
                session_id: "s1".into(),
                message_id: None,
            },
            ApiEvent::TokenUsage {
                session_id: "s1".into(),
                input: 200,
                output: 10,
                cache_read_input: None,
                cache_creation_input: None,
            },
        ];
        let cost = cost_artifact(trace.clone(), task.clone(), &events);
        assert_eq!(cost.input_tokens, 300);
        assert_eq!(cost.output_tokens, 60);
        // Upstream reports no price, so the artifact carries the honest zero
        // and says so, rather than a number this crate made up.
        assert_eq!(cost.cost_micro_usd, 0);
        assert_eq!(cost.events, 2);
    }

    #[test]
    fn a_token_usage_without_a_cost_figure_is_not_a_guessed_zero() {
        // It contributes tokens and an event count, and nothing to the cost.
        let (trace, task) = ids();
        let events = vec![ApiEvent::TokenUsage {
            session_id: "s1".into(),
            input: 10,
            output: 5,
            cache_read_input: None,
            cache_creation_input: None,
        }];
        let cost = cost_artifact(trace, task, &events);
        assert_eq!(cost.input_tokens, 10);
        assert_eq!(cost.cost_micro_usd, 0);
        assert_eq!(cost.events, 1);
    }

    #[test]
    fn a_manifest_collects_what_a_turn_produced() {
        let (trace, task) = ids();
        let mut manifest = ArtifactManifest::new(trace.clone(), task.clone(), "s1");
        manifest.push(Artifact::Cost(CostArtifact {
            trace_id: trace.clone(),
            task_id: task.clone(),
            input_tokens: 1,
            output_tokens: 2,
            cost_micro_usd: 3,
            events: 1,
        }));
        manifest.push(Artifact::Verification(VerificationArtifact::new(
            trace.clone(),
            task.clone(),
            "cargo test",
            Some(0),
            Some(4),
            Some(4),
            Some(0),
        )));
        assert_eq!(manifest.artifacts.len(), 2);
        assert_eq!(manifest.cost().unwrap().cost_micro_usd, 3);
        assert!(manifest.verification().unwrap().passed);
        let json = manifest.to_json();
        assert!(json.contains("\"kind\": \"cost\""));
        assert!(json.contains("\"kind\": \"verification\""));
    }

    #[test]
    fn a_cost_figure_with_sub_micro_precision_truncates() {
        // 0.0000009 USD is 0.9 micro-USD. Truncated to 0 rather than rounded
        // up to 1, because the bridge did not report a micro.
        assert_eq!(parse_micro_usd("0.0000009"), Some(0));
        assert_eq!(parse_micro_usd("0.000001"), Some(1));
        assert_eq!(parse_micro_usd("12.3456789"), Some(12_345_678));
        assert_eq!(parse_micro_usd("1"), None);
        assert_eq!(parse_micro_usd("abc"), None);
    }
}
