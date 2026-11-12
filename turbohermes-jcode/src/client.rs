//! `JcodeClient`: the narrow typed surface over the transport.
//!
//! **Glue over [`crate::transport`]**, which is itself a port of
//! `jcode/crates/jcode-harness-api/src/client.rs`. The client's shape follows
//! upstream's `HarnessClient`; what it adds is the *narrowness*: the
//! architecture document's list of operations, and nothing else.
//!
//! The measured behaviour that shapes this module (verified live 2026-11-10
//! against `jcode api-bridge` on this host):
//!
//! - `send_message` does **not** reply with a frame. It emits
//!   `message_accepted`, `connection_phase`, `reasoning_delta`,
//!   `reasoning_done`, `tool_start`, `tool_input_delta`, `tool_exec`,
//!   `tool_done`, `text_delta`, `text_done`, `token_usage`, `runtime_info`,
//!   then `turn_done`. A client that waits for `reply_to` on `send` blocks
//!   forever. So [`JcodeClient::send`] returns `Ok(())` immediately and the
//!   reply is the event stream, which the caller pumps with
//!   [`JcodeClient::next_event`].
//! - `cancel` replies `ok` and then emits
//!   `turn_stopped {reason: "interrupted"}`.
//! - Two `send_message` calls in one turn: the second returns an `error`
//!   frame, because a turn is already active. That is the duplicate-dispatch
//!   boundary the integration test pins.

use crate::frames::{ApiEvent, ApiRequest, ServerFrame, SessionInfo};
use crate::transport::{SocketPath, Transport, TransportError};
use std::time::{Duration, Instant};

/// Errors from the client.
#[derive(Debug)]
pub enum JcodeClientError {
    Transport(TransportError),
    /// The bridge answered a request with an error frame.
    Refused { code: String, message: String },
    /// A reply arrived, but not the one this request needed.
    UnexpectedReply(String),
    /// The event stream ended before the turn finished.
    StreamClosed,
}

impl std::fmt::Display for JcodeClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "{e}"),
            Self::Refused { code, message } => {
                write!(f, "the harness refused the request ({code}): {message}")
            }
            Self::UnexpectedReply(m) => write!(f, "{m}"),
            Self::StreamClosed => write!(f, "the harness event stream closed"),
        }
    }
}

impl std::error::Error for JcodeClientError {}

impl From<TransportError> for JcodeClientError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

/// The narrow typed surface: the operations the architecture document names.
///
/// "start, resume, inspect and stop sessions; create a swarm from a task-DAG
/// node; pass scoped task specification, repository path, branch or worktree,
/// budget and permitted tools; inject context; collect typed artifacts; map
/// jcode identities to TurboHermes identities; reconcile after process
/// failure."
///
/// A request that is not in that list is not in this client. In particular the
/// client cannot switch models, rewind history, set an API key, read files or
/// drive an applet: those are a rendering client's operations, and a supervisor
/// that could do them would be a second control surface for the same session.
pub struct JcodeClient {
    transport: Transport,
}

impl JcodeClient {
    /// Connect to the bridge at `socket`: hello, then a `Ping`.
    ///
    /// The ping is not redundant with the hello. The hello proves the bridge
    /// speaks this protocol; the ping proves the bridge's translation path to
    /// the daemon works, which is a different question and the one that fails
    /// when the daemon is down but the bridge is up.
    pub fn connect(socket: &SocketPath) -> Result<Self, JcodeClientError> {
        let transport = Transport::connect(socket)?;
        let mut client = Self { transport };
        client.ping()?;
        Ok(client)
    }

    /// Build a client over an existing transport, for tests.
    pub fn from_transport(transport: Transport) -> Self {
        Self { transport }
    }

    /// The transport underneath, for the reconciler's session probe.
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Liveness check.
    pub fn ping(&mut self) -> Result<(), JcodeClientError> {
        let id = self.transport.send(ApiRequest::Ping)?;
        let (reply, _) = self.transport.recv_reply(id)?;
        match reply.event {
            ApiEvent::Pong => Ok(()),
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected pong, got {other:?}"
            ))),
        }
    }

    /// Create a new session in `working_dir` and attach to it.
    ///
    /// The architecture document's "start a session". The bridge moves the
    /// socket's cwd into `~/.jcode/scratch/<name>`, so the working directory is
    /// what scopes every file tool the session runs, which is why a controlled
    /// worktree is passed here rather than the repository root.
    pub fn create(&mut self, working_dir: &str) -> Result<String, JcodeClientError> {
        let id = self.transport.send(ApiRequest::CreateSession {
            working_dir: Some(working_dir.to_string()),
            system_prompt: None,
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        self.session_from(reply)
    }

    /// Attach to an existing session and subscribe to its event stream.
    ///
    /// The architecture document's "resume a session". A session that was never
    /// persisted answers `error unknown_session`, which is surfaced rather than
    /// retried: there is nothing to resume.
    pub fn attach(&mut self, session_id: &str) -> Result<String, JcodeClientError> {
        let id = self.transport.send(ApiRequest::AttachSession {
            session_id: session_id.to_string(),
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        self.session_from(reply)
    }

    /// List sessions, newest first, at most `limit`.
    ///
    /// The reconciler's probe: whether a claimed session still exists, and what
    /// its status is.
    pub fn list(&mut self, limit: Option<u32>) -> Result<Vec<SessionInfo>, JcodeClientError> {
        let id = self.transport.send(ApiRequest::ListSessions {
            include_archived: false,
            limit,
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        match reply.event {
            ApiEvent::Sessions { sessions } => Ok(sessions),
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected sessions, got {other:?}"
            ))),
        }
    }

    /// Fetch the tail of any session's conversation, attached or not.
    pub fn peek(
        &mut self,
        session_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<crate::frames::HistoryMessage>, JcodeClientError> {
        let id = self.transport.send(ApiRequest::PeekSession {
            session_id: session_id.to_string(),
            limit,
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        match reply.event {
            ApiEvent::History { messages, .. } => Ok(messages),
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected history, got {other:?}"
            ))),
        }
    }

    /// Replace the session's tool configuration.
    ///
    /// The architecture document's "permitted tools". The configuration is a
    /// replacement, not a patch, so the allow-list is exactly what the task
    /// spec named and nothing the daemon defaults to.
    pub fn configure_tools(
        &mut self,
        session_id: &str,
        tools: crate::frames::ToolConfiguration,
    ) -> Result<(), JcodeClientError> {
        let id = self.transport.send(ApiRequest::ConfigureTools {
            session_id: session_id.to_string(),
            tools,
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        self.expect_ok(reply)
    }

    /// Send a user message to the session.
    ///
    /// **This does not wait for a reply.** Measured on this host: the bridge
    /// emits `message_accepted` and then the streaming events of the turn, and
    /// never a frame with `reply_to` matching the send. A caller that waits
    /// blocks forever. The reply is the event stream, pumped with
    /// [`JcodeClient::next_event`].
    ///
    /// The `system_reminder` is the architecture document's "inject context":
    /// a hidden instruction, not a user transcript message.
    pub fn send(
        &mut self,
        session_id: &str,
        content: &str,
        system_reminder: Option<&str>,
    ) -> Result<(), JcodeClientError> {
        self.transport.send(ApiRequest::SendMessage {
            session_id: session_id.to_string(),
            content: content.to_string(),
            system_reminder: system_reminder.map(str::to_string),
            images: Vec::new(),
            no_reply: false,
        })?;
        Ok(())
    }

    /// Cancel the in-flight generation.
    ///
    /// Replies `ok`, then emits `turn_stopped {reason: "interrupted"}`. The
    /// caller sees the stop through the event pump, which is what makes a
    /// cancellation observable rather than merely requested.
    pub fn cancel(&mut self, session_id: &str) -> Result<(), JcodeClientError> {
        let id = self.transport.send(ApiRequest::Cancel {
            session_id: session_id.to_string(),
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        self.expect_ok(reply)
    }

    /// Fetch the attached session's conversation history.
    pub fn history(
        &mut self,
        session_id: &str,
    ) -> Result<Vec<crate::frames::HistoryMessage>, JcodeClientError> {
        let id = self.transport.send(ApiRequest::GetHistory {
            session_id: session_id.to_string(),
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        match reply.event {
            ApiEvent::History { messages, .. } => Ok(messages),
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected history, got {other:?}"
            ))),
        }
    }

    /// Provider routes and active runtime identity for the session.
    ///
    /// The reply carries the session's context window, which is the only way
    /// to observe a turn's remaining room without a model call.
    pub fn runtime_info(&mut self, session_id: &str) -> Result<serde_json::Value, JcodeClientError> {
        let id = self.transport.send(ApiRequest::GetRuntimeInfo {
            session_id: session_id.to_string(),
        })?;
        let (reply, _) = self.transport.recv_reply(id)?;
        match reply.event {
            ApiEvent::RuntimeInfo { .. } => {
                Ok(serde_json::to_value(&reply.event).unwrap_or(serde_json::Value::Null))
            }
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected runtime info, got {other:?}"
            ))),
        }
    }

    /// The next event on the stream, waiting at most `timeout`.
    ///
    /// `None` on timeout or a closed stream. The caller owns the loop: the
    /// client never blocks a turn on a reply that will not come, and it never
    /// decides on the caller's behalf that a turn is over. `turn_done`,
    /// `turn_stopped` and `error` are all returned as events for the caller to
    /// interpret.
    pub fn next_event(&mut self, timeout: Duration) -> Option<ApiEvent> {
        let deadline = Instant::now() + timeout;
        // The transport's read timeout is the socket-level bound; the caller's
        // timeout is the turn-level one. Whichever fires first ends the wait.
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match self.transport.recv() {
                Ok(frame) => return Some(frame.event),
                Err(crate::transport::TransportError::Frame(crate::frames::FrameError::Eof)) => {
                    return None;
                }
                Err(_) => return None,
            }
        }
    }

    /// Drain events until the turn ends, returning everything observed.
    ///
    /// The turn ends on `turn_done` (the agent is idle), `turn_stopped` (an
    /// abnormal stop) or `error` (a failure). Both terminal events are
    /// included in the returned vector so the supervisor maps them, and a
    /// timeout returns whatever arrived with the reason it stopped.
    pub fn collect_turn(&mut self, timeout: Duration) -> TurnOutcome {
        let mut events = Vec::new();
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return TurnOutcome::TimedOut(events);
            }
            match self.transport.recv() {
                Ok(frame) => {
                    let terminal = matches!(
                        frame.event,
                        ApiEvent::TurnDone { .. }
                            | ApiEvent::TurnStopped { .. }
                            | ApiEvent::Error { .. }
                    );
                    events.push(frame.event);
                    if terminal {
                        return TurnOutcome::Finished(events);
                    }
                }
                Err(_) => return TurnOutcome::TimedOut(events),
            }
        }
    }

    fn expect_ok(&self, reply: ServerFrame) -> Result<(), JcodeClientError> {
        match reply.event {
            ApiEvent::Ok => Ok(()),
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected ok, got {other:?}"
            ))),
        }
    }

    fn session_from(&self, reply: ServerFrame) -> Result<String, JcodeClientError> {
        match reply.event {
            ApiEvent::Attached { session } => Ok(session.session_id),
            ApiEvent::Error { code, message } => Err(JcodeClientError::Refused {
                code: code.to_string(),
                message,
            }),
            other => Err(JcodeClientError::UnexpectedReply(format!(
                "expected attached, got {other:?}"
            ))),
        }
    }
}

/// How a turn ended.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnOutcome {
    /// A terminal event arrived. The events include it.
    Finished(Vec<ApiEvent>),
    /// The timeout fired first. The events are what arrived before it.
    TimedOut(Vec<ApiEvent>),
}

impl TurnOutcome {
    /// The events observed either way.
    pub fn events(&self) -> &[ApiEvent] {
        match self {
            Self::Finished(e) | Self::TimedOut(e) => e,
        }
    }

    /// True when a terminal event ended the turn.
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{
        API_VERSION_MAJOR, ApiEvent as E, ClientFrame, HistoryMessage, ServerFrame, read_frame,
        write_frame,
    };
    use std::io::BufReader;
    use std::os::unix::net::UnixListener;

    /// A stub bridge that speaks the upstream wire protocol.
    ///
    /// It answers each request with the reply the real bridge would give, so
    /// the client's parsing and its refusal handling are exercised without a
    /// jcode binary. It records the requests it saw, so the tests can assert
    /// the client sent the right ones.
    struct StubBridge {
        path: std::path::PathBuf,
        seen: std::sync::Arc<std::sync::Mutex<Vec<ApiRequest>>>,
    }

    impl StubBridge {
        fn spawn(reply: impl Fn(ApiRequest) -> ApiEvent + Send + 'static) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("stub.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let seen2 = seen.clone();
            std::thread::spawn(move || {
                let _keep = dir;
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let frame: ClientFrame = match read_frame(&mut reader) {
                        Ok(f) => f,
                        Err(_) => break,
                    };
                    seen2.lock().unwrap().push(frame.request.clone());
                    let event = reply(frame.request);
                    if write_frame(&mut stream.try_clone().unwrap(), &ServerFrame::reply(frame.id, event))
                        .is_err()
                    {
                        break;
                    }
                }
            });
            Self { path, seen }
        }

        /// Connect through the real entry point, not through `from_transport`.
        ///
        /// `Transport::connect` performs the hello handshake and
        /// `JcodeClient::connect` then pings, so the stub's request log is the
        /// same sequence a live bridge would see. Bypassing the ping here would
        /// make every request index in these tests one off, which is exactly
        /// the kind of drift a stub is supposed to prevent.
        fn connect(&self) -> JcodeClient {
            JcodeClient::connect(&SocketPath::at(&self.path)).unwrap()
        }

        fn requests(&self) -> Vec<ApiRequest> {
            self.seen.lock().unwrap().clone()
        }
    }

    fn hello_then(event: ApiEvent) -> impl Fn(ApiRequest) -> ApiEvent + Send + 'static {
        move |req| match req {
            ApiRequest::Hello { .. } => ApiEvent::HelloOk {
                version: API_VERSION_MAJOR,
                server: "stub".into(),
                capabilities: vec![],
            },
            ApiRequest::Ping => ApiEvent::Pong,
            _ => event.clone(),
        }
    }

    #[test]
    fn connect_performs_hello_then_ping() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::Pong));
        let _client = stub.connect();
        let requests = stub.requests();
        assert!(matches!(requests[0], ApiRequest::Hello { .. }));
        assert_eq!(requests[1], ApiRequest::Ping);
    }

    #[test]
    fn create_and_attach_return_the_session_id() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::Attached {
            session: SessionInfo {
                session_id: "s1".into(),
                working_dir: Some("/repo".into()),
                status: "idle".into(),
            },
        }));
        let mut client = stub.connect();
        assert_eq!(client.create("/repo").unwrap(), "s1");
        assert_eq!(client.attach("s1").unwrap(), "s1");
        let requests = stub.requests();
        assert!(matches!(
            &requests[2],
            ApiRequest::CreateSession { working_dir: Some(w), .. } if w == "/repo"
        ));
        assert!(matches!(&requests[3], ApiRequest::AttachSession { session_id } if session_id == "s1"));
    }

    #[test]
    fn attach_to_an_unknown_session_is_refused_not_retried() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::Error {
            code: crate::frames::ErrorCode::UnknownSession,
            message: "Failed to restore session".into(),
        }));
        let mut client = stub.connect();
        let err = client.attach("nope").unwrap_err();
        assert!(matches!(err, JcodeClientError::Refused { .. }), "got {err}");
        let msg = err.to_string();
        assert!(msg.contains("unknown_session"), "got {msg}");
    }

    #[test]
    fn configure_tools_sends_the_allow_list_as_a_replacement() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::Ok));
        let mut client = stub.connect();
        client
            .configure_tools(
                "s1",
                crate::frames::ToolConfiguration {
                    enabled: Some(vec!["bash".into(), "str_replace".into()]),
                    ..Default::default()
                },
            )
            .unwrap();
        let requests = stub.requests();
        match &requests[2] {
            ApiRequest::ConfigureTools { session_id, tools } => {
                assert_eq!(session_id, "s1");
                assert_eq!(
                    tools.enabled.as_deref(),
                    Some(&["bash".to_string(), "str_replace".to_string()][..])
                );
            }
            other => panic!("expected configure_tools, got {other:?}"),
        }
    }

    #[test]
    fn list_returns_the_sessions_the_bridge_has() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::Sessions {
            sessions: vec![
                SessionInfo {
                    session_id: "s1".into(),
                    working_dir: None,
                    status: "idle".into(),
                },
                SessionInfo {
                    session_id: "s2".into(),
                    working_dir: None,
                    status: "generating".into(),
                },
            ],
        }));
        let mut client = stub.connect();
        let sessions = client.list(Some(10)).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[1].status, "generating");
    }

    #[test]
    fn history_and_peek_return_messages() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::History {
            session_id: "s1".into(),
            messages: vec![HistoryMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
        }));
        let mut client = stub.connect();
        let history = client.history("s1").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        let peeked = client.peek("s1", Some(5)).unwrap();
        assert_eq!(peeked.len(), 1);
    }

    #[test]
    fn cancel_asks_the_bridge_to_stop_and_expects_ok() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::Ok));
        let mut client = stub.connect();
        client.cancel("s1").unwrap();
        assert!(matches!(&stub.requests()[2], ApiRequest::Cancel { session_id } if session_id == "s1"));
    }

    #[test]
    fn runtime_info_returns_the_bridges_structured_data() {
        let stub = StubBridge::spawn(hello_then(ApiEvent::RuntimeInfo {
            session_id: "s1".into(),
            provider: Some("anthropic".into()),
            model: Some("claude-opus-5".into()),
            routes: vec![],
        }));
        let mut client = stub.connect();
        let info = client.runtime_info("s1").unwrap();
        assert_eq!(info["model"], "claude-opus-5");
    }

    #[test]
    fn send_does_not_wait_for_a_reply_frame() {
        // The measured behaviour: send_message emits streaming events and never
        // a frame with reply_to. A client that waited for one would hang, so
        // this test asserts send returns immediately and the reply arrives as
        // an event.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("send.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let _keep = dir;
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let hello: ClientFrame = read_frame(&mut reader).unwrap();
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::reply(hello.id, E::HelloOk {
                    version: API_VERSION_MAJOR,
                    server: "stub".into(),
                    capabilities: vec![],
                }),
            )
            .unwrap();
            let ping: ClientFrame = read_frame(&mut reader).unwrap();
            write_frame(&mut stream.try_clone().unwrap(), &ServerFrame::reply(ping.id, E::Pong))
                .unwrap();
            let send: ClientFrame = read_frame(&mut reader).unwrap();
            assert!(matches!(&send.request, ApiRequest::SendMessage { .. }));
            // No reply frame: the reply is the event stream.
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::event(E::MessageAccepted {
                    session_id: "s1".into(),
                }),
            )
            .unwrap();
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::event(E::TurnDone {
                    session_id: "s1".into(),
                    pending_soft_interrupts: None,
                }),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });

        // Through the real connect, so the stub's hello and ping reads match
        // what the client actually sends. Driving the transport by hand here
        // left the stub blocked on a ping that never came, so it never reached
        // the send frame and the events never arrived.
        let mut client = JcodeClient::connect(&SocketPath::at(&path)).unwrap();
        // Returns without waiting for any frame.
        client
            .send("s1", "do the thing", Some("hidden context"))
            .unwrap();
        // The reply is the event stream.
        let outcome = client.collect_turn(Duration::from_secs(2));
        assert!(outcome.is_finished());
        let events = outcome.events();
        assert!(matches!(events[0], E::MessageAccepted { .. }));
        assert!(matches!(events[1], E::TurnDone { .. }));
    }

    #[test]
    fn a_second_send_in_one_turn_is_refused_by_the_bridge() {
        // The duplicate-dispatch boundary: the bridge answers the second
        // send_message with an error because a turn is already active.
        let stub = StubBridge::spawn(|req| match req {
            ApiRequest::Hello { .. } => E::HelloOk {
                version: API_VERSION_MAJOR,
                server: "stub".into(),
                capabilities: vec![],
            },
            ApiRequest::Ping => E::Pong,
            ApiRequest::SendMessage { .. } => E::Error {
                code: crate::frames::ErrorCode::InvalidRequest,
                message: "a turn is already active".into(),
            },
            _ => E::Ok,
        });
        let mut client = stub.connect();
        // The send itself succeeds: `send_message` never has a reply frame to
        // be wrong about. The bridge's refusal arrives on the event stream, so
        // the pump is what surfaces it.
        client.send("s1", "second message", None).unwrap();
        let outcome = client.collect_turn(Duration::from_millis(200));
        let saw_error = outcome
            .events()
            .iter()
            .any(|e| matches!(e, E::Error { .. }));
        assert!(saw_error, "the pump did not surface the refusal: {:?}", outcome.events());
    }
}
