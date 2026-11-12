//! NDJSON frames: the wire vocabulary of jcode's harness API bridge.
//!
//! **Ported from jcode** (upstream commit
//! `7206efd0c1dbf49ef38d06b63f3a96a3a7cee611`, MIT):
//! - `read_frame` / `write_frame` / `FrameError` — verbatim from
//!   `crates/jcode-harness-api/src/client.rs` lines 8-67.
//! - `ClientFrame` / `ServerFrame` — from `crates/jcode-harness-api/src/lib.rs`
//!   lines 58-90.
//! - `ApiRequest` — from `crates/jcode-harness-api/src/requests.rs`.
//! - `ApiEvent`, `SessionInfo`, `HistoryMessage` — from
//!   `crates/jcode-harness-api/src/events.rs`.
//! - `TurnStopReason` — from `crates/jcode-session-types/src/lib.rs` lines
//!   1113-1124.
//!
//! The two enums are internally tagged (`req` / `ev`) with `rename_all =
//! "snake_case"` and a `#[serde(other)] Unknown` catch-all. That contract is
//! upstream's, and it is what makes a bridge running a newer jcode talk to this
//! adapter: an event kind this version does not know deserialises to `Unknown`
//! and is skipped, rather than failing the frame. Both enums are therefore
//! ported with their tags and catch-alls intact.
//!
//! The variant set is the subset the architecture document's narrow interface
//! names, plus every variant the adapter must recognise on the event stream
//! during a turn. Variants that only a rendering client needs (`TextDelta`,
//! `SidePanelState`, `KvCacheMiss`, ...) are omitted and arrive as `Unknown`.

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// Protocol major version. Breaking changes only.
///
/// Copied from `jcode-harness-api/src/lib.rs` line 54.
pub const API_VERSION_MAJOR: u32 = 1;

// ---------------------------------------------------------------------------
// Frame I/O — ported verbatim from jcode-harness-api/src/client.rs
// ---------------------------------------------------------------------------

/// Errors from reading or writing a frame.
#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    Json(serde_json::Error),
    /// The stream closed cleanly.
    Eof,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "harness API I/O error: {e}"),
            Self::Json(e) => write!(f, "harness API JSON error: {e}"),
            Self::Eof => write!(f, "harness API stream closed"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for FrameError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Write one frame as a single NDJSON line and flush.
pub fn write_frame<W: Write, T: serde::Serialize>(
    writer: &mut W,
    frame: &T,
) -> Result<(), FrameError> {
    let mut line = serde_json::to_string(frame)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Read one frame from a buffered reader. Skips blank lines.
pub fn read_frame<R: BufRead, T: serde::de::DeserializeOwned>(
    reader: &mut R,
) -> Result<T, FrameError> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Err(FrameError::Eof);
        }
        if line.trim().is_empty() {
            continue;
        }
        return Ok(serde_json::from_str(line.trim())?);
    }
}

// ---------------------------------------------------------------------------
// Envelopes — ported from jcode-harness-api/src/lib.rs
// ---------------------------------------------------------------------------

/// Envelope wrapping every client-to-server frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientFrame {
    /// Protocol major version.
    pub v: u32,
    /// Client-chosen id echoed in acks/replies. Monotonic per connection.
    pub id: u64,
    #[serde(flatten)]
    pub request: ApiRequest,
}

/// Envelope wrapping every server-to-client frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerFrame {
    /// Protocol major version.
    pub v: u32,
    /// The request id this frame replies to, if any. Streaming events that
    /// are not direct replies omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<u64>,
    #[serde(flatten)]
    pub event: ApiEvent,
}

impl ClientFrame {
    pub fn new(id: u64, request: ApiRequest) -> Self {
        Self {
            v: API_VERSION_MAJOR,
            id,
            request,
        }
    }
}

impl ServerFrame {
    pub fn event(event: ApiEvent) -> Self {
        Self {
            v: API_VERSION_MAJOR,
            reply_to: None,
            event,
        }
    }

    pub fn reply(reply_to: u64, event: ApiEvent) -> Self {
        Self {
            v: API_VERSION_MAJOR,
            reply_to: Some(reply_to),
            event,
        }
    }
}

// ---------------------------------------------------------------------------
// Requests — ported from jcode-harness-api/src/requests.rs
// ---------------------------------------------------------------------------

/// Replacement tool configuration for a live session, not a patch.
///
/// Ported from `jcode-harness-api/src/requests.rs` lines 14-27. The
/// "omitted/null inherits defaults, empty disables all built-in/MCP tools"
/// distinction is upstream's and is load-bearing: an empty `enabled` list is a
/// lockdown, not a request for the default inventory.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ToolConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<serde_json::Map<String, serde_json::Value>>,
}

/// Curated request surface. Internally-tagged on `"req"`.
///
/// Ported from `jcode-harness-api/src/requests.rs` lines 29-343. The variants
/// this adapter sends, and the variants it must be able to name in an error
/// message, are kept; a rendering client's requests (file reads, applet
/// actions, rewind, model switching) are not, and a bridge that needed them
/// would answer with `Error {code: UnknownRequest}`, which this adapter
/// surfaces rather than retries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "req", rename_all = "snake_case")]
pub enum ApiRequest {
    /// Version negotiation. Must be the first frame on a connection.
    Hello {
        min_version: u32,
        max_version: u32,
        client: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        supports_user_response: bool,
    },

    /// List sessions visible to this client.
    ListSessions {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        include_archived: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },

    /// Create a new session (optionally in a working directory) and attach.
    CreateSession {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
    },

    /// Attach to an existing session and subscribe to its event stream.
    AttachSession { session_id: String },

    /// Send a user message to the attached session.
    SendMessage {
        session_id: String,
        content: String,
        /// Hidden recovery/context instruction, not a user transcript message.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_reminder: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        no_reply: bool,
    },

    /// Replace the attached session's tool configuration.
    ConfigureTools {
        session_id: String,
        tools: ToolConfiguration,
    },

    /// List the effective tools available to the attached session.
    ListTools { session_id: String },

    /// Cancel the in-flight generation.
    Cancel { session_id: String },

    /// Fetch conversation history.
    GetHistory { session_id: String },

    /// Fetch the tail of *any* session's conversation, attached or not.
    PeekSession {
        session_id: String,
        #[serde(default)]
        limit: Option<u32>,
    },

    /// Provider routes and active runtime identity for the attached session.
    GetRuntimeInfo { session_id: String },

    /// Execute a runtime-backed command without a model turn.
    RuntimeCommand {
        session_id: String,
        #[serde(default)]
        command: RuntimeCommand,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },

    /// Liveness check.
    Ping,

    /// Forward-compatibility catch-all. Servers reply with an error frame.
    #[serde(other)]
    Unknown,
}

/// The runtime commands this adapter can ask for.
///
/// Ported from `jcode-harness-api/src/runtime_commands.rs`. `info` is the one
/// the supervisor uses: it replies with structured data including the session's
/// context window, which is the only way to observe a turn's remaining room
/// without a model call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCommand {
    #[default]
    Info,
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Events — ported from jcode-harness-api/src/events.rs
// ---------------------------------------------------------------------------

/// Curated event surface. Internally-tagged on `"ev"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum ApiEvent {
    /// Handshake accepted. Sent in reply to `Hello`.
    HelloOk {
        version: u32,
        server: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        capabilities: Vec<String>,
    },

    /// Generic success acknowledgment for requests without a richer reply.
    Ok,

    /// Reply to a runtime command.
    RuntimeCommandResult {
        session_id: String,
        command: RuntimeCommand,
        data: serde_json::Value,
        message: String,
    },

    /// Request failed.
    Error { code: ErrorCode, message: String },

    /// Reply to `ListSessions`.
    Sessions { sessions: Vec<SessionInfo> },

    /// Reply to `CreateSession` / `AttachSession`.
    Attached { session: SessionInfo },

    /// Reply to `GetHistory`.
    History {
        session_id: String,
        messages: Vec<HistoryMessage>,
    },

    /// Reply to `Ping`.
    Pong,

    /// Effective tool inventory, in reply to `ListTools`.
    Tools {
        session_id: String,
        tools: Vec<serde_json::Map<String, serde_json::Value>>,
    },

    /// A tool name is known, even if no argument bytes have arrived yet.
    ToolStart {
        session_id: String,
        call_id: String,
        name: String,
    },

    /// Incremental, potentially incomplete JSON.
    ToolInputDelta {
        session_id: String,
        call_id: String,
        delta: String,
    },

    ToolExec {
        session_id: String,
        call_id: String,
        name: String,
    },

    ToolDone {
        session_id: String,
        call_id: String,
        name: String,
        output: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Usage for the latest provider call, not cumulative session totals.
    TokenUsage {
        session_id: String,
        input: u64,
        output: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_read_input: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_creation_input: Option<u64>,
    },

    /// An abnormal stop, never emitted for natural completion. This precedes
    /// `TurnDone` (and Error for failures).
    TurnStopped {
        session_id: String,
        reason: TurnStopReason,
        message: String,
    },

    /// The turn finished; the agent is idle.
    TurnDone {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pending_soft_interrupts: Option<u32>,
    },

    /// The agent accepted a user message: it is in the session's queue.
    MessageAccepted { session_id: String },

    /// Reasoning finished for the current step.
    ReasoningDone {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_secs: Option<f64>,
    },

    /// Assistant message text ended.
    TextDone {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },

    /// Provider/runtime identity and every route the daemon exposes.
    RuntimeInfo {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        routes: Vec<serde_json::Value>,
    },

    /// Session-level status change (idle, generating, tool_running, ...).
    SessionStatus {
        session_id: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pending_soft_interrupts: Option<u32>,
    },

    /// The provider and model serving the attached session.
    ModelInfo {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },

    /// Forward-compatibility catch-all: clients must skip this silently.
    #[serde(other)]
    Unknown,
}

impl ApiEvent {
    /// The session id this event concerns, when it carries one.
    ///
    /// Upstream's rule: streaming events carry `session_id`, request replies
    /// may not. A supervisor correlating events into the shared store needs
    /// this without matching on every variant.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::RuntimeCommandResult { session_id, .. } => Some(session_id),
            Self::Attached { session } => Some(session.session_id.as_str()),
            Self::History { session_id, .. }
            | Self::Tools { session_id, .. }
            | Self::ToolStart { session_id, .. }
            | Self::ToolInputDelta { session_id, .. }
            | Self::ToolExec { session_id, .. }
            | Self::ToolDone { session_id, .. }
            | Self::TokenUsage { session_id, .. }
            | Self::TurnStopped { session_id, .. }
            | Self::TurnDone { session_id, .. }
            | Self::MessageAccepted { session_id }
            | Self::ReasoningDone { session_id, .. }
            | Self::TextDone { session_id, .. }
            | Self::RuntimeInfo { session_id, .. }
            | Self::SessionStatus { session_id, .. }
            | Self::ModelInfo { session_id, .. } => Some(session_id),
            Self::HelloOk { .. }
            | Self::Ok
            | Self::Error { .. }
            | Self::Pong
            | Self::Unknown => None,
            Self::Sessions { .. } => None,
        }
    }

    /// The upstream protocol name of this event.
    ///
    /// This is the value that appears on the wire, so a store record built
    /// from it matches what a bridge log says. A variant that is not a real
    /// upstream frame — only `Unknown` is not — still reports the name
    /// upstream uses for the malformed case.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::HelloOk { .. } => "hello_ok",
            Self::Ok => "ok",
            Self::Error { .. } => "error",
            Self::Pong => "pong",
            Self::RuntimeCommandResult { .. } => "runtime_command_result",
            Self::Sessions { .. } => "sessions",
            Self::Attached { .. } => "attach_session",
            Self::History { .. } => "history",
            Self::MessageAccepted { .. } => "message_accepted",
            Self::Tools { .. } => "tools",
            Self::ToolStart { .. } => "tool_start",
            Self::ToolInputDelta { .. } => "tool_input_delta",
            Self::ToolExec { .. } => "tool_exec",
            Self::ToolDone { .. } => "tool_done",
            Self::TokenUsage { .. } => "token_usage",
            Self::TurnStopped { .. } => "turn_stopped",
            Self::TurnDone { .. } => "turn_done",
            Self::ReasoningDone { .. } => "reasoning_done",
            Self::TextDone { .. } => "text_done",
            Self::RuntimeInfo { .. } => "runtime_info",
            Self::SessionStatus { .. } => "session_status",
            Self::ModelInfo { .. } => "model_info",
            Self::Unknown => "unknown",
        }
    }
}

/// Copied from `jcode-harness-api/src/events.rs` lines 445-456.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnsupportedVersion,
    UnknownRequest,
    UnknownSession,
    InvalidRequest,
    Internal,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The wire name, which is what a bridge log and a caller both see.
        // `Debug` would print `UnknownSession`, which no protocol document
        // uses, so an error message built from it could not be grepped against
        // a bridge log.
        let name = match self {
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnknownRequest => "unknown_request",
            Self::UnknownSession => "unknown_session",
            Self::InvalidRequest => "invalid_request",
            Self::Internal => "internal",
        };
        f.write_str(name)
    }
}

/// Copied from `jcode-session-types/src/lib.rs` lines 1113-1124.
///
/// `Crash` is "a caught runtime panic, not an inference from a lost
/// connection" upstream. The reconciler therefore never invents a `Crash`
/// verdict from a dead pid: a dead pid with an open lease is `Orphaned`, and
/// only the daemon's own `TurnStopped {reason: Crash}` is a crash.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStopReason {
    Interrupted,
    Failure,
    Crash,
    ProviderGuardrail,
    LimitReached,
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for TurnStopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The wire name, not the Rust variant name, so a store record built
        // from this matches what a bridge log says.
        let name = match self {
            Self::Interrupted => "interrupted",
            Self::Failure => "failure",
            Self::Crash => "crash",
            Self::ProviderGuardrail => "provider_guardrail",
            Self::LimitReached => "limit_reached",
            Self::Unknown => "unknown",
        };
        f.write_str(name)
    }
}

/// The session fields the adapter reads.
///
/// Ported from `jcode-harness-api/src/events.rs` lines 462-518. The remaining
/// upstream fields (`user_connected`, `edit_stats`, `parent_session_id`,
/// `agent_label`, `swarm_status`, `title`, `transcript_bytes`, `saved`,
/// `save_label`, `updated_at_ms`, `last_active_at_ms`, `archived`,
/// `archived_at_ms`) are a client-switcher's concern and are not read here;
/// `#[serde(flatten)]`-free plain deserialization ignores them, which is
/// upstream's own forward-compatibility rule for clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionInfo {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    pub status: String,
}

/// Copied from `jcode-harness-api/src/events.rs` lines 565-573.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryMessage {
    /// "user" | "assistant" | "tool".
    pub role: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_round_trips_through_an_in_memory_pipe() {
        let frame = ClientFrame::new(7, ApiRequest::Ping);
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();
        assert_eq!(buf.last(), Some(&b'\n'), "one frame is one NDJSON line");
        let mut reader = std::io::BufReader::new(buf.as_slice());
        let read: ClientFrame = read_frame(&mut reader).unwrap();
        assert_eq!(read, frame);
    }

    #[test]
    fn a_blank_line_is_skipped_rather_than_failing_the_frame() {
        let text = "\n\n{\"v\":1,\"id\":1,\"req\":\"ping\"}\n\n";
        let mut reader = std::io::BufReader::new(text.as_bytes());
        let frame: ClientFrame = read_frame(&mut reader).unwrap();
        assert_eq!(frame.request, ApiRequest::Ping);
        assert_eq!(frame.v, API_VERSION_MAJOR);
    }

    #[test]
    fn eof_is_a_typed_error_not_a_json_error() {
        let mut reader = std::io::BufReader::new("".as_bytes());
        let err = read_frame::<_, ServerFrame>(&mut reader).unwrap_err();
        assert!(matches!(err, FrameError::Eof), "got {err:?}");
    }

    #[test]
    fn an_unknown_event_kind_deserialises_to_unknown_rather_than_failing() {
        // A bridge running a newer jcode emits a kind this build does not know.
        let line = "{\"v\":1,\"ev\":\"some_future_event\",\"session_id\":\"s1\"}\n";
        let mut reader = std::io::BufReader::new(line.as_bytes());
        let frame: ServerFrame = read_frame(&mut reader).unwrap();
        assert_eq!(frame.event, ApiEvent::Unknown);
    }

    #[test]
    fn an_unknown_request_kind_deserialises_to_unknown() {
        let line = "{\"v\":1,\"id\":4,\"req\":\"some_future_request\"}\n";
        let mut reader = std::io::BufReader::new(line.as_bytes());
        let frame: ClientFrame = read_frame(&mut reader).unwrap();
        assert_eq!(frame.request, ApiRequest::Unknown);
    }

    #[test]
    fn requests_serialise_with_the_upstream_snake_case_tag() {
        let frame = ClientFrame::new(
            1,
            ApiRequest::ConfigureTools {
                session_id: "s1".into(),
                tools: ToolConfiguration {
                    enabled: Some(vec!["bash".into()]),
                    ..Default::default()
                },
            },
        );
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"req\":\"configure_tools\""), "got {json}");
    }

    #[test]
    fn events_serialise_with_the_upstream_snake_case_tag() {
        let frame = ServerFrame::reply(9, ApiEvent::Pong);
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"ev\":\"pong\""), "got {json}");
        assert!(json.contains("\"reply_to\":9"), "got {json}");
    }

    #[test]
    fn a_streaming_event_omits_reply_to() {
        let json = serde_json::to_string(&ServerFrame::event(ApiEvent::MessageAccepted {
            session_id: "s1".into(),
        }))
        .unwrap();
        assert!(!json.contains("reply_to"), "got {json}");
    }

    #[test]
    fn session_id_is_recoverable_from_streaming_events_only() {
        assert_eq!(
            ApiEvent::MessageAccepted {
                session_id: "s1".into()
            }
            .session_id(),
            Some("s1")
        );
        assert_eq!(ApiEvent::Pong.session_id(), None);
    }

    #[test]
    fn an_empty_tool_list_is_a_lockdown_not_an_inherit() {
        // Upstream: `enabled: None` inherits defaults, `enabled: Some(vec![])`
        // disables every built-in and MCP tool. The two must not collapse.
        let inherit = ToolConfiguration::default();
        let lockdown = ToolConfiguration {
            enabled: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(inherit.enabled, None);
        assert_eq!(lockdown.enabled, Some(Vec::new()));
        assert_ne!(inherit, lockdown);
    }

    #[test]
    fn turn_stop_reasons_use_upstream_vocabulary() {
        for (json, want) in [
            ("\"interrupted\"", TurnStopReason::Interrupted),
            ("\"failure\"", TurnStopReason::Failure),
            ("\"crash\"", TurnStopReason::Crash),
            ("\"limit_reached\"", TurnStopReason::LimitReached),
        ] {
            let got: TurnStopReason = serde_json::from_str(json).unwrap();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn session_info_ignores_fields_this_version_does_not_read() {
        // Upstream's forward-compatibility rule for clients: unknown fields on
        // a known struct are ignored, not an error.
        let line = "{\"session_id\":\"s1\",\"status\":\"idle\",\"working_dir\":\"/tmp\",\"saved\":true,\"transcript_bytes\":99,\"title\":\"later field\"}\n";
        let info: SessionInfo = serde_json::from_str(line).unwrap();
        assert_eq!(info.session_id, "s1");
        assert_eq!(info.status, "idle");
    }
}
