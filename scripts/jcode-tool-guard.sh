#!/usr/bin/env bash
# jcode-tool-guard — pre_tool enforcement hook for jcode.
#
# Wired via [hooks] pre_tool in ~/.jcode/config.toml. Runs synchronously before
# every tool call with the full tool input JSON on stdin and JCODE_HOOK_TOOL_NAME
# in the environment. Contract (verified in crates/jcode-base/src/hooks.rs and
# docs/HOOKS.md): exit 0 allows, exit 2 blocks and the trimmed stderr becomes the
# tool error the model sees, anything else fails OPEN with a logged warning.
#
# This hook exists because measurement showed agents use efficient tools only when
# the brief demands it: the 2026-11-08 builds recorded zero clippy runs and zero
# scanner use in one case while every tool was wired and available. Doctrine in
# AGENTS.md is advisory; this is the mechanical version of the two rules that
# matter most for token bloat.
#
# Rules, deliberately conservative. A wrong block stalls a build, so each rule is
# narrow and everything unrecognised fails open:
#   1. A WHOLE-FILE read over 400 lines with no prior agentgrep this session is
#      blocked. Surgical reads (offset or limit present) always pass, as does any
#      file under the threshold or any read after agentgrep has run.
#   2. A verbose command that is not wrapped in rtk is blocked.
# Everything else passes.
#
# Session state: pre_tool fires before EVERY tool call, so an agentgrep call is
# recorded per JCODE_HOOK_SESSION_ID and satisfies rule 1 for the rest of the
# session. State dir is scratch (pruned after 24h idle; sessions are shorter).

STATE_DIR="${JCODE_TOOL_GUARD_STATE:-$HOME/.hermes/cache/scratch/jcode-tool-guard}"
SESSION="${JCODE_HOOK_SESSION_ID:-default}"

INPUT="$(cat)"
TOOL="${JCODE_HOOK_TOOL_NAME:-}"

# Fast path: nothing to inspect.
[ -z "$INPUT" ] && exit 0

# Experiment kill switch (owner-requested 3-arm test, 2026-11-09): setting
# JCODE_GUARD_DISABLED=1 makes this hook a no-op, which is how the
# "AGENTS.md only" and "prompt layer only" arms are run without editing the
# global config or disturbing live builds. Default is enforced.
[ "${JCODE_GUARD_DISABLED:-0}" = "1" ] && exit 0

mkdir -p "$STATE_DIR" 2>/dev/null || exit 0

debug_log() {
  if [ "${JCODE_TOOL_GUARD_DEBUG:-0}" = "1" ]; then
    printf '%s tool=%s session=%s decision=%s input=%s\n' \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$TOOL" "$SESSION" "$1" "$INPUT" \
      >> "$STATE_DIR/debug.log" 2>/dev/null || true
  fi
}

case "$TOOL" in
agentgrep)
    # Record that this session has used the AST-aware search; satisfies rule 1.
    : > "$STATE_DIR/$SESSION.agentgrep" 2>/dev/null || true
    debug_log "allow:agentgrep-recorded"
    exit 0
    ;;
esac

case "$TOOL" in
Read|read_file|read)
    FILE=$(printf '%s' "$INPUT" | jq -r '.file_path // .path // .filename // empty' 2>/dev/null)
    [ -z "$FILE" ] && exit 0
    OFFSET=$(printf '%s' "$INPUT" | jq -r '.offset // empty' 2>/dev/null)
    LIMIT=$(printf '%s' "$INPUT" | jq -r '.limit // empty' 2>/dev/null)
    # Surgical reads are exactly what we want; never block them.
    [ -n "$OFFSET" ] && exit 0
    [ -n "$LIMIT" ] && exit 0
    [ -f "$FILE" ] || exit 0
    LINES=$(wc -l < "$FILE" 2>/dev/null | tr -d ' ')
    [ -z "$LINES" ] && exit 0
    if [ "$LINES" -gt 400 ] && [ ! -f "$STATE_DIR/$SESSION.agentgrep" ]; then
        echo "BLOCKED by tool guard: whole-file read of $LINES lines with no prior agentgrep this session. Run agentgrep in outline mode first to find the region you need, then Read with offset and limit for that range only. The 400-line threshold is the workspace rule." >&2
        debug_log "block:read-large-no-agentgrep"
        exit 2
    fi
    debug_log "allow:read"
    exit 0
    ;;
Bash|bash|shell|terminal)
    CMD=$(printf '%s' "$INPUT" | jq -r '.command // .cmd // empty' 2>/dev/null)
    [ -z "$CMD" ] && exit 0
    # Documented escape hatch: an explicit opt-out passes through untouched.
    printf '%s' "$CMD" | grep -q 'JCODE_GUARD_OFF=1' && exit 0
    printf '%s' "$CMD" | grep -qE '^[[:space:]]*rtk[[:space:]]' && exit 0

    NEWCMD="$CMD"
    # QUALITY GATES (2026-11-10). Measured across 1,538 build commands: gitleaks,
    # ripsecrets, kingfisher and cargo-deny were invoked ZERO times, while the
    # builds ran grep 91 times and python3 55 times. The scanners and the CVE gate
    # are exactly the tools that must not be optional, so they are appended to the
    # commands that need them rather than requested in prose.
    case "$CMD" in
        git\ commit*)
            printf '%s' "$CMD" | grep -q 'gitleaks' || \
                NEWCMD="$CMD && gitleaks protect --staged --redact 2>&1 | tail -5"
            ;;
        cargo\ test*|cargo\ build*|cargo\ check*)
            printf '%s' "$CMD" | grep -q 'cargo fmt' || NEWCMD="$NEWCMD && cargo fmt --check"
            printf '%s' "$NEWCMD" | grep -q 'cargo deny' || NEWCMD="$NEWCMD && cargo deny check 2>&1 | tail -6"
            ;;
    esac
    # RTK WRAP. Measured on this host: rtk cut cargo test output 3,730 to 503
    # bytes (86.5%) and holds 474.7K tokens saved across 2,588 commands.
    REWRITTEN=$(rtk rewrite "$NEWCMD" 2>/dev/null)
    RC=$?
    if [ "$RC" -eq 3 ] && [ -n "$REWRITTEN" ] && [ "$REWRITTEN" != "$NEWCMD" ]; then
        NEWCMD="$REWRITTEN"
    fi
    # NOTE: no grep->rg rewrite here. rtk already wraps grep (verified above), and
    # a second rewrite on top would be a rule that never fires. If rtk's grep wrap
    # turns out to scan ignored paths, the fix belongs in rtk, not stacked here.
    [ "$NEWCMD" = "$CMD" ] && exit 0

    # WHICH CONTRACT AM I RUNNING UNDER? jcode has two separate pre-tool hooks:
    #   pre_tool          = a GATE. Exit 0 allows, exit 2 blocks, stdout is context.
    #   pre_tool_transform= an INPUT REPLACER. Exit 0 required, and stdout must be
    #                       the COMPLETE replacement input as a JSON object.
    # For 2026-11-10 the rewrite was wired only to pre_tool, so the JSON was
    # parsed as gate context and thrown away: the guard logged
    # "rewrite:bash->rtk" and jcode still ran the unwrapped command. That is why
    # the rtk ledger sat frozen at 2,588 across 397 build commands while rtk had
    # a rewrite available for every one of them.
    case "${JCODE_HOOK_EVENT:-}" in
        pre_tool_transform)
            # Emit the whole input object with the command replaced.
            printf '%s' "$INPUT" | jq --arg c "$NEWCMD" '.command = $c' 2>/dev/null
            debug_log "transform:bash->rtk"
            exit 0
            ;;
        *)
            # Gate contract: cannot rewrite, so allow and explain in context.
            printf '%s' "TOOL-GUARD: this command has an rtk-wrapped form (%s) that measured 86.5%% smaller on cargo test output. The rewrite is wired as pre_tool_transform; if it is not applying, check ~/.jcode/config.toml." "$NEWCMD" >&2
            debug_log "allow:bash-rewrite-unavailable"
            exit 0
            ;;
    esac
    ;;
esac
debug_log "allow:other-tool"
exit 0
