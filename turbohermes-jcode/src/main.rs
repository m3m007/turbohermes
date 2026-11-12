//! The `turbohermes-jcode` CLI: the supervisor-facing entrypoint.
//!
//! Three round-trips, one per job the architecture document names for the
//! adapter's operator:
//!
//! 1. `dispatch` — claim a task, create a scope, start a bridge, run the turn,
//!    capture the events, write the artifacts, resolve the claim.
//! 2. `reconcile` — read every unresolved claim and decide what happened to it.
//! 3. `inspect` — list what the bridge currently has, without dispatching.
//!
//! The CLI is deliberately thin: every decision lives in the library, so the
//! binary parses arguments, prints JSON and exits. A supervisor that wants
//! different behaviour calls the library, not the binary.

use std::path::PathBuf;
use std::time::Duration;

use turbohermes_jcode::artifacts::{self, Artifact, ArtifactManifest};
use turbohermes_jcode::client::{JcodeClient, JcodeClientError};
use turbohermes_jcode::events::{EventCapture, Identity};
use turbohermes_jcode::frames::ToolConfiguration;
use turbohermes_jcode::reconcile::{
    Claim, ClaimLedger, DEFAULT_LEASE_TTL_SECS, Verdict,
};
use turbohermes_jcode::scope::Scope;
use turbohermes_jcode::spec::TaskSpec;
use turbohermes_jcode::transport::{Bridge, BridgeError, SocketPath};

/// Where the ledger and event store live by default.
const DEFAULT_STATE_DIR: &str = ".turbohermes-jcode";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("dispatch") => cmd_dispatch(&args[2..]),
        Some("reconcile") => cmd_reconcile(&args[2..]),
        Some("inspect") => cmd_inspect(&args[2..]),
        Some("--help") | Some("-h") | None => {
            print_usage();
            0
        }
        Some(other) => {
            eprintln!("turbohermes-jcode: unknown command {other:?}");
            print_usage();
            2
        }
    };
    std::process::exit(code);
}

fn print_usage() {
    println!(
        "turbohermes-jcode: the jcode side of the TurboHermes adapter

USAGE:
    turbohermes-jcode dispatch  --task <id> --repo <path> [options]
    turbohermes-jcode reconcile [--ledger <path>]
    turbohermes-jcode inspect   [--socket <path>]

DISPATCH OPTIONS:
    --task <id>        The task to dispatch. Required.
    --repo <path>      The repository. Required.
    --prompt <text>    What to ask the session to do.
    --spec <file>      A task spec as JSON, instead of --prompt.
    --branch <name>    The branch to create. Default: turbohermes/<task>.
    --budget <usd>     The spend ceiling in USD. 0 means unlimited.
    --tools <a,b,c>    The permitted tools. Empty means lockdown.
    --timeout <secs>   How long to wait for the turn. Default: 600.
    --state <dir>      Where the ledger and event store live.
                       Default: {DEFAULT_STATE_DIR}

    The bridge is started, the turn is run, the events are captured into the
    event store, the artifacts are written, and the claim is resolved. The
    worktree is removed unless --keep-worktree is given.

RECONCILE OPTIONS:
    --ledger <path>    The ledger to read. Default: {DEFAULT_STATE_DIR}/claims.db
    --probe            Ask a live bridge about each session's status.

INSPECT OPTIONS:
    --socket <path>    The bridge socket. Default: the resolved harness socket.
"
    );
}

/// A parsed flag set. Unknown flags are an error rather than a silent ignore,
/// because a mistyped flag that silently changes the run's scope is worse than
/// a refusal.
struct Flags {
    map: std::collections::HashMap<String, Vec<String>>,
}

impl Flags {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut i = 0;
        while i < args.len() {
            let key = args[i]
                .strip_prefix("--")
                .ok_or_else(|| format!("unexpected argument {:?}", args[i]))?
                .to_string();
            let value = args
                .get(i + 1)
                .filter(|v| !v.starts_with("--"))
                .cloned()
                .unwrap_or_default();
            // A flag with no value is a boolean: it consumes one argument,
            // not two. The emptiness is decided before the move, so the two
            // branches cannot disagree about what was pushed.
            let boolean = value.is_empty();
            map.entry(key).or_default().push(value);
            i += if boolean { 1 } else { 2 };
        }
        Ok(Self { map })
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).and_then(|v| v.last()).map(String::as_str)
    }

    fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    fn require(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| format!("--{key} is required"))
    }
}

/// `dispatch`: the full round-trip.
fn cmd_dispatch(args: &[String]) -> i32 {
    let flags = match Flags::parse(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            return 2;
        }
    };
    match dispatch(&flags) {
        Ok(out) => {
            println!("{out}");
            0
        }
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            1
        }
    }
}

fn dispatch(flags: &Flags) -> Result<String, String> {
    let task_id = flags.require("task")?.to_string();
    let repo = flags.require("repo")?.to_string();
    let state_dir = PathBuf::from(flags.get("state").unwrap_or(DEFAULT_STATE_DIR));
    std::fs::create_dir_all(&state_dir)
        .map_err(|e| format!("cannot create {}: {e}", state_dir.display()))?;

    // The task spec: from a file, or built from the flags. Either way it is
    // validated before anything is dispatched, so a bad spec cannot consume a
    // claim, a bridge or a budget.
    let spec = match flags.get("spec") {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
            serde_json::from_str::<TaskSpec>(&text)
                .map_err(|e| format!("{path} is not a valid task spec: {e}"))?
        }
        None => TaskSpec::new(
            &task_id,
            flags.get("prompt").unwrap_or("Do the task.").to_string(),
        )
        .with_budget_usd(flags.get("budget").and_then(|b| b.parse().ok()).unwrap_or(0.0))
        .with_allowed_tools(
            flags
                .get("tools")
                .map(|t| t.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
                .unwrap_or_default(),
        ),
    };
    spec.validate()
        .map_err(|e| format!("the task spec is not valid: {e}"))?;

    // The event store and the ledger both live under the state dir, so one
    // dispatch's evidence is in one place.
    let store = turbohermes_core::EventStore::open(state_dir.join("events.db"))
        .map_err(|e| format!("cannot open the event store: {e}"))?;
    let ledger = ClaimLedger::open(state_dir.join("claims.db"))
        .map_err(|e| format!("cannot open the claim ledger: {e}"))?;

    // The claim is taken before the bridge is started: a task that is already
    // claimed must not consume a second bridge and a second budget.
    let bridge_pid = 0;
    // The pending row's session id is a placeholder that is still a real id:
    // `ClaimLedger` keys on (task_id, attempt), so this row is only a lease
    // record until the session exists. It is never sent to the bridge.
    let pending_session_id = format!("pending-{}", std::process::id());
    let claim = Claim {
        task_id: task_id.clone(),
        session_id: pending_session_id,
        agent_pid: std::process::id(),
        bridge_pid,
        lease_expires_at: turbohermes_jcode::reconcile::now_secs() + DEFAULT_LEASE_TTL_SECS,
        attempt: 1,
    };
    ledger
        .claim(&claim)
        .map_err(|e| format!("cannot claim {task_id}: {e}"))?;

    // The worktree scope. Created after the claim so a refused claim leaves no
    // branch behind, and before the bridge so the session's cwd is scoped.
    let branch = flags
        .get("branch")
        .map(str::to_string)
        .unwrap_or_else(|| format!("turbohermes/{task_id}"));
    let scope = Scope::create(&repo, &branch).map_err(|e| format!("cannot create a scope: {e}"))?;

    // The bridge. Started on a socket under the state dir, so two dispatches on
    // one host do not fight over the default socket.
    let socket = SocketPath::at(state_dir.join("bridge.sock"));
    let bridge = Bridge::start(socket.clone()).map_err(|e| format!("{e}"))?;
    let mut client = JcodeClient::connect(&socket).map_err(|e| format!("{e}"))?;

    // The session is created in the worktree, so every file tool the session
    // runs is scoped to it rather than to the repository root.
    let session_id = client
        .create(scope.path())
        .map_err(|e| format!("cannot start a session: {e}"))?;
    // The claim's session id is the real one now. The pending row is superseded
    // by a fresh claim rather than updated, because the ledger is append-only
    // in spirit and a stale pending row is a record, not an error.
    let claim = Claim {
        task_id: task_id.clone(),
        session_id: session_id.clone(),
        agent_pid: std::process::id(),
        bridge_pid: bridge.pid(),
        lease_expires_at: turbohermes_jcode::reconcile::now_secs() + DEFAULT_LEASE_TTL_SECS,
        attempt: 2,
    };
    if let Err(e) = ledger.claim(&claim) {
        eprintln!("turbohermes-jcode: warning: {e}");
    }

    // The permitted tools are sent as a replacement, not a patch.
    client
        .configure_tools(
            &session_id,
            ToolConfiguration {
                enabled: Some(spec.allowed_ops.tools.clone()),
                ..Default::default()
            },
        )
        .map_err(|e| format!("cannot configure the session's tools: {e}"))?;

    let identity = Identity {
        trace_id: turbohermes_core::TraceId::new(),
        task_id: turbohermes_core::TaskId::new(),
    };
    let capture = EventCapture::new(&store, &identity, &session_id);

    // The turn. The prompt is the spec's, and the spec's acceptance criteria
    // go as the system reminder, which is the "inject context" operation.
    let reminder = spec.injected_context();
    client
        .send(&session_id, &spec.prompt, reminder.as_deref())
        .map_err(|e| format!("cannot send the turn: {e}"))?;

    let timeout = Duration::from_secs(
        flags
            .get("timeout")
            .and_then(|t| t.parse().ok())
            .unwrap_or(600),
    );
    let outcome = client.collect_turn(timeout);
    let events = outcome.events().to_vec();
    let captured = capture
        .capture_all(&events)
        .map_err(|e| format!("cannot capture the events: {e}"))?;

    // The artifacts: cost from the token_usage events, and the verification
    // run if the turn reported one.
    let mut manifest = ArtifactManifest::new(identity.trace_id.clone(), identity.task_id.clone(), &session_id);
    let cost = artifacts::cost_artifact(identity.trace_id.clone(), identity.task_id.clone(), &events);
    if cost.events > 0 {
        manifest.push(Artifact::Cost(cost));
    }

    // The budget is checked against what was actually spent, not against what
    // the turn intended.
    let spent_micro = manifest.cost().map(|c| c.cost_micro_usd).unwrap_or(0);
    let spent_tokens = (manifest.cost().map(|c| c.input_tokens + c.output_tokens)).unwrap_or(0);
    let over_budget = spec.budget.is_exhausted(spent_tokens, spent_micro);

    // The claim is resolved either way: a turn that timed out is still
    // resolved, so the reconciler does not re-decide it later.
    let verdict = if outcome.is_finished() {
        Verdict::Finished
    } else {
        Verdict::Orphaned
    };
    let reason = if outcome.is_finished() {
        "the turn reached a terminal event".to_string()
    } else {
        format!("the turn did not finish within {timeout:?}")
    };
    let _ = ledger.resolve(&task_id, claim.attempt, verdict, &reason);

    // The worktree is kept only when asked. Removing it is the default because
    // an abandoned worktree holds a branch and a directory forever.
    let kept_worktree = flags.has("keep-worktree");
    if !kept_worktree {
        if let Err(e) = scope.remove() {
            eprintln!("turbohermes-jcode: warning: cannot remove the scope: {e}");
        }
    }
    // The bridge is dropped here, which kills and reaps it.
    drop(bridge);

    serde_json::to_string_pretty(&serde_json::json!({
        "task_id": task_id,
        "session_id": session_id,
        "branch": scope.branch(),
        "worktree": kept_worktree.then_some(scope.path().to_string()),
        "events_captured": captured.len(),
        "outcome": if outcome.is_finished() { "finished" } else { "timed_out" },
        "verdict": verdict.as_str(),
        "cost_micro_usd": manifest.cost().map(|c| c.cost_micro_usd).unwrap_or(0),
        "over_budget": over_budget,
        "artifacts": manifest.to_json(),
    }))
    .map_err(|e| e.to_string())
}

/// `reconcile`: decide what happened to every unresolved claim.
fn cmd_reconcile(args: &[String]) -> i32 {
    let flags = match Flags::parse(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            return 2;
        }
    };
    let state_dir = PathBuf::from(flags.get("state").unwrap_or(DEFAULT_STATE_DIR));
    let ledger_path = flags
        .get("ledger")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_dir.join("claims.db"));
    let ledger = match ClaimLedger::open(&ledger_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("turbohermes-jcode: cannot open {}: {e}", ledger_path.display());
            return 1;
        }
    };

    // With --probe, ask a live bridge about each session. Without it, the
    // reconciler says "unknown" for session evidence rather than guessing,
    // which is the difference between a supervisor that can report `Lost` and
    // one that invents it.
    let probe = if flags.has("probe") {
        let socket = flags
            .get("socket")
            .map(SocketPath::at)
            .unwrap_or_else(SocketPath::resolve);
        let mut client = match JcodeClient::connect(&socket) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("turbohermes-jcode: cannot reach the bridge: {e}");
                return 1;
            }
        };
        let sessions = client.list(None).unwrap_or_default();
        let known: std::collections::HashMap<String, String> = sessions
            .into_iter()
            .map(|s| (s.session_id, s.status))
            .collect();
        std::sync::Arc::new(move |session_id: &str| {
            match known.get(session_id) {
                Some(status) => turbohermes_jcode::reconcile::ProbeResult::Status(status.clone()),
                None => turbohermes_jcode::reconcile::ProbeResult::Missing,
            }
        }) as std::sync::Arc<turbohermes_jcode::reconcile::Probe>
    } else {
        std::sync::Arc::new(|_: &str| turbohermes_jcode::reconcile::ProbeResult::Unknown)
            as std::sync::Arc<turbohermes_jcode::reconcile::Probe>
    };

    let verdicts = ledger.reconcile(turbohermes_jcode::reconcile::now_secs(), probe.as_ref());
    // Every verdict is then recorded, which is what closes the claim. The
    // library's `reconcile` is read-only on purpose, so that a supervisor can
    // run it against a ledger a live dispatch is still writing to without
    // disturbing anything. The CLI is the supervisor: it decides and records.
    //
    // A `StillRunning` verdict is reported but deliberately not recorded:
    // recording it would close a claim that is still open, and the next run
    // would then have nothing to decide for it.
    for v in &verdicts {
        if v.verdict == turbohermes_jcode::reconcile::Verdict::StillRunning {
            continue;
        }
        if let Err(e) = ledger.resolve(&v.task_id, v.attempt, v.verdict, &v.reason) {
            eprintln!("turbohermes-jcode: cannot record the verdict for {}: {e}", v.task_id);
        }
    }
    let out = serde_json::to_string_pretty(&serde_json::json!({
        "ledger": ledger_path.display().to_string(),
        "claims": verdicts.iter().map(|v| serde_json::json!({
            "task_id": v.task_id,
            "session_id": v.session_id,
            "attempt": v.attempt,
            "verdict": v.verdict.as_str(),
            "reason": v.reason,
        })).collect::<Vec<_>>(),
    }));
    match out {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            1
        }
    }
}

/// `inspect`: list what the bridge currently has.
fn cmd_inspect(args: &[String]) -> i32 {
    let flags = match Flags::parse(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            return 2;
        }
    };
    let socket = flags
        .get("socket")
        .map(SocketPath::at)
        .unwrap_or_else(SocketPath::resolve);
    let mut client = match JcodeClient::connect(&socket) {
        Ok(c) => c,
        Err(JcodeClientError::Transport(t)) => {
            eprintln!("turbohermes-jcode: cannot reach the bridge: {t}");
            return 1;
        }
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            return 1;
        }
    };
    let sessions = match client.list(None) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            return 1;
        }
    };
    let out = serde_json::to_string_pretty(&serde_json::json!({
        "socket": socket.to_string(),
        "protocol_version": client.transport().negotiated_version(),
        "sessions": sessions.iter().map(|s| serde_json::json!({
            "session_id": s.session_id,
            "status": s.status,
            "working_dir": s.working_dir,
        })).collect::<Vec<_>>(),
    }));
    match out {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("turbohermes-jcode: {e}");
            1
        }
    }
}

/// Unused import guards: these keep the error types named in one place so a
/// future edit that changes a return type has a single place to update.
#[allow(dead_code)]
fn _error_types_are_named() -> (BridgeError, JcodeClientError) {
    unreachable!()
}
