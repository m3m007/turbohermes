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
import subprocess
import time
from pathlib import Path

LOGDIR = Path("/home/hermes/.hermes/logs")
LOG = LOGDIR / "turbohermes-watch.log"
KANBAN_DIR = Path("/home/hermes/projects/agent-daemon")
ENV = Path("/home/hermes/.hermes/.env")
ASSIGNEE = "jcode"


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
    ids = []
    for line in out.splitlines():
        line = line.strip()
        if line[:1] in ("▶", "⊘"):
            parts = line.split()
            if len(parts) > 1 and parts[1].startswith("t_"):
                ids.append(parts[1])
    return ids


def dispatch(ticket_id):
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
        "jcode --provider-profile litellm --model step-5-preview run "
        "\"Run: hermes kanban show %s --body, read that body, and execute it "
        "exactly. Work order v7 is the authority: reuse before write, and name "
        "every upstream file you copy or port from. Report only what you ran "
        "and observed.\""
        % (ENV, key, ticket_id)
    )
    with open(logf, "a") as lf:
        subprocess.Popen(["bash", "-lc", cmd], cwd=str(KANBAN_DIR),
                         stdout=lf, stderr=subprocess.STDOUT,
                         start_new_session=True)
    log("DISPATCHED %s to jcode, log %s" % (ticket_id, logf.name))
    return True


def main():
    alive = jcode_pids()
    if alive:
        log("RUNNING pids=%s" % ",".join(alive))
        return
    tickets = ready_tickets()
    if not tickets:
        log("IDLE no ready %s tickets" % ASSIGNEE)
        return
    if not dispatch(tickets[0]):
        log("IDLE dispatch of %s failed" % tickets[0])


if __name__ == "__main__":
    main()
