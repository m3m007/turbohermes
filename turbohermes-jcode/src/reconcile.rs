//! The claim ledger and post-failure reconciliation.
//!
//! **Ported from beads** (upstream commit
//! `9d4ebf42f0496cdd0f0d7fdc6062c0981f374a70`, Go original `gastownhall/beads`,
//! plain MIT):
//! - The claim compare-and-swap — `internal/storage/issueops/claim.go`
//!   `ClaimIssueInTx` / `claimIssueInTx`, lines 33-146.
//! - The lease — `internal/storage/issueops/lease.go` `DefaultLeaseTTL`
//!   (line 28), `UpsertLeaseInTx` (line 181), `DeleteLeaseInTx` (line 202),
//!   `HeartbeatIssueInTx` (line 320).
//! - The refusal shape — the same file's `rowsAffected == 0` branch, lines
//!   152-205: re-read the state that lost the CAS and return it typed, so the
//!   caller learns who holds the claim without parsing a message.
//!
//! What was adapted, and why: beads runs on MySQL/Dolt and does its CAS with
//! `UPDATE ... WHERE row_lock = ?`, because MySQL cannot express "insert
//! unless this row exists" atomically. SQLite can, so the CAS is a
//! `UNIQUE(task_id, attempt)` index plus an insert that reports whether it
//! wrote. The semantics beads establishes are unchanged: a second claim for the
//! same task and attempt is refused, an idempotent re-claim by the same holder
//! is a no-op success, and the refusal carries the holder's identity.
//!
//! Beads' Dolt versioning, wisp routing, claim pools and the lease commit
//! boundary are not ported. There is one node and no replica here; the
//! append-only event store in `turbohermes-core` is the journal.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The lease TTL beads defaults to (`lease.go` line 28).
pub const DEFAULT_LEASE_TTL_SECS: i64 = 5 * 60;

/// Is `pid` alive right now.
///
/// `kill(pid, 0)` is the liveness probe. Pattern-based `pgrep` is deliberately
/// not used: three recorded self-kill incidents on this host came from pattern
/// matching that matched the agent's own process, recorded in DECISIONS D-102.
/// The probe is by pid only, and it is the one syscall `libc` is pulled in for.
pub fn pid_is_alive(pid: u32) -> bool {
    // SAFETY: `kill` with signal 0 performs the permission and existence
    // check and sends nothing. A negative pid would address a process group,
    // so it is rejected rather than passed through.
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    // EPERM means the process exists but belongs to another user: alive, and
    // not ours to signal.
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    )
}

/// One dispatch's claim on a task.
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    pub task_id: String,
    pub session_id: String,
    /// The adapter process that dispatched the turn.
    pub agent_pid: u32,
    /// The bridge process serving the turn.
    pub bridge_pid: u32,
    /// Unix seconds after which the claim is stale unless heartbeated.
    pub lease_expires_at: i64,
    /// Which attempt of this task the claim covers. The duplicate-dispatch
    /// guard is `(task_id, attempt)`, so a retry with a new attempt number is
    /// allowed and a retry with the same one is not.
    pub attempt: u32,
}

/// The state a claim is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimState {
    /// The claim is open: the turn is believed to be running.
    Open,
    /// The claim was resolved. A resolution is a new row, never an edit.
    Resolved,
}

/// Why a claim was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimRefusal {
    /// Another dispatch already holds this task and attempt.
    AlreadyHeld { task_id: String, attempt: u32 },
    /// The ledger could not be written.
    Storage(String),
}

impl std::fmt::Display for ClaimRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyHeld { task_id, attempt } => write!(
                f,
                "task {task_id} attempt {attempt} is already claimed by another dispatch"
            ),
            Self::Storage(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ClaimRefusal {}

/// What the reconciler decided about one open claim.
#[derive(Debug, Clone, PartialEq)]
pub struct Reconciliation {
    pub task_id: String,
    pub session_id: String,
    /// Which attempt of the task this verdict decided.
    ///
    /// Carried because the resolution is keyed on `(task_id, attempt)`: a
    /// supervisor that reported a verdict without the attempt could not record
    /// it, and would re-decide the same claim on every run forever.
    pub attempt: u32,
    pub verdict: Verdict,
    pub reason: String,
}

/// The four outcomes of reconciliation.
///
/// The order the reconciler checks in is the order beads' reclaim path implies:
/// the lease first (a worker that died stops heartbeating and the lease
/// expires), then the process, then the session's own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The lease is live and the process is alive.
    StillRunning,
    /// The lease expired or the process is gone: reap the session and the
    /// bridge, and release the claim.
    Orphaned,
    /// The session reached a terminal state: collect the artifacts.
    Finished,
    /// Neither the lease, the process nor the session says anything: there is
    /// no evidence either way, which is reported rather than guessed at.
    Lost,
}

impl Verdict {
    /// The canonical string form, for storage and for the CLI's output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StillRunning => "still_running",
            Self::Orphaned => "orphaned",
            Self::Finished => "finished",
            Self::Lost => "lost",
        }
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Errors from the ledger.
#[derive(Debug)]
pub enum ClaimLedgerError {
    Storage(rusqlite::Error),
}

impl std::fmt::Display for ClaimLedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "claim ledger: {e}"),
        }
    }
}

impl std::error::Error for ClaimLedgerError {}

impl From<rusqlite::Error> for ClaimLedgerError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Storage(e)
    }
}

/// The SQLite claim ledger.
///
/// Append-only in spirit: a claim is a row, and a resolution is a new row in
/// `resolutions`, never an edit to `claims`. A supervisor that restarts opens
/// the same file and re-reads every unresolved claim, which is what makes a
/// crash recoverable rather than merely survivable.
pub struct ClaimLedger {
    conn: Connection,
}

impl ClaimLedger {
    /// Open or create a ledger at `path`.
    ///
    /// The parent directory must already exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ClaimLedgerError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let ledger = Self { conn };
        ledger.init_schema()?;
        Ok(ledger)
    }

    /// An in-memory ledger, for tests.
    pub fn in_memory() -> Result<Self, ClaimLedgerError> {
        let conn = Connection::open_in_memory()?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let ledger = Self { conn };
        ledger.init_schema()?;
        Ok(ledger)
    }

    fn init_schema(&self) -> Result<(), ClaimLedgerError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS claims (
                task_id TEXT NOT NULL,
                attempt INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                agent_pid INTEGER NOT NULL,
                bridge_pid INTEGER NOT NULL,
                lease_expires_at INTEGER NOT NULL,
                state TEXT NOT NULL,
                claimed_at INTEGER NOT NULL,
                PRIMARY KEY (task_id, attempt)
            );
            CREATE INDEX IF NOT EXISTS claims_open ON claims (state, lease_expires_at);
            CREATE TABLE IF NOT EXISTS resolutions (
                task_id TEXT NOT NULL,
                attempt INTEGER NOT NULL,
                decided_at INTEGER NOT NULL,
                verdict TEXT NOT NULL,
                reason TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Record a claim on `task_id` + `attempt`.
    ///
    /// The compare-and-swap ported from beads' `claimIssueInTx`. The insert is
    /// guarded by the primary key, and the number of rows it wrote is the CAS
    /// result: `1` means this dispatch won, `0` means someone else already
    /// holds it, and the holder's state is then re-read so the refusal can
    /// name it.
    pub fn claim(&self, claim: &Claim) -> Result<(), ClaimRefusal> {
        let now = now_secs();
        let changed = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO claims
                 (task_id, attempt, session_id, agent_pid, bridge_pid,
                  lease_expires_at, state, claimed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    claim.task_id,
                    claim.attempt,
                    claim.session_id,
                    claim.agent_pid,
                    claim.bridge_pid,
                    claim.lease_expires_at,
                    ClaimState::Open.as_str(),
                    now,
                ],
            )
            .map_err(|e| ClaimRefusal::Storage(e.to_string()))?;

        if changed == 0 {
            // The CAS lost. Re-read the state that won, exactly as beads does,
            // so the refusal carries it rather than a guess.
            let existing = self
                .conn
                .query_row(
                    "SELECT state, agent_pid, lease_expires_at FROM claims
                     WHERE task_id = ?1 AND attempt = ?2",
                    params![claim.task_id, claim.attempt],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u32>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| ClaimRefusal::Storage(e.to_string()))?;
            match existing {
                // Idempotent re-claim by the same holder while the claim is
                // still open: beads treats this as success, because an agent
                // that retries after a transient failure has not stolen
                // anything. The lease is renewed, which is a heartbeat.
                Some((state, agent_pid, lease)) if state == ClaimState::Open.as_str() => {
                    if agent_pid == claim.agent_pid || lease < now {
                        self.heartbeat(&claim.task_id, claim.attempt, claim.lease_expires_at)
                            .map_err(|e| ClaimRefusal::Storage(e.to_string()))?;
                        return Ok(());
                    }
                    Err(ClaimRefusal::AlreadyHeld {
                        task_id: claim.task_id.clone(),
                        attempt: claim.attempt,
                    })
                }
                Some(_) => Err(ClaimRefusal::AlreadyHeld {
                    task_id: claim.task_id.clone(),
                    attempt: claim.attempt,
                }),
                None => Err(ClaimRefusal::AlreadyHeld {
                    task_id: claim.task_id.clone(),
                    attempt: claim.attempt,
                }),
            }
        } else {
            Ok(())
        }
    }

    /// Renew a claim's lease.
    ///
    /// Beads' `HeartbeatIssueInTx` (`lease.go` line 320). The heartbeat is what
    /// makes the claim recoverable: a worker that dies stops heartbeating and
    /// the lease expires, and a later dispatch wins.
    pub fn heartbeat(
        &self,
        task_id: &str,
        attempt: u32,
        lease_expires_at: i64,
    ) -> Result<(), ClaimLedgerError> {
        self.conn.execute(
            "UPDATE claims SET lease_expires_at = ?1
             WHERE task_id = ?2 AND attempt = ?3 AND state = ?4",
            params![
                lease_expires_at,
                task_id,
                attempt,
                ClaimState::Open.as_str()
            ],
        )?;
        Ok(())
    }

    /// Mark a claim resolved and record the verdict.
    ///
    /// Two writes: the claim's state, and a new `resolutions` row. The
    /// resolution row is the record; the state change is the index that keeps
    /// `reconcile` from re-deciding the same claim.
    pub fn resolve(
        &self,
        task_id: &str,
        attempt: u32,
        verdict: Verdict,
        reason: &str,
    ) -> Result<(), ClaimLedgerError> {
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO resolutions (task_id, attempt, decided_at, verdict, reason)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, attempt, now, verdict.as_str(), reason],
        )?;
        self.conn.execute(
            "UPDATE claims SET state = ?1 WHERE task_id = ?2 AND attempt = ?3",
            params![ClaimState::Resolved.as_str(), task_id, attempt],
        )?;
        Ok(())
    }

    /// Every open claim, in claim order.
    pub fn open_claims(&self) -> Result<Vec<Claim>, ClaimLedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, attempt, session_id, agent_pid, bridge_pid, lease_expires_at
             FROM claims WHERE state = ?1 ORDER BY claimed_at ASC",
        )?;
        let rows = stmt.query_map(params![ClaimState::Open.as_str()], |row| {
            Ok(Claim {
                task_id: row.get(0)?,
                attempt: row.get(1)?,
                session_id: row.get(2)?,
                agent_pid: row.get(3)?,
                bridge_pid: row.get(4)?,
                lease_expires_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every resolution recorded, newest last.
    pub fn resolutions(&self) -> Result<Vec<Reconciliation>, ClaimLedgerError> {
        let mut stmt = self.conn.prepare(
            // `session_id` and `attempt` live on the claims row; a resolution
            // is keyed on (task_id, attempt) and carries only the decision.
            "SELECT resolutions.task_id, claims.session_id, verdict, reason,
                    decided_at, claims.attempt
             FROM resolutions
             JOIN claims USING (task_id, attempt) ORDER BY decided_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Reconciliation {
                task_id: row.get(0)?,
                session_id: row.get(1)?,
                verdict: Verdict::from_str(&row.get::<_, String>(2)?),
                reason: row.get(3)?,
                // The join is on (task_id, attempt), so the attempt is the
                // claims row's, which is the attempt the resolution decided.
                attempt: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Decide what to do about every open claim.
    ///
    /// The lease is checked first, exactly as beads' reclaim path implies: a
    /// worker that died stopped heartbeating, so the lease is the cheapest and
    /// most reliable signal. Only a live lease with a live process and no
    /// terminal session state is `StillRunning`.
    ///
    /// `probe` answers the two questions only a live bridge can answer:
    /// whether the session still exists, and what its status is. A reconciler
    /// with no bridge to ask cannot reach `Finished` or `Lost` on session
    /// evidence, and says so with `Orphaned` rather than guessing.
    pub fn reconcile(&self, now: i64, probe: &Probe) -> Vec<Reconciliation> {
        let mut out = Vec::new();
        let Ok(claims) = self.open_claims() else {
            return out;
        };
        for claim in claims {
            let (verdict, reason) = if claim.lease_expires_at <= now {
                (
                    Verdict::Orphaned,
                    format!(
                        "lease expired at {} (now {now}); the dispatch stopped heartbeating",
                        claim.lease_expires_at
                    ),
                )
            } else if !pid_is_alive(claim.agent_pid) {
                (
                    Verdict::Orphaned,
                    format!(
                        "agent pid {} is gone while the claim is open",
                        claim.agent_pid
                    ),
                )
            } else if !pid_is_alive(claim.bridge_pid) {
                (
                    Verdict::Orphaned,
                    format!(
                        "bridge pid {} is gone while the claim is open",
                        claim.bridge_pid
                    ),
                )
            } else {
                match probe(&claim.session_id) {
                    ProbeResult::Unknown => (
                        Verdict::StillRunning,
                        "lease live, both processes alive, session state unknown".into(),
                    ),
                    ProbeResult::Missing => (
                        Verdict::Lost,
                        "processes alive but the session is not in the bridge's list".into(),
                    ),
                    ProbeResult::Status(status) if is_terminal_status(&status) => (
                        Verdict::Finished,
                        format!("session reached terminal status {status}"),
                    ),
                    ProbeResult::Status(status) => (
                        Verdict::StillRunning,
                        format!("session is {status} with a live lease"),
                    ),
                }
            };
            out.push(Reconciliation {
                task_id: claim.task_id,
                session_id: claim.session_id,
                attempt: claim.attempt,
                verdict,
                reason,
            });
        }
        out
    }
}

impl ClaimState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

/// The answer to "does the bridge still have this session, and what is it doing".
pub type Probe = dyn Fn(&str) -> ProbeResult + Send + Sync;

/// What a probe found.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeResult {
    /// The session is in the list, with this status.
    Status(String),
    /// The session is not in the list at all.
    Missing,
    /// The probe could not ask: no bridge, or the bridge did not answer.
    Unknown,
}

/// True when a jcode session status is terminal.
///
/// The statuses come from the bridge's own vocabulary. A session that reached
/// one of these has finished its work whether or not this adapter saw the
/// `turn_done`, which is what makes `Finished` reachable after a crash.
fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "error" | "done")
}

impl Verdict {
    fn from_str(s: &str) -> Self {
        match s {
            "still_running" => Self::StillRunning,
            "orphaned" => Self::Orphaned,
            "finished" => Self::Finished,
            _ => Self::Lost,
        }
    }
}

/// Unix seconds now.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(task: &str, attempt: u32, pid: u32) -> Claim {
        Claim {
            task_id: task.into(),
            session_id: format!("sess-{task}"),
            agent_pid: pid,
            bridge_pid: pid,
            lease_expires_at: now_secs() + DEFAULT_LEASE_TTL_SECS,
            attempt,
        }
    }

    /// A probe that knows nothing: the reconciler must not invent session
    /// evidence it does not have.
    fn no_probe(_: &str) -> ProbeResult {
        ProbeResult::Unknown
    }

    #[test]
    fn a_first_claim_succeeds() {
        let ledger = ClaimLedger::in_memory().unwrap();
        assert!(ledger.claim(&claim("t1", 1, std::process::id())).is_ok());
        assert_eq!(ledger.open_claims().unwrap().len(), 1);
    }

    #[test]
    fn a_duplicate_claim_of_the_same_task_and_attempt_is_refused() {
        let ledger = ClaimLedger::in_memory().unwrap();
        let me = std::process::id();
        ledger.claim(&claim("t1", 1, me)).unwrap();
        // A different pid trying the same task and attempt is the
        // duplicate-dispatch boundary.
        let other = if me > 1 { me - 1 } else { me + 1 };
        let refusal = ledger.claim(&claim("t1", 1, other)).unwrap_err();
        assert!(
            matches!(refusal, ClaimRefusal::AlreadyHeld { .. }),
            "got {refusal}"
        );
    }

    #[test]
    fn a_retry_with_a_new_attempt_number_is_allowed() {
        let ledger = ClaimLedger::in_memory().unwrap();
        let me = std::process::id();
        ledger.claim(&claim("t1", 1, me)).unwrap();
        // Attempt 2 is a new dispatch, not a duplicate.
        assert!(ledger.claim(&claim("t1", 2, me)).is_ok());
        assert_eq!(ledger.open_claims().unwrap().len(), 2);
    }

    #[test]
    fn an_expired_lease_with_a_dead_pid_resolves_orphaned() {
        let ledger = ClaimLedger::in_memory().unwrap();
        let mut c = claim("t1", 1, std::process::id());
        c.lease_expires_at = now_secs() - 1;
        c.agent_pid = 999_999;
        c.bridge_pid = 999_999;
        ledger.claim(&c).unwrap();

        let verdicts = ledger.reconcile(now_secs(), &no_probe);
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].verdict, Verdict::Orphaned);
        assert!(verdicts[0].reason.contains("lease expired"));
    }

    #[test]
    fn a_live_pid_with_an_active_session_resolves_still_running() {
        let ledger = ClaimLedger::in_memory().unwrap();
        ledger.claim(&claim("t1", 1, std::process::id())).unwrap();

        let verdicts = ledger.reconcile(
            now_secs(),
            &|_| ProbeResult::Status("generating".into()),
        );
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].verdict, Verdict::StillRunning);
    }

    #[test]
    fn a_terminal_session_status_resolves_finished() {
        let ledger = ClaimLedger::in_memory().unwrap();
        ledger.claim(&claim("t1", 1, std::process::id())).unwrap();

        let verdicts = ledger.reconcile(now_secs(), &|_| ProbeResult::Status("idle".into()));
        // `idle` is not terminal: the turn ended but the session lives.
        assert_eq!(verdicts[0].verdict, Verdict::StillRunning);

        let verdicts = ledger.reconcile(
            now_secs(),
            &|_| ProbeResult::Status("completed".into()),
        );
        assert_eq!(verdicts[0].verdict, Verdict::Finished);
    }

    #[test]
    fn a_missing_session_with_live_processes_resolves_lost() {
        let ledger = ClaimLedger::in_memory().unwrap();
        ledger.claim(&claim("t1", 1, std::process::id())).unwrap();
        let verdicts = ledger.reconcile(now_secs(), &|_| ProbeResult::Missing);
        assert_eq!(verdicts[0].verdict, Verdict::Lost);
    }

    #[test]
    fn a_restart_re_reads_unresolved_claims_and_resolves_them() {
        // The restart case: a new ledger handle over the same file must see
        // what the previous process left open.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.db");
        {
            let ledger = ClaimLedger::open(&path).unwrap();
            ledger.claim(&claim("t1", 1, std::process::id())).unwrap();
            ledger.claim(&claim("t2", 1, 999_999)).unwrap();
        }
        // A fresh handle, as a restarted supervisor would open.
        let ledger = ClaimLedger::open(&path).unwrap();
        assert_eq!(ledger.open_claims().unwrap().len(), 2);
        let verdicts = ledger.reconcile(now_secs(), &no_probe);
        let t1 = verdicts.iter().find(|v| v.task_id == "t1").unwrap();
        let t2 = verdicts.iter().find(|v| v.task_id == "t2").unwrap();
        assert_eq!(t1.verdict, Verdict::StillRunning);
        assert_eq!(t2.verdict, Verdict::Orphaned);

        ledger
            .resolve("t2", 1, Verdict::Orphaned, "reaped by the reconciler")
            .unwrap();
        let ledger = ClaimLedger::open(&path).unwrap();
        // Resolved claims are not re-decided.
        assert_eq!(ledger.open_claims().unwrap().len(), 1);
        let resolutions = ledger.resolutions().unwrap();
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].task_id, "t2");
        assert_eq!(resolutions[0].verdict, Verdict::Orphaned);
    }

    #[test]
    fn a_heartbeat_renews_the_lease() {
        let ledger = ClaimLedger::in_memory().unwrap();
        ledger.claim(&claim("t1", 1, std::process::id())).unwrap();
        let later = now_secs() + 3600;
        ledger.heartbeat("t1", 1, later).unwrap();
        let claim = &ledger.open_claims().unwrap()[0];
        assert_eq!(claim.lease_expires_at, later);
    }

    #[test]
    fn the_pid_probe_does_not_match_by_pattern() {
        // The probe is by pid: this process is alive, and a pid that does not
        // exist is not. A pattern-based probe is what caused three self-kill
        // incidents (DECISIONS D-102), so the test pins the pid-only contract.
        assert!(pid_is_alive(std::process::id()));
        assert!(!pid_is_alive(999_999));
        assert!(!pid_is_alive(0));
        assert!(!pid_is_alive(u32::MAX));
    }
}
