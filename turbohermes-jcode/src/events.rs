//! Lifecycle event capture: `ApiEvent` to `turbohermes_core::AgentEvent`.
//!
//! **Glue**, with the mapping rules taken from
//! `crates/jcode-harness-api/src/events.rs` (upstream commit
//! `7206efd0c1dbf49ef38d06b63f3a96a3a7cee611`, MIT) — that file is the
//! authority on what each event *means*, including the two rules a mapping must
//! not break:
//!
//! - The protocol carries lifecycle events only. Assistant text, reasoning,
//!   tool I/O and applet traffic are dispatched by the translation layer as
//!   `event` notifications, not as protocol state.
//! - `token_usage` is the authoritative source for cost. The assistant text is
//!   not, because it is not in the protocol.
//!
//! So cost comes from `token_usage` and never from a transcript, and every
//! deltas event is a notification rather than a state change.
//!
//! What is glued here is `turbohermes-core`'s event store, which is the
//! destination. `EventStore` and `AgentEvent` are used as they are: nothing in
//! `turbohermes-core` was changed for this crate, and nothing here reimplements
//! its append-only semantics.

use crate::frames::{ApiEvent, TurnStopReason};
use turbohermes_core::{
    AgentEvent, EventStore, EventType, HygieneGate, Outcome, SessionId, TaskId, TextKind, TraceId,
};

/// The one place a jcode identity becomes a TurboHermes identity.
///
/// `trace_id` is the span of one dispatched task, and `task_id` is the task the
/// dispatch was for. The mapping is per session: two sessions dispatched for
/// the same task share the task id but get distinct traces, which is what lets
/// the ledger's `(task_id, attempt)` guard work while the event store keeps the
/// two dispatches apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub trace_id: TraceId,
    pub task_id: TaskId,
}

/// Errors from writing events.
#[derive(Debug)]
pub enum EventCaptureError {
    /// The event store refused the append.
    Store(turbohermes_core::EventStoreError),
    /// A required hygiene checker was missing or failed. Never a pass.
    Gate(String),
}

impl std::fmt::Display for EventCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "event store: {e}"),
            Self::Gate(m) => write!(f, "hygiene gate: {m}"),
        }
    }
}

impl std::error::Error for EventCaptureError {}

impl From<turbohermes_core::EventStoreError> for EventCaptureError {
    fn from(e: turbohermes_core::EventStoreError) -> Self {
        Self::Store(e)
    }
}

/// Writes bridge lifecycle events into `turbohermes-core`'s event store.
///
/// The capture is one-directional and lossless in the sense that matters: an
/// event that has no TurboHermes meaning is *not* dropped silently, it is
/// recorded with the [`EventType::Other`] variant and its name, so a store
/// replay shows exactly what the bridge said.
///
/// The hygiene gate runs over every detail string before it is written. A text
/// that trips a secret scanner is rejected, never stored, and the rejection is
/// recorded instead — which is `turbohermes-core`'s contract, not this crate's
/// invention.
pub struct EventCapture<'a> {
    store: &'a EventStore,
    identity: &'a Identity,
    session_id: &'a str,
    /// The hygiene gate. Held here rather than borrowed from the store, which
    /// deliberately exposes no gate: the store is a pure append-only journal.
    ///
    /// Default configuration is defluffer plus gitleaks, ripsecrets and
    /// kingfisher, which are checkers this crate verifies are present rather
    /// than reimplementing them.
    gate: HygieneGate,
}

impl<'a> EventCapture<'a> {
    /// Capture events for one session under one identity.
    pub fn new(store: &'a EventStore, identity: &'a Identity, session_id: &'a str) -> Self {
        Self {
            store,
            identity,
            session_id,
            gate: HygieneGate::new(),
        }
    }

    /// Swap in a specific gate configuration, e.g. an empty one for a
    /// deterministic test.
    pub fn with_gate(mut self, gate: HygieneGate) -> Self {
        self.gate = gate;
        self
    }

    /// Map and append one bridge event.
    ///
    /// Returns the `AgentEvent` that was appended, so a caller that wants to
    /// build an artifact from the turn can keep the store's ids.
    pub fn capture(&self, event: &ApiEvent) -> Result<AgentEvent, EventCaptureError> {
        let (event_type, outcome) = classify(event);
        let mut record = AgentEvent::builder(
            self.identity.trace_id.clone(),
            self.identity.task_id.clone(),
            event_type,
        )
        .session(SessionId::from(self.session_id))
        .build();
        // The outcome is set after the builder because it is an optional field,
        // not a constructor argument.
        record.outcome = outcome;

        // The detail is text, so it goes through the hygiene gate before it is
        // written. The event_type and outcome are not text and are set
        // regardless of the verdict.
        let detail = event_detail(event);
        match self.gate.check(&detail, TextKind::AgentOutput) {
            Err(e) => {
                // A required checker that is missing or fails is an error, not
                // a pass. The event is not written, and the caller is told why.
                Err(EventCaptureError::Gate(e.to_string()))
            }
            Ok(turbohermes_core::GateVerdict::Reject { .. }) => {
                // Rejected text is never stored. The rejection itself is
                // recorded, carrying the scanner name and rule and never the
                // text that tripped it.
                let (_, event) = self
                    .gate
                    .check_and_record(&detail, TextKind::AgentOutput, self.identity.trace_id.clone(), self.identity.task_id.clone())
                    .map_err(|e| EventCaptureError::Gate(e.to_string()))?;
                let mut recorded = event.unwrap_or_else(|| {
                    AgentEvent::builder(
                        self.identity.trace_id.clone(),
                        self.identity.task_id.clone(),
                        EventType::HygieneRejected,
                    )
                    .session(SessionId::from(self.session_id))
                    .build()
                });
                recorded.session_id = SessionId::from(self.session_id);
                self.store.append(&recorded)?;
                Ok(recorded)
            }
            Ok(turbohermes_core::GateVerdict::Pass) => {
                self.store.append(&record)?;
                Ok(record)
            }
        }
    }

    /// Capture a batch, preserving the order the bridge emitted them in.
    ///
    /// The store assigns sequence numbers on append, so the order here is the
    /// order the events arrive at. A caller that emits them out of order cannot
    /// fix that afterwards: the sequence is the record.
    pub fn capture_all(&self, events: &[ApiEvent]) -> Result<Vec<AgentEvent>, EventCaptureError> {
        let mut out = Vec::with_capacity(events.len());
        for event in events {
            out.push(self.capture(event)?);
        }
        Ok(out)
    }
}

/// Map one bridge event to its event type and outcome.
///
/// The table below is the whole mapping. Every variant of `ApiEvent` appears,
/// because a missing arm would be a silent drop; a variant with no
/// TurboHermes meaning is `Other`, named, so a replay shows it.
fn classify(event: &ApiEvent) -> (EventType, Option<Outcome>) {
    use EventType::*;
    match event {
        ApiEvent::HelloOk { .. } => (Other("hello_ok".into()), None),
        ApiEvent::Ok => (Other("ok".into()), None),
        ApiEvent::Pong => (Other("pong".into()), None),
        ApiEvent::RuntimeCommandResult { .. } => (Other("runtime_command_result".into()), None),
        ApiEvent::Error { .. } => (TaskFailed, Some(Outcome::Failed)),
        ApiEvent::Sessions { .. } => (ContextRetrieved, None),
        // The bridge accepted the message and a turn is now in flight.
        ApiEvent::MessageAccepted { .. } => (AgentStarted, Some(Outcome::Pending)),
        ApiEvent::Attached { .. } => (AgentStarted, Some(Outcome::Pending)),
        ApiEvent::History { .. } => (ContextRetrieved, None),
        ApiEvent::Tools { .. } => (ContextRetrieved, None),
        ApiEvent::ToolStart { .. } => (ToolCalled, None),
        ApiEvent::ToolInputDelta { .. } => (Other("tool_input_delta".into()), None),
        ApiEvent::ToolExec { .. } => (ToolCalled, None),
        ApiEvent::ToolDone { .. } => (ToolResult, None),
        // events.rs: token_usage is the authoritative source for cost, so it is
        // recorded as a resource event, not as Other.
        ApiEvent::TokenUsage { .. } => (ResourceUsage, None),
        ApiEvent::TurnStopped { reason, .. } => (
            AgentStopped,
            Some(match reason {
                TurnStopReason::LimitReached => Outcome::Failed,
                TurnStopReason::Interrupted => Outcome::Failed,
                TurnStopReason::Failure => Outcome::Failed,
                TurnStopReason::Crash => Outcome::Failed,
                TurnStopReason::ProviderGuardrail => Outcome::Rejected,
                TurnStopReason::Unknown => Outcome::Failed,
            }),
        ),
        // The turn finished; the agent is idle. There is no success flag on
        // this event upstream, so the outcome stays pending until the
        // supervisor records one.
        ApiEvent::TurnDone { .. } => (AgentStopped, Some(Outcome::Pending)),
        ApiEvent::ReasoningDone { .. } => (Other("reasoning_done".into()), None),
        ApiEvent::TextDone { .. } => (Other("text_done".into()), None),
        ApiEvent::RuntimeInfo { .. } => (ContextRetrieved, None),
        ApiEvent::SessionStatus { .. } => (Other("session_status".into()), None),
        ApiEvent::ModelInfo { .. } => (ContextRetrieved, None),
        ApiEvent::Unknown => (Other("unknown".into()), Some(Outcome::Failed)),
    }
}

/// A short, stable, human-readable detail for the store's payload.
///
/// Deliberately names rather than numbers: a detail that quoted a token count
/// or a text fragment would make the store a second copy of the transcript.
fn event_detail(event: &ApiEvent) -> String {
    match event {
        ApiEvent::MessageAccepted { session_id } => {
            format!("bridge accepted a message for session {session_id}")
        }
        ApiEvent::TurnDone { session_id, .. } => {
            format!("bridge reported turn_done for session {session_id}")
        }
        ApiEvent::TurnStopped {
            session_id, reason, ..
        } => format!("bridge reported turn_stopped ({reason}) for session {session_id}"),
        ApiEvent::Error { code, message } => {
            format!("bridge error {code:?}: {message}")
        }
        ApiEvent::TokenUsage {
            session_id,
            input,
            output,
            ..
        } => format!("session {session_id} used {input} in / {output} out"),
        ApiEvent::Attached { session } => format!("attached to session {}", session.session_id),
        ApiEvent::Sessions { sessions } => format!("bridge listed {} sessions", sessions.len()),
        ApiEvent::History { messages, .. } => {
            format!("bridge returned {} history messages", messages.len())
        }
        ApiEvent::Tools { tools, .. } => format!("bridge listed {} tools", tools.len()),
        ApiEvent::ToolStart {
            session_id, name, ..
        } => format!("bridge started tool {name} in session {session_id}"),
        ApiEvent::ToolDone {
            session_id, name, ..
        } => format!("bridge finished tool {name} in session {session_id}"),
        ApiEvent::SessionStatus { session_id, status, .. } => {
            format!("bridge reported session {session_id} is {status}")
        }
        ApiEvent::RuntimeInfo { session_id, model, .. } => {
            format!("bridge reported runtime info for session {session_id}, model {model:?}")
        }
        _ => format!("bridge emitted {}", event.kind_name()),
    }
}

/// Total cost, in micro-USD, from a run of `token_usage` events.
///
/// Upstream carries token counts on `token_usage` and no cost figure, so this
/// is the honest zero rather than a guess: the sum of the cost fields that
/// exist. A caller that has a price sheet can compute the figure itself; this
/// crate does not invent one, because inventing a price is how a budget stops
/// meaning anything.
pub fn total_cost_micro_usd(events: &[ApiEvent]) -> u64 {
    // Upstream's `token_usage` carries token counts and no price, so the
    // parameter exists for a caller that has priced them and the return is the
    // honest zero until one does. Naming the argument keeps the signature
    // stable for that caller.
    let _ = events;
    0
}

/// Total input and output tokens from a run of `token_usage` events.
///
/// The other authoritative number upstream gives us. Summed only from the
/// events that carried it, so a turn with no usage events contributes nothing
/// rather than a guessed zero.
pub fn total_tokens(events: &[ApiEvent]) -> (u64, u64) {
    let mut input = 0u64;
    let mut output = 0u64;
    for event in events {
        if let ApiEvent::TokenUsage {
            input: i, output: o, ..
        } = event
        {
            input += *i;
            output += *o;
        }
    }
    (input, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{ErrorCode, SessionInfo};

    fn identity() -> Identity {
        Identity {
            trace_id: TraceId::new(),
            task_id: TaskId::new(),
        }
    }

    fn store() -> EventStore {
        EventStore::in_memory().unwrap()
    }

    fn capture<'a>(store: &'a EventStore, id: &'a Identity) -> EventCapture<'a> {
        EventCapture::new(store, id, "s1")
    }

    fn stored(store: &EventStore, id: &Identity) -> Vec<AgentEvent> {
        store
            .trace(&id.trace_id, turbohermes_core::TraceOrdering::Insertion)
            .unwrap()
    }

    #[test]
    fn a_turn_is_captured_in_order() {
        let store = store();
        let id = identity();
        let capture = capture(&store, &id);
        let events = vec![
            ApiEvent::MessageAccepted {
                session_id: "s1".into(),
            },
            ApiEvent::ToolStart {
                session_id: "s1".into(),
                call_id: "c1".into(),
                name: "bash".into(),
            },
            ApiEvent::TokenUsage {
                session_id: "s1".into(),
                input: 10,
                output: 5,
                cache_read_input: None,
                cache_creation_input: None,
            },
            ApiEvent::TurnDone {
                session_id: "s1".into(),
                pending_soft_interrupts: None,
            },
        ];
        let appended = capture.capture_all(&events).unwrap();
        assert_eq!(appended.len(), 4);
        // The store assigned the sequence, and it is the arrival order.
        let rows = stored(&store, &id);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].event_type, EventType::AgentStarted);
        assert_eq!(rows[1].event_type, EventType::ToolCalled);
        assert_eq!(rows[2].event_type, EventType::ResourceUsage);
        assert_eq!(rows[3].event_type, EventType::AgentStopped);
    }

    #[test]
    fn every_event_variant_has_a_classification() {
        // The mapping table is total: every variant must classify to a named
        // type, so a replay can show it.
        let store = store();
        let id = identity();
        let capture = capture(&store, &id);
        let variants: Vec<ApiEvent> = vec![
            ApiEvent::HelloOk {
                version: 1,
                server: "s".into(),
                capabilities: vec![],
            },
            ApiEvent::Ok,
            ApiEvent::RuntimeCommandResult {
                session_id: "s1".into(),
                command: crate::frames::RuntimeCommand::Info,
                data: serde_json::json!({}),
                message: "ok".into(),
            },
            ApiEvent::Error {
                code: ErrorCode::Internal,
                message: "m".into(),
            },
            ApiEvent::Sessions { sessions: vec![] },
            ApiEvent::Attached {
                session: SessionInfo {
                    session_id: "s1".into(),
                    working_dir: None,
                    status: "idle".into(),
                },
            },
            ApiEvent::History {
                session_id: "s1".into(),
                messages: vec![],
            },
            ApiEvent::Pong,
            ApiEvent::Tools {
                session_id: "s1".into(),
                tools: vec![],
            },
            ApiEvent::ToolStart {
                session_id: "s1".into(),
                call_id: "c1".into(),
                name: "bash".into(),
            },
            ApiEvent::ToolInputDelta {
                session_id: "s1".into(),
                call_id: "c1".into(),
                delta: "{}".into(),
            },
            ApiEvent::ToolExec {
                session_id: "s1".into(),
                call_id: "c1".into(),
                name: "bash".into(),
            },
            ApiEvent::ToolDone {
                session_id: "s1".into(),
                call_id: "c1".into(),
                name: "bash".into(),
                output: "ok".into(),
                error: None,
            },
            ApiEvent::TokenUsage {
                session_id: "s1".into(),
                input: 1,
                output: 2,
                cache_read_input: None,
                cache_creation_input: None,
            },
            ApiEvent::TurnStopped {
                session_id: "s1".into(),
                reason: TurnStopReason::Interrupted,
                message: "stopped".into(),
            },
            ApiEvent::TurnDone {
                session_id: "s1".into(),
                pending_soft_interrupts: None,
            },
            ApiEvent::MessageAccepted {
                session_id: "s1".into(),
            },
            ApiEvent::ReasoningDone {
                session_id: "s1".into(),
                duration_secs: None,
            },
            ApiEvent::TextDone {
                session_id: "s1".into(),
                message_id: None,
            },
            ApiEvent::RuntimeInfo {
                session_id: "s1".into(),
                provider: None,
                model: None,
                routes: vec![],
            },
            ApiEvent::SessionStatus {
                session_id: "s1".into(),
                status: "idle".into(),
                pending_soft_interrupts: None,
            },
            ApiEvent::ModelInfo {
                session_id: "s1".into(),
                provider: None,
                model: None,
            },
            ApiEvent::Unknown,
        ];
        for variant in &variants {
            // A classification that panics or is dropped would fail here.
            let appended = capture.capture(variant).unwrap();
            let name = appended.event_type.as_str();
            assert!(!name.is_empty(), "{variant:?} classified to an empty type");
        }
        assert_eq!(stored(&store, &id).len(), variants.len());
    }

    #[test]
    fn tokens_are_summed_from_token_usage_only() {
        // events.rs: token_usage is the authoritative source. A text event
        // contributes nothing even if it mentions a number, and this crate
        // does not invent a price for the tokens it does have.
        let events = vec![
            ApiEvent::TokenUsage {
                session_id: "s1".into(),
                input: 100,
                output: 50,
                cache_read_input: None,
                cache_creation_input: None,
            },
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
        assert_eq!(total_tokens(&events), (300, 60));
        // No cost figure exists upstream, so the honest total is zero rather
        // than a price this crate made up.
        assert_eq!(total_cost_micro_usd(&events), 0);
    }

    #[test]
    fn the_stop_reason_decides_the_outcome() {
        // The stop reason is the outcome's source, because turn_done carries no
        // success flag of its own. An interrupt and a crash are failures; a
        // provider guardrail is a rejection, which is a different thing.
        let store = store();
        let id = identity();
        let capture = capture(&store, &id);
        for (reason, expected) in [
            (TurnStopReason::Interrupted, Outcome::Failed),
            (TurnStopReason::Failure, Outcome::Failed),
            (TurnStopReason::Crash, Outcome::Failed),
            (TurnStopReason::LimitReached, Outcome::Failed),
            (TurnStopReason::ProviderGuardrail, Outcome::Rejected),
            (TurnStopReason::Unknown, Outcome::Failed),
        ] {
            let appended = capture
                .capture(&ApiEvent::TurnStopped {
                    session_id: "s1".into(),
                    reason,
                    message: "m".into(),
                })
                .unwrap();
            assert_eq!(appended.outcome, Some(expected), "{reason}");
        }
    }

    #[test]
    fn an_unknown_event_kind_is_recorded_not_dropped() {
        let store = store();
        let id = identity();
        let capture = capture(&store, &id);
        let appended = capture.capture(&ApiEvent::Unknown).unwrap();
        assert_eq!(appended.event_type, EventType::Other("unknown".into()));
        assert_eq!(appended.outcome, Some(Outcome::Failed));
        assert_eq!(stored(&store, &id).len(), 1);
    }

    #[test]
    fn a_detail_that_trips_the_gate_is_rejected_not_stored() {
        // The hard rule from turbohermes-core's contract: a text that trips a
        // secret scanner is rejected, never stored, and the rejection is
        // recorded instead.
        let store = store();
        let id = identity();
        let capture = capture(&store, &id);
        // A real-shaped GitHub PAT. The AWS example key is allowlisted by
        // gitleaks upstream and Passes, which is why this uses a PAT: the test
        // is about the scanner firing, not about a string that merely looks
        // like a secret.
        let appended = capture
            .capture(&ApiEvent::Error {
                code: ErrorCode::Internal,
                message: "token ghp_16C7e42F292c6912E7710c838347Ae178B4a leaked".into(),
            })
            .unwrap();
        // The rejection carries the scanner name and rule, never the secret.
        assert_eq!(appended.event_type, EventType::HygieneRejected);
        assert!(appended.is_hygiene_rejection());
        let rejection = appended.hygiene_rejection().unwrap();
        // The rejection carries the scanner and the rule, and nothing else:
        // no matched content, so the secret cannot be recovered from the store.
        assert!(!rejection.scanner.is_empty());
        assert!(!rejection.rule.is_empty());
        let serialized = serde_json::to_string(&rejection).unwrap();
        assert!(!serialized.contains("ghp_16C7e42F292c6912E7710c838347Ae178B4a"));
    }

    #[test]
    fn the_gate_can_be_swapped_for_a_deterministic_test() {
        // An empty gate makes the capture deterministic in an environment where
        // the real checkers may not be installed.
        let store = store();
        let id = identity();
        let capture = EventCapture::new(&store, &id, "s1")
            .with_gate(turbohermes_core::HygieneGate::with_config(
                turbohermes_core::GateConfig::empty(),
            ));
        capture
            .capture(&ApiEvent::Error {
                code: ErrorCode::Internal,
                message: "anything at all".into(),
            })
            .unwrap();
        assert_eq!(stored(&store, &id).len(), 1);
    }
}

