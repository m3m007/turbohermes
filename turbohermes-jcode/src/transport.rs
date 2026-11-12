//! Unix-socket transport, version handshake, and bridge lifecycle.
//!
//! **Ported from jcode** (upstream commit
//! `7206efd0c1dbf49ef38d06b63f3a96a3a7cee611`, MIT):
//! - Socket path resolution — `crates/jcode-harness-api/src/sockets.rs` lines
//!   12-56, copied verbatim, including the `JCODE_API_SOCKET` override.
//! - The handshake and the monotonic request id — the shape of
//!   `HarnessClient::{hello,send,recv}` in
//!   `crates/jcode-harness-api/src/client.rs` lines 69-116.
//!
//! **Glue** (checked in each source, nothing to port): the bridge's process
//! lifecycle. jcode ships the bridge as a binary
//! (`crates/jcode-harness-api-server/src/bin/bridge.rs`) and starts it from its
//! own desktop/TUI entrypoints; there is no library function to spawn and reap
//! it that this crate could call without depending on jcode as a library. So
//! [`Bridge`] owns the child process, waits for the socket, and reaps it on
//! drop.

use crate::frames::{
    API_VERSION_MAJOR, ApiEvent, ApiRequest, ClientFrame, ErrorCode, FrameError, ServerFrame,
    read_frame, write_frame,
};
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// How long [`Transport::connect`] waits for the socket to accept.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long [`Bridge::start`] waits for the socket file to appear.
const BRIDGE_START_TIMEOUT: Duration = Duration::from_secs(20);

/// Errors from the transport.
#[derive(Debug)]
pub enum TransportError {
    /// The socket could not be connected to within the timeout.
    Connect(String),
    /// A frame could not be written, read or parsed.
    Frame(FrameError),
    /// The bridge refused the handshake: it speaks a protocol this crate does
    /// not, or it rejected the client name.
    Handshake(String),
    /// The bridge answered a request with an error frame.
    Refused { code: ErrorCode, message: String },
    /// A reply arrived for a different request id, or the stream ended first.
    LostReply { expected: u64, got: Option<u64> },
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(m) => write!(f, "cannot connect to the harness API socket: {m}"),
            Self::Frame(e) => write!(f, "{e}"),
            Self::Handshake(m) => write!(f, "harness API handshake failed: {m}"),
            Self::Refused { code, message } => {
                write!(f, "harness API refused the request ({code:?}): {message}")
            }
            Self::LostReply { expected, got } => write!(
                f,
                "harness API reply lost: expected frame {expected}, got {got:?}"
            ),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<FrameError> for TransportError {
    fn from(e: FrameError) -> Self {
        Self::Frame(e)
    }
}

/// Where the harness API socket lives.
///
/// Ported from `jcode-harness-api/src/sockets.rs`. The resolution order is
/// upstream's and is not a preference: upstream's own doc comment records that
/// a client resolving a different directory than the bridge "could never
/// connect even with a healthy bridge running". One definition, used by both
/// sides, makes that class of bug impossible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketPath(PathBuf);

impl SocketPath {
    /// The path of the versioned harness API socket. `JCODE_API_SOCKET`
    /// overrides it.
    ///
    /// Copied from `sockets.rs` `api_socket_path`.
    pub fn resolve() -> Self {
        if let Ok(custom) = std::env::var("JCODE_API_SOCKET") {
            return Self(PathBuf::from(custom));
        }
        Self(runtime_dir().join("jcode-api.sock"))
    }

    /// A specific path, bypassing environment resolution.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// The path as a `&Path`.
    pub fn as_path(&self) -> &std::path::Path {
        &self.0
    }
}

impl std::fmt::Display for SocketPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// Runtime directory holding the daemon and API sockets.
///
/// Copied from `sockets.rs` `runtime_dir`: `JCODE_RUNTIME_DIR`, then
/// `XDG_RUNTIME_DIR`, then a per-user temp fallback.
fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("JCODE_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    std::env::temp_dir().join(format!("jcode-{}", runtime_user_discriminator()))
}

/// Read the uid without pulling in libc, as upstream does.
///
/// Copied from `sockets.rs` `runtime_user_discriminator`.
fn runtime_user_discriminator() -> String {
    std::env::var("UID")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .map(sanitize)
        .unwrap_or_else(|| "user".to_string())
}

/// Copied from `sockets.rs` `sanitize`.
fn sanitize(raw: String) -> String {
    let out: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .take(64)
        .collect();
    if out.is_empty() {
        "user".to_string()
    } else {
        out
    }
}

/// A connected harness API transport.
///
/// Owns the read and write halves of one Unix socket, the monotonic request id
/// counter, and the negotiated version. One transport serves one bridge
/// connection; a supervisor that wants two sessions runs two transports.
#[derive(Debug)]
pub struct Transport {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
    /// The version the bridge accepted in the handshake.
    negotiated_version: u32,
    /// The capability strings the bridge advertised, e.g. `soft_interrupt_ids`.
    capabilities: Vec<String>,
}

impl Transport {
    /// Connect to the socket at `path` and perform the version handshake.
    ///
    /// The handshake is upstream's: send `Hello {min_version, max_version,
    /// client}`, then wait for a frame whose `reply_to` is the hello's id. A
    /// bridge that answers `Error {UnsupportedVersion}` is refused here rather
    /// than discovered mid-turn.
    pub fn connect(path: &SocketPath) -> Result<Self, TransportError> {
        let stream = connect_with_timeout(path.as_path(), CONNECT_TIMEOUT)?;
        Self::from_stream(stream)
    }

    /// Build a transport over an already-connected stream.
    ///
    /// Used by tests, which can pass a socketpair or a stub server.
    pub fn from_stream(stream: UnixStream) -> Result<Self, TransportError> {
        stream
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .map_err(|e| TransportError::Connect(e.to_string()))?;
        let writer = stream.try_clone().map_err(|e| TransportError::Connect(e.to_string()))?;
        let mut transport = Self {
            reader: BufReader::new(stream),
            writer,
            next_id: 1,
            negotiated_version: 0,
            capabilities: Vec::new(),
        };
        let caps = transport.hello(crate::CLIENT_NAME)?;
        transport.capabilities = caps;
        Ok(transport)
    }

    /// Send one request, returning the id it was allocated.
    ///
    /// Ported from `HarnessClient::send` (`client.rs` lines 100-105): the id is
    /// taken then incremented, so ids are monotonic per connection.
    pub fn send(&mut self, request: ApiRequest) -> Result<u64, TransportError> {
        let id = self.next_id;
        self.next_id += 1;
        write_frame(&mut self.writer, &ClientFrame::new(id, request))?;
        Ok(id)
    }

    /// The next server frame, skipping unknown event kinds.
    ///
    /// Ported from `HarnessClient::recv` (`client.rs` lines 108-116). Skipping
    /// `Unknown` here rather than at the call site is what upstream's
    /// forward-compatibility rule requires: a newer bridge's event kinds must
    /// not desynchronise a client.
    pub fn recv(&mut self) -> Result<ServerFrame, TransportError> {
        loop {
            let frame: ServerFrame = read_frame(&mut self.reader)?;
            if matches!(frame.event, ApiEvent::Unknown) {
                continue;
            }
            return Ok(frame);
        }
    }

    /// Read frames until one replies to `expected`, returning the non-reply
    /// frames observed on the way.
    ///
    /// A request reply is interleaved with the session's streaming events, so a
    /// client that reads exactly one frame after sending would consume a
    /// `tool_start` and deadlock on the reply. The events it skipped past are
    /// returned so the caller can still map them.
    pub fn recv_reply(
        &mut self,
        expected: u64,
    ) -> Result<(ServerFrame, Vec<ServerFrame>), TransportError> {
        let mut events = Vec::new();
        loop {
            let frame = self.recv()?;
            match frame.reply_to {
                Some(id) if id == expected => return Ok((frame, events)),
                Some(_) => {
                    return Err(TransportError::LostReply {
                        expected,
                        got: frame.reply_to,
                    });
                }
                None => events.push(frame),
            }
        }
    }

    /// Perform the handshake and return the bridge's capability list.
    fn hello(&mut self, client_name: &str) -> Result<Vec<String>, TransportError> {
        let id = self.send(ApiRequest::Hello {
            min_version: API_VERSION_MAJOR,
            max_version: API_VERSION_MAJOR,
            client: client_name.to_string(),
            supports_user_response: false,
        })?;
        let (frame, _) = self.recv_reply(id)?;
        match frame.event {
            ApiEvent::HelloOk {
                version,
                capabilities,
                ..
            } => {
                if version != API_VERSION_MAJOR {
                    return Err(TransportError::Handshake(format!(
                        "bridge speaks protocol {version}, this crate speaks {API_VERSION_MAJOR}"
                    )));
                }
                self.negotiated_version = version;
                Ok(capabilities)
            }
            ApiEvent::Error { code, message } => Err(TransportError::Refused { code, message }),
            other => Err(TransportError::Handshake(format!(
                "expected hello_ok, got {other:?}"
            ))),
        }
    }

    /// The version the bridge accepted.
    pub fn negotiated_version(&self) -> u32 {
        self.negotiated_version
    }

    /// The capability strings the bridge advertised.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// True when the bridge advertised `capability`.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }
}

/// Connect to a Unix socket, giving up after `timeout`.
fn connect_with_timeout(path: &std::path::Path, timeout: Duration) -> Result<UnixStream, TransportError> {
    let deadline = Instant::now() + timeout;
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => return Ok(stream),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // ECONNREFUSED and EAGAIN mean the socket exists but is not
            // accepting yet: a bridge that has created the file but not
            // finished binding. Retrying is correct; other errors are not.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(e) => return Err(TransportError::Connect(format!("{path:?}: {e}"))),
        }
        if Instant::now() >= deadline {
            return Err(TransportError::Connect(format!(
                "no bridge accepted a connection at {path:?} within {:?}",
                timeout
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A running `jcode api-bridge` child process.
///
/// The bridge "dies when its parent shell dies", so the adapter owns its
/// lifecycle: it starts the bridge with `start_new_session(true)`, waits for the
/// socket to exist and accept a connection, and reaps the child on drop.
pub struct Bridge {
    child: Child,
    socket: SocketPath,
}

impl Bridge {
    /// Start a bridge listening on `socket`.
    ///
    /// `jcode api-bridge --api-socket <path>` is upstream's own entrypoint
    /// (`crates/jcode-harness-api-server/src/bin/bridge.rs`). The socket file is
    /// removed first when it is a stale socket, because `UnixStream::connect`
    /// to a dead socket's path returns `ConnectionRefused` and the bridge
    /// itself refuses to bind over an existing file.
    pub fn start(socket: SocketPath) -> Result<Self, BridgeError> {
        // Remove a stale socket left by a bridge that was killed.
        if let Ok(meta) = std::fs::symlink_metadata(socket.as_path()) {
            if meta.file_type().is_symlink() || meta.is_file() {
                std::fs::remove_file(socket.as_path())
                    .map_err(|e| BridgeError::Start(format!("cannot remove stale socket: {e}")))?;
            }
        }
        let child = Command::new("jcode")
            .arg("api-bridge")
            .arg("--api-socket")
            .arg(socket.as_path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BridgeError::Start(format!("cannot start jcode api-bridge: {e}")))?;
        let mut bridge = Self { child, socket };
        bridge.wait_for_socket()?;
        Ok(bridge)
    }

    /// Wait until the socket exists and accepts a connection.
    fn wait_for_socket(&mut self) -> Result<(), BridgeError> {
        let deadline = Instant::now() + BRIDGE_START_TIMEOUT;
        loop {
            if self.child_has_exited()? {
                return Err(BridgeError::Start(format!(
                    "the bridge exited before its socket appeared; stderr tail: {}",
                    self.stderr_tail()
                )));
            }
            if self.socket.as_path().exists() {
                // The file can exist before the listener is ready, so prove it
                // accepts a connection rather than trusting the path.
                match UnixStream::connect(self.socket.as_path()) {
                    Ok(_) => return Ok(()),
                    Err(_) => {}
                }
            }
            if Instant::now() >= deadline {
                let socket = self.socket.clone();
                let tail = self.stderr_tail();
                return Err(BridgeError::Start(format!(
                    "no bridge socket at {socket} within {BRIDGE_START_TIMEOUT:?}; stderr tail: {tail}"
                )));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// The socket this bridge listens on.
    pub fn socket(&self) -> &SocketPath {
        &self.socket
    }

    /// The bridge's process id, for the claim ledger's liveness probe.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// True when the child has already exited.
    pub fn child_has_exited(&self) -> Result<bool, BridgeError> {
        // `Child` is owned mutably in Drop; a non-mutating probe reads
        // /proc directly, which cannot disturb the handle.
        Ok(crate::reconcile::pid_is_alive(self.child.id()))
    }

    /// The tail of the bridge's stderr, for a diagnosis an operator can act on.
    ///
    /// Takes `&mut self` because `take()` moves the pipe out of the child. It
    /// is called only from `wait_for_socket`, which is already `&mut self`, so
    /// this costs nothing at the call sites.
    fn stderr_tail(&mut self) -> String {
        let Some(mut stderr) = self.child.stderr.take() else {
            return "<none>".into();
        };
        let mut buf = String::new();
        use std::io::Read;
        let _ = stderr.read_to_string(&mut buf);
        let lines: Vec<&str> = buf.lines().rev().take(3).collect();
        if lines.is_empty() {
            "<empty>".into()
        } else {
            lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // Kill then reap. `kill` alone leaves a zombie, and a zombie still
        // answers `kill(pid, 0)`, which would make the reconciler's liveness
        // probe report a live bridge that can serve nothing.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Errors from starting or supervising the bridge.
#[derive(Debug)]
pub enum BridgeError {
    /// The bridge could not be started, or died before its socket appeared.
    Start(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for BridgeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_resolution_prefers_the_api_socket_override() {
        // The env var is process-global; this test runs with a unique value and
        // restores it, and is not run in parallel with other env-touching tests.
        let key = "JCODE_API_SOCKET";
        let saved = std::env::var(key).ok();
        std::env::set_var(key, "/tmp/turbohermes-test-api.sock");
        let resolved = SocketPath::resolve();
        std::env::remove_var(key);
        if let Some(v) = saved {
            std::env::set_var(key, v);
        }
        assert_eq!(resolved.as_path(), std::path::Path::new("/tmp/turbohermes-test-api.sock"));
    }

    #[test]
    fn socket_resolution_falls_back_to_the_runtime_dir() {
        std::env::remove_var("JCODE_API_SOCKET");
        std::env::remove_var("JCODE_RUNTIME_DIR");
        let resolved = SocketPath::resolve();
        // Either $XDG_RUNTIME_DIR/jcode-api.sock or the temp fallback; both
        // end in jcode-api.sock, which is the part that must not drift.
        assert!(
            resolved.as_path().ends_with("jcode-api.sock"),
            "got {resolved}"
        );
    }

    #[test]
    fn a_stub_bridge_completes_a_hello_round_trip() {
        // A stand-in for the real bridge: it answers hello with the upstream
        // hello_ok shape, so the handshake's parsing is exercised without
        // needing a jcode binary.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stub.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let frame: ClientFrame = read_frame(&mut reader).unwrap();
            assert_eq!(frame.request, ApiRequest::Hello {
                min_version: API_VERSION_MAJOR,
                max_version: API_VERSION_MAJOR,
                client: crate::CLIENT_NAME.to_string(),
                supports_user_response: false,
            });
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::reply(
                    frame.id,
                    ApiEvent::HelloOk {
                        version: API_VERSION_MAJOR,
                        server: "jcode-harness-api-bridge/0.1.0".into(),
                        capabilities: vec!["soft_interrupt_ids".into()],
                    },
                ),
            )
            .unwrap();
            // Keep the connection open long enough for the client's assertions.
            std::thread::sleep(Duration::from_millis(200));
        });

        let transport = Transport::connect(&SocketPath::at(&path)).unwrap();
        assert_eq!(transport.negotiated_version(), API_VERSION_MAJOR);
        assert!(transport.has_capability("soft_interrupt_ids"));
        assert!(!transport.has_capability("runtime_commands"));
    }

    #[test]
    fn a_bridge_speaking_a_newer_major_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let frame: ClientFrame = read_frame(&mut reader).unwrap();
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::reply(
                    frame.id,
                    ApiEvent::HelloOk {
                        version: API_VERSION_MAJOR + 1,
                        server: "jcode-harness-api-bridge/9.9.9".into(),
                        capabilities: vec![],
                    },
                ),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });

        let err = Transport::connect(&SocketPath::at(&path)).unwrap_err();
        assert!(
            matches!(err, TransportError::Handshake(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn recv_reply_returns_the_events_it_walked_past() {
        // A bridge that emits a streaming event before the reply, which is the
        // real interleaving: a client reading one frame per send deadlocks.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("interleaved.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let hello: ClientFrame = read_frame(&mut reader).unwrap();
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::reply(
                    hello.id,
                    ApiEvent::HelloOk {
                        version: API_VERSION_MAJOR,
                        server: "stub".into(),
                        capabilities: vec![],
                    },
                ),
            )
            .unwrap();
            let ping: ClientFrame = read_frame(&mut reader).unwrap();
            assert_eq!(ping.request, ApiRequest::Ping);
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::event(ApiEvent::MessageAccepted {
                    session_id: "s1".into(),
                }),
            )
            .unwrap();
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::reply(ping.id, ApiEvent::Pong),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });

        let mut transport = Transport::connect(&SocketPath::at(&path)).unwrap();
        let id = transport.send(ApiRequest::Ping).unwrap();
        let (reply, events) = transport.recv_reply(id).unwrap();
        assert_eq!(reply.event, ApiEvent::Pong);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event,
            ApiEvent::MessageAccepted {
                session_id: "s1".into()
            }
        );
    }

    #[test]
    fn request_ids_are_monotonic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ids.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let hello: ClientFrame = read_frame(&mut reader).unwrap();
            write_frame(
                &mut stream.try_clone().unwrap(),
                &ServerFrame::reply(
                    hello.id,
                    ApiEvent::HelloOk {
                        version: API_VERSION_MAJOR,
                        server: "stub".into(),
                        capabilities: vec![],
                    },
                ),
            )
            .unwrap();
            for _ in 0..3 {
                let ping: ClientFrame = read_frame(&mut reader).unwrap();
                seen2.lock().unwrap().push(ping.id);
                write_frame(&mut stream.try_clone().unwrap(), &ServerFrame::reply(ping.id, ApiEvent::Pong)).unwrap();
            }
            std::thread::sleep(Duration::from_millis(200));
        });

        let mut transport = Transport::connect(&SocketPath::at(&path)).unwrap();
        for _ in 0..3 {
            let id = transport.send(ApiRequest::Ping).unwrap();
            let (reply, _) = transport.recv_reply(id).unwrap();
            assert_eq!(reply.event, ApiEvent::Pong);
        }
        assert_eq!(*seen.lock().unwrap(), vec![2, 3, 4]);
    }

    #[test]
    fn connecting_to_a_socket_that_never_appears_times_out_with_a_stated_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.sock");
        let err = Transport::connect(&SocketPath::at(&path)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("absent.sock"), "got {msg}");
    }
}
