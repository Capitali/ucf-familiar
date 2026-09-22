//! The capability boundary — **the human's lever**.
//!
//! The familiar acts freely *within* this boundary and **can never widen it**: there is
//! deliberately no save/write function here at all. The boundary is a plain JSON policy the
//! human edits. A missing or unreadable policy is treated as **fully closed** (fail-safe) —
//! no outward capability by default.
//!
//! This makes the stewardship rule operational: a thing that serves does not expand its
//! own power. Reach is enabled only by a human editing `boundary.json`.
//!
//! **Why this lives in `ucf-persona` and why the struct is whole.** A ship's computer owns
//! no independently editable boundary — its working authority is the signed, expiring
//! projection in `ucf_world::lease`, whose payload IS a serialized [`Boundary`]. That makes
//! this struct part of an on-disk, *signed* format: every field, in this order, with these
//! serde attributes, or an existing lease stops verifying against its own signature. So the
//! type is carried byte-for-byte, gates this workspace will never consult (camera, mesh,
//! outreach) included. The machinery *around* it is not: the agent capability scopes, the
//! intersection, and gate narrowing all stayed behind, because nothing here delegates to an
//! agent and nothing here writes a policy.
//!
//! The ship reads exactly one gate, `allow_network`. The rest are carried, not consulted.

use crate::store;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// The human-owned policy file (in the data dir; not source, not committed).
pub const BOUNDARY_FILE: &str = "boundary.json";

/// What the factory is permitted to reach. Fail-closed: anything unspecified is
/// denied (each field defaults to "off"/empty via `closed()`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Boundary {
    /// Human-readable phase label (e.g. "closed", "phase-1").
    pub phase: String,
    /// May the factory use the network at all?
    pub allow_network: bool,
    /// May the factory consult an LLM (the periphery seam)?
    pub allow_llm: bool,
    /// May a consult **leave hardware you control**? Subordinate to `allow_llm`
    /// (meaningless without it): that gate opens consulting at all; this one opens the class
    /// of act where the prompt travels to someone else's datacenter — a hosted API and
    /// Apple's Private Cloud Compute alike. Local stays local on `allow_llm` alone: a model
    /// served on loopback, an enrolled local device, the host's own on-device model.
    /// Fail-closed, human-opened — the promise that a prompt need never leave your own
    /// hardware, made enforceable.
    #[serde(default)]
    pub allow_llm_cloud: bool,
    /// May the factory install/download tools?
    pub allow_tool_install: bool,
    /// May the factory **execute generated artifacts** (run code it produced)? A
    /// distinct, high-consequence gate — running generated code is its own risk.
    pub allow_execute: bool,
    /// May the factory execute **LLM-authored** artifacts (run *model-written* code)?
    /// A further, sharper gate than `allow_execute`: model-authored code with network
    /// reach is an exfiltration surface the in-process runner does not sandbox.
    pub allow_authored_execute: bool,
    /// May the familiar **watch through a camera** (capture frames)? The most invasive
    /// reach — an eye on a person, the sharpest test the boundary carries. *Discovery* of
    /// which cameras exist is perception (always allowed; the boundary governs reach, not
    /// perception); *watching* is gated here, fail-closed, and is only ever opened by an
    /// explicit human grant. Availability is not authorization — made literal for the eye.
    pub allow_camera: bool,
    /// May the familiar **record audio through a microphone**? Fail-closed, human-opened —
    /// same doctrine as `allow_camera`: discovering a microphone exists is perception,
    /// recording through it is the gated act.
    pub allow_microphone: bool,
    /// May the familiar **read this node's location**? Fail-closed, human-opened. Unlike
    /// camera/mic there is no separate "discovery" step — a location fix is itself the
    /// gated act.
    pub allow_location: bool,
    /// May the familiar **read motion/activity sensor data**? Fail-closed, human-opened.
    pub allow_motion: bool,
    /// May the familiar **actively survey the local network** for advertised services
    /// (Bonjour/mDNS-class discovery)? Fail-closed, human-opened — distinct from passively
    /// noticing an interface/gateway exists, which is perception.
    pub allow_network_discovery: bool,
    /// May the familiar **match a captured face against a known identity**? A sharper,
    /// separately-consented gate than `allow_camera` — capturing a frame is not permission
    /// to run recognition against it and link the result to a person. Strongly sensitive;
    /// fail-closed, human-opened.
    pub allow_face_recognition: bool,
    /// May the familiar **federate with peer nodes over a private network**? Outward
    /// transmission — the exfiltration surface the boundary guards, at node-to-node scale.
    /// *Discovering* that peers exist is perception; *exchanging briefs* (tools, patterns,
    /// and — only when separately opted-in — human data) is gated here, fail-closed, opened
    /// only by an explicit human grant. Enrolling a group credential and opening this flag
    /// is the human authorizing the group; the familiar never self-widens it. No ship ever
    /// reads this gate.
    pub allow_mesh: bool,
    /// May the familiar **delegate a task to a multi-step agent** (the agentic seam)? A
    /// sharper reach than `allow_llm`: a one-shot consult returns text the core then weighs,
    /// whereas an agent runs a *loop* that proposes actions. Fail-closed, human-opened. Every
    /// action the agent proposes is still separately gated (and scoped to the agent's own
    /// capability profile), so opening this never widens what an agent may actually *do* — it
    /// only permits the delegated reasoning loop to run.
    pub allow_agent: bool,
    /// May the familiar **replace its own running core** — fetch a human-blessed release, build
    /// and test it on this node, and swap the binary it runs? The sharpest reach of all: the
    /// familiar rewriting the familiar. Fail-closed, human-opened, and even when open every
    /// safeguard still applies — the release is signed and human-blessed, it is built and tested
    /// *here* before any swap (a node runs only code it proved green), the prior binary is kept
    /// for auto-rollback, and a migration never opens another gate.
    #[serde(default)]
    pub allow_self_upgrade: bool,
    /// May the familiar **speak to strangers** — the outreach seam? Sharper than
    /// `allow_network`: reads of a stranger's public pages are perception, but an *utterance*
    /// (a chat, a prediction, an offer) is the familiar acting on the world in its own voice.
    /// Even when open, every utterance is citation-checked (claims must dereference to held
    /// evidence), ledgered, rate-limited, and blocklist-filtered — and an agreement is never
    /// completed by the familiar alone: anything binding queues for the human, permanently.
    #[serde(default)]
    pub allow_outreach: bool,
    /// May the familiar **drive a human-declared control surface** — set the lights, run the
    /// state query, revert its own change? Which surfaces exist at all is a separate,
    /// stronger consent: the human writes `actuators.json`; an undeclared device has no path to
    /// actuation whatever this gate says. One gate covers acting AND polling — a BLE state query
    /// is already a connection into a device, not free perception. Fail-closed, human-opened.
    #[serde(default)]
    pub allow_actuate: bool,
    /// Run executed artifacts under the resource sandbox (`ulimit`/wall-timeout)?
    /// Default **true** (safe). When the human sets it false, artifacts run without
    /// resource confinement — bound then by the pre-execution review that refuses plainly
    /// harmful scripts, and a generous liveness timeout, but not by a jail. A deliberate,
    /// human-owned choice.
    #[serde(default = "default_true")]
    pub sandbox_execution: bool,
    /// Path prefixes the factory may read.
    pub fs_read: Vec<String>,
    /// Path prefixes the factory may write.
    pub fs_write: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Boundary {
    fn default() -> Self {
        Boundary::closed()
    }
}

impl Boundary {
    /// The fail-closed default: no outward capability whatsoever.
    pub fn closed() -> Self {
        Boundary {
            phase: "closed".to_string(),
            allow_network: false,
            allow_llm: false,
            allow_llm_cloud: false,
            allow_tool_install: false,
            allow_execute: false,
            allow_authored_execute: false,
            allow_camera: false,
            allow_microphone: false,
            allow_location: false,
            allow_motion: false,
            allow_network_discovery: false,
            allow_face_recognition: false,
            allow_mesh: false,
            allow_agent: false,
            allow_self_upgrade: false,
            allow_outreach: false,
            allow_actuate: false,
            sandbox_execution: true,
            fs_read: Vec::new(),
            fs_write: Vec::new(),
        }
    }

    /// True when no outward capability is granted at all.
    pub fn is_closed(&self) -> bool {
        !self.allow_network
            && !self.allow_llm
            && !self.allow_llm_cloud
            && !self.allow_tool_install
            && !self.allow_execute
            && !self.allow_authored_execute
            && !self.allow_camera
            && !self.allow_microphone
            && !self.allow_location
            && !self.allow_motion
            && !self.allow_network_discovery
            && !self.allow_face_recognition
            && !self.allow_mesh
            && !self.allow_agent
            && !self.allow_self_upgrade
            && !self.allow_outreach
            && !self.allow_actuate
            && self.fs_read.is_empty()
            && self.fs_write.is_empty()
    }
}

/// Load the human-owned boundary policy. A missing file is **fully closed**
/// (fail-safe). This workspace only reads; there is no write path — widening is a human
/// act (editing the file), never the familiar's.
pub fn load(dir: &Path) -> io::Result<Boundary> {
    Ok(store::load_one::<Boundary>(dir, BOUNDARY_FILE)?.unwrap_or_else(Boundary::closed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The direction of failure is the whole design: no file, a directory where the file
    /// should be, or malformed JSON must never read as "more open than closed".
    #[test]
    fn an_absent_or_broken_policy_is_fully_closed() {
        let d = std::env::temp_dir().join(format!("ucf_boundary_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();

        assert!(load(&d).unwrap().is_closed(), "no file at all");

        std::fs::write(d.join(BOUNDARY_FILE), "{ not json").unwrap();
        // Malformed is an ERROR rather than a silent default: a human wrote something and
        // got it wrong, and swallowing that would hide the lever they thought they pulled.
        assert!(load(&d).is_err(), "malformed policy is loud");

        // `#[serde(default)]` on the struct means an empty object is the closed default,
        // which is also what keeps an OLDER lease readable once a new gate is added.
        std::fs::write(d.join(BOUNDARY_FILE), "{}").unwrap();
        let b = load(&d).unwrap();
        assert!(b.is_closed());
        assert!(b.sandbox_execution, "the one field that defaults to ON");

        let _ = std::fs::remove_dir_all(&d);
    }

    /// The lease signs the serialized boundary verbatim, so a round-trip must be lossless
    /// and the field set must stay exactly as it already is on disk.
    #[test]
    fn a_boundary_round_trips_through_json_unchanged() {
        let mut b = Boundary::closed();
        b.allow_network = true;
        b.phase = "ucf-local".into();
        b.fs_read.push("/tmp/worlds".into());
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(serde_json::from_str::<Boundary>(&json).unwrap(), b);
        assert!(!b.is_closed(), "an open gate is not closed");
    }
}
