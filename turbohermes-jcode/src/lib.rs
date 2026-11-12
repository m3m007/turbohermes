//! turbohermes-jcode: the jcode side of the TurboHermes adapter.
//!
//! Work order v7 section 9, Phase 3 "jcode integration". The architecture
//! document, section 3: "It delegates coding execution to jcode through a narrow
//! adapter: start, resume, inspect and stop sessions; create a swarm from a
//! task-DAG node; pass scoped task specification, repository path, branch or
//! worktree, budget and permitted tools; inject context; collect typed
//! artifacts; map jcode identities to TurboHermes identities; reconcile after
//! process failure."
//!
//! # The narrowest stable interface (verified live 2026-11-10)
//!
//! Not `jcode run`, a CLI wrapper that streams text and exits. The stable
//! interface is the **harness API bridge**: `jcode api-bridge --api-socket
//! <path>` serves NDJSON frames on a Unix socket with a versioned handshake.
//! The adapter owns the bridge's lifecycle: it starts the bridge, waits for the
//! socket, and reaps the child.
//!
//! # Module map and where each one comes from
//!
//! | Module | Provenance | Upstream file |
//! |---|---|---|
//! | [`frames`] | port | `jcode/crates/jcode-harness-api/src/client.rs` |
//! | [`transport`] | port + glue | `jcode/crates/jcode-harness-api/src/{client,sockets}.rs` |
//! | [`client`] | glue over [`transport`] | `jcode/crates/jcode-harness-api/src/client.rs` |
//! | [`spec`] | port-adapt | `blockcell/crates/core/src/{task_contract,budget}.rs` |
//! | [`scope`] | port | `jcode/crates/jcode-sdk/src/worktrees.rs` |
//! | [`events`] | glue | `turbohermes-core` event schema |
//! | [`artifacts`] | glue | `turbohermes-core` hash + hygiene gate |
//! | [`reconcile`] | port-adapt | `beads/internal/storage/issueops/{claim,lease}.go` |
//!
//! Every row above is expanded, with the pinned commit and licence, in
//! `../../REUSE_MAP.md`.
//!
//! # What this crate does not do
//!
//! It does not depend on jcode as a library: jcode is an external process and
//! this crate speaks its wire protocol. It makes no network calls and no LLM
//! calls: the model runs inside the jcode daemon the adapter supervises. It
//! never stores text: tool inputs and outputs are carried as SHA-256 digests by
//! [`turbohermes_core`], and the artifact envelope carries references, not
//! content.

pub mod artifacts;
pub mod client;
pub mod events;
pub mod frames;
pub mod reconcile;
pub mod scope;
pub mod spec;
pub mod transport;

pub use artifacts::{Artifact, ArtifactError, ArtifactManifest};
pub use client::{JcodeClient, JcodeClientError};
pub use events::{EventCapture, EventCaptureError, Identity};
pub use frames::{ClientFrame, FrameError, ServerFrame, read_frame, write_frame};
pub use reconcile::{
    Claim, ClaimLedger, ClaimLedgerError, ClaimRefusal, Probe, ProbeResult, Reconciliation, Verdict,
};
pub use scope::{Scope, ScopeError, Worktree};
pub use spec::{AcceptanceCriterion, AllowedOps, Budget, SpecError, TaskSpec};
pub use transport::{Bridge, BridgeError, SocketPath, Transport, TransportError};

/// The version of the harness API this crate speaks.
///
/// Copied from `jcode-harness-api/src/lib.rs` so the handshake refuses a
/// protocol this crate does not implement rather than failing mid-turn.
pub const API_VERSION_MAJOR: u32 = 1;

/// This crate's client identity in the `hello` frame.
pub const CLIENT_NAME: &str = "turbohermes-jcode/0.1.0";
