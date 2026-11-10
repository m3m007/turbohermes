# Phase 0 outcomes (auto-recorded by turbohermes-watch)

Recorded 2026-11-10 10:47:03 UTC. Read-only checks; nothing was configured or installed.

- **P0.1_decisions_route**: `405`
- **P0.1_systemone_route**: `405`
- **P0.2_litellm_version**: `LiteLLM: Current Version = 1.110.0`
- **P0.3_clef_in_config**: `0
0`
- **P0.4_critic_weights_http**: `200`
- **P0.5_composio**: `analytics.json
config.json
0.4.3-beta.411`
- **P0.6_jcode_head**: `8eb7c477d6b6`

Interpretation per the contract: P0.1 blocks Phase 3 if either route is 404; P0.4 gates Phase 2, with the :8079 fallback (measured ~14s per verdict) accepted if the weights are absent; P0.3 and P0.5 are reports only.
