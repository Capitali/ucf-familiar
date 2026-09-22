//! **ucf-persona** — who the ship's computer is, and the two or three plain-file
//! primitives the fleet reads around it.
//!
//! The persona seam arrived whole ([`persona`]): a ship's computer is named by a human,
//! chooses its own gender at every naming, and wears that voice out of its own
//! `persona.json`.
//!
//! Around it sits deliberately the *minimum* the fleet CLI and `ucf_world` actually call:
//!
//! - [`store`] — `data_dir`, the one-line resolution of `--data-dir`. No database: this
//!   workspace's whole store is plain files.
//! - [`identity`] — `current`, the handle of whoever is present, read from `observer.txt`.
//!   Used to default a commissioner so a world still begins by a human's word.
//! - [`boundary`] — the human-owned capability policy. It is here rather than in its own
//!   crate because the signed lease (`ucf_world::lease`) carries a `Boundary` as its
//!   payload, so the type is part of an on-disk, *signed* format and must stay byte-exact.
//!
//! **Modules, not a flat namespace.** Callers say `ucf_persona::persona::load`,
//! `ucf_persona::boundary::Boundary`, `ucf_persona::store::data_dir`. The paths are longer
//! than a flattened re-export would be, and that is the point: four unrelated concerns
//! share this crate for packaging reasons, and a reader who sees `boundary::load` should
//! not have to wonder whether the boundary is somehow part of the persona. It is not.

pub mod boundary;
pub mod identity;
pub mod persona;
pub mod store;
