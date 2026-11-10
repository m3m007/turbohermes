#!/usr/bin/env python3
"""turbohermes-watch — thin adapter over the Hermes kanban.

REDUCED 2026-11-10 from 536 lines to this. The old version reimplemented a phase
queue, dispatch caps, dependency ordering and state tracking, and produced six
bugs in one day: a pgrep self-match that made it think a build was always
running (so it never dispatched anything), an operator-precedence TypeError in
two places, and a phase["crate"] KeyError in three. Every one is a bug in
scratchpad code sitting beside a tested primitive.

The kanban already owns the queue, the ordering, the dispatch caps, the cooldown
guards, the worker scopes and the audit trail. So it owns them. This script does
the one thing the kanban cannot: spawn the jcode lane, which the kanban
dispatcher reports as "non-spawnable assignee - terminal lane".

The phase contracts live as kanban tickets (t_0e21e9aa through t_86892825),
linked parent->child so the kanban enforces work order v7's build order. This
script reads that queue and dispatches. It holds no phase data of its own.
"""
import re
import subprocess
import time
from pathlib import Path

LOGDIR = Path("/home/hermes/.hermes/logs")
LOG = LOGDIR / "turbohermes-watch.log"
KANBAN_DIR = Path("/home/hermes/projects/agent-daemon")
ENV = Path("/home/hermes/.hermes/.env")
ASSIGNEE = "jcode"
# The standing policy. The model-lane table in section 1 is the single source of
# truth for which model runs which phase; this script reads it rather than
# hard-coding a model, because a hard-coded model is how the three-lane
# instruction (GLM 5.3 architecture, step-5-preview grunt, deepseek verification)
# silently became one lane running step-5-preview for everything.
POLICY = KANBAN_DIR / "TURBOHERMES-POLICY.md"
FALLBACK_MODEL = "step-5-preview"


def log(msg):
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with LOG.open("a") as fh:
        fh.write("%s %s\n" % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), msg))


def sh(cmd, timeout=120, cwd=None):
    try:
        p = subprocess.run(cmd, shell=True, capture_output=True, text=True,
                           timeout=timeout, cwd=str(cwd) if cwd else None)
        return p.returncode, (p.stdout or "") + (p.stderr or "")
    except subprocess.TimeoutExpired:
        return 124, "timeout"


def jcode_pids():
    """Pids of live jcode builds, verified via /proc/<pid>/comm.

    TWO BUGS LIVED HERE, both from the same wrong pattern. The old code used
    `pgrep -f 'jcode --provider-profile ...'`, but the real binary is
    `jcode-linux-x86_64.bin --provider-profile ...`, so the pattern never
    matched the build at all. It matched the bash wrapper instead, which is
    always alive, so the watchdog believed a build was always running and never
    dispatched anything. The first fix then filtered comm to exactly "jcode",
    which excluded the real binary (its comm is "jcode-linux-x86_64") and
    dispatched a SECOND build into the same tree, the incident edace42.

    Correct pattern: `--provider-profile litellm`, which is genuinely in the
    build's command line. Correct comm test: anything starting with "jcode".
    """
    rc, out = sh("pgrep -f -- '--provider-profile litellm'")
    if rc != 0 or not out.strip():
        return []
    pids = []
    for pid in out.strip().splitlines():
        pid = pid.strip()
        if not pid:
            continue
        try:
            with open("/proc/%s/comm" % pid) as fh:
                comm = fh.read().strip()
        except OSError:
            continue
        if comm.startswith("jcode"):
            pids.append(pid)
    return pids


def ready_tickets():
    """Ready tickets for the jcode lane, in kanban order. The kanban has already
    applied its own dependency, cooldown and cap guards by the time a ticket is
    ready, so this list is authoritative and needs no second opinion."""
    rc, out = sh("hermes kanban list --assignee %s --status ready" % ASSIGNEE,
                 cwd=KANBAN_DIR)
    if rc != 0:
        return []
    return _ticket_ids(out)


def model_for(ticket_id):
    """The model for a ticket's phase, read from the standing policy.

    Parses the "Model lanes" table in TURBOHERMES-POLICY.md section 1. The ticket
    title carries the phase name (`v7 phase: <name>`), which is the join key. Falls
    back to the workhorse model and says so in the log, so a missing row is loud
    rather than silent.
    """
    rc, out = sh("hermes kanban show %s" % ticket_id, cwd=KANBAN_DIR)
    title = ""
    for line in out.splitlines():
        if "v7 phase:" in line:
            title = line.split("v7 phase:", 1)[1].strip()
            break
    phase = title.split()[0] if title else ""
    if not phase:
        log("model_for %s: no phase in title, using %s" % (ticket_id, FALLBACK_MODEL))
        return FALLBACK_MODEL, "unknown"
    if not POLICY.exists():
        log("model_for %s: no policy file, using %s" % (ticket_id, FALLBACK_MODEL))
        return FALLBACK_MODEL, phase
    lanes = {}
    in_table = False
    for line in POLICY.read_text().splitlines():
        if "Model lanes" in line:
            in_table = True
            continue
        if in_table:
            if line.startswith("##"):
                break
            if line.strip().startswith("|") and "---" not in line:
                cells = [c.strip() for c in line.strip().strip("|").split("|")]
                if len(cells) >= 3 and cells[0] and cells[0] != "Phase":
                    lanes[cells[0]] = (cells[1], cells[2])
    if phase not in lanes:
        log("model_for %s: phase %s not in the policy lane table, using %s"
            % (ticket_id, phase, FALLBACK_MODEL))
        return FALLBACK_MODEL, phase
    role, model = lanes[phase]
    log("model_for %s: phase=%s role=%s model=%s" % (ticket_id, phase, role, model))
    return model, phase


def dispatch(ticket_id):
    model, phase = model_for(ticket_id)
    rc, _ = sh("hermes kanban claim %s" % ticket_id, cwd=KANBAN_DIR)
    if rc != 0:
        log("dispatch %s: claim failed rc=%d" % (ticket_id, rc))
        return False
    key = ""
    for line in ENV.read_text().splitlines():
        if line.startswith("LITELLM_MASTER_KEY="):
            key = line.split("=", 1)[1].strip().strip('"').strip("'")
    logf = LOGDIR / ("jcode-%s.log" % ticket_id)
    cmd = (
        "source %s 2>/dev/null; export JCODE_PROVIDER_LITELLM_API_KEY=\"%s\"; "
        "jcode --provider-profile litellm --model %s run "
        "\"Run: hermes kanban show %s --body, read that body, and execute it "
        "exactly. Work order v7 is the authority: reuse before write, and name "
        "every upstream file you copy or port from. Report only what you ran "
        "and observed.\""
        % (ENV, key, model, ticket_id)
    )
    with open(logf, "a") as lf:
        subprocess.Popen(["bash", "-lc", cmd], cwd=str(KANBAN_DIR),
                         stdout=lf, stderr=subprocess.STDOUT,
                         start_new_session=True)
    log("DISPATCHED %s (phase %s) to jcode on model %s, log %s"
        % (ticket_id, phase, model, logf.name))
    return True


def _ticket_ids(out):
    """Ticket ids from kanban list output, marker-independent.

    The marker changes with the output context: `▶` when stdout is a terminal,
    `●` when it is captured, and `⊘`/`✓`/`◻` for other states. A parser keyed on
    the marker silently returns nothing when the context changes, which is how
    running_tickets() came back empty while a ticket was plainly in `running`.
    Match the id instead; it is stable.
    """
    ids = []
    for line in out.splitlines():
        for m in re.findall(r"\b(t_[a-f0-9]{8})\b", line):
            if m not in ids:
                ids.append(m)
    return ids


def running_tickets():
    """Tickets this lane has claimed but not yet completed.

    The defect this closes: dispatch() claims a ticket, which moves it to
    'running', and nothing ever completed it. A finished phase therefore sat in
    'running' forever and the next tick re-dispatched work that was already
    done. The kanban owns the state; this function is how the adapter tells the
    kanban the build is over.
    """
    rc, out = sh("hermes kanban list --assignee %s --status running" % ASSIGNEE,
                 cwd=KANBAN_DIR)
    if rc != 0:
        return []
    return _ticket_ids(out)


# A build that ends on one of these did not succeed, and completing its ticket
# would record a failure as a delivery. Leave it running and say why.
FAIL_SIGS = (
    "451 Unavailable For Legal Reasons",
    "Error: OpenAI-compatible chat request failed",
    "Traceback (most recent call last)",
    "panicked at",
)
# A log still being written is not a finished build. 180s of quiet is the bar.
QUIET_SECONDS = 180


def log_evidence(ticket_id):
    """(result_text, None) if the build can be completed, else (None, reason).

    Evidence comes from the build log and is reported verbatim: token counts,
    test-binary counts, and the last commit in the worktree. No invention.
    """
    logf = LOGDIR / ("jcode-%s.log" % ticket_id)
    if not logf.exists():
        return None, "no build log at %s" % logf
    age = time.time() - logf.stat().st_mtime
    if age < QUIET_SECONDS:
        return None, "log still active (%.0fs since last write, need %ds)" % (
            age, QUIET_SECONDS)
    text = logf.read_text(errors="replace")
    tail = text[-8000:]
    for sig in FAIL_SIGS:
        if sig in tail:
            return None, "build log ends on a failure signature: %s" % sig
    ev = []
    toks = re.findall(r"\[Tokens\] upload: (\d+)", text)
    if toks:
        ev.append("jcode tokens %s" % toks[-1])
    tests = re.findall(r"test result: ok\. (\d+) passed", text)
    if tests:
        ev.append("%d test binaries ok" % len(tests))
    rc, out = sh("git -C %s log --oneline -1" % (KANBAN_DIR.parent / "turbohermes-wt"),
                 timeout=30)
    if rc == 0 and out.strip():
        ev.append("last worktree commit %s" % out.strip().split()[0])
    if not ev:
        return None, "log has no measurable evidence (no tokens, no tests, no commit)"
    return "Build finished. Evidence from the log: %s." % "; ".join(ev), None


def complete_finished():
    """Close tickets whose build has provably finished. Returns count closed."""
    closed = 0
    for tid in running_tickets():
        result, why = log_evidence(tid)
        if result is None:
            log("NOT-COMPLETING %s: %s" % (tid, why))
            continue
        rc, out = sh("hermes kanban complete %s --result %s" % (
            tid, subprocess.list2cmdline([result])), cwd=KANBAN_DIR)
        if rc == 0:
            log("COMPLETED %s" % tid)
            closed += 1
        else:
            log("COMPLETE-FAILED %s rc=%d %s" % (tid, rc, out.strip()[:160]))
    return closed


def main():
    alive = jcode_pids()
    if alive:
        log("RUNNING pids=%s" % ",".join(alive))
        return
    # No build alive: anything this lane claimed is finished or dead. Close what
    # has evidence, leave the rest with a reason logged.
    complete_finished()
    tickets = ready_tickets()
    if not tickets:
        log("IDLE no ready %s tickets" % ASSIGNEE)
        return
    if not dispatch(tickets[0]):
        log("IDLE dispatch of %s failed" % tickets[0])


if __name__ == "__main__":
    main()
