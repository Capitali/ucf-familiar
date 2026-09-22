//! `ucf-familiar` — the UCF ship's computer, from the captain's side.
//!
//! This is the familiar's CLI shell narrowed to the three things a fleet actually needs:
//! the ships and the pilots that fly them (`fleet`), the autonomy dial and the approvals
//! queue (`autonomy`), and the commissioning ceremony that gives a ship a store, a key and
//! a lease (`world`). There is nothing else — no background service, no ledger, no
//! record export — because none of it is part of flying a hull.
//!
//! Argument parsing is hand-rolled and dependency-free on purpose: a small, legible trust
//! surface is part of the promise that this cannot be turned against the people it serves.

// The feed's ship row is one `json!` literal with many fields; the macro's
// expansion outgrows the default limit.
#![recursion_limit = "256"]

mod autonomy_cmd;
mod economy;
mod fleet;
mod fleet_serve;

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use ucf_persona::boundary;
use ucf_persona::store;

const USAGE: &str = "\
ucf-familiar — a ship's computer for United Cat Foods

usage:
  ucf-familiar <command> [options]

commands:
  fleet          the captain's ships and the pilots that fly them:
                 `fleet pair --label <name> --captain <who> --server <url> --key <k>`
                 (a ship world of its own: own store, own key, every gate shut)
                 | `fleet status` (the default) | `fleet captains [--adopt]`
                 | `fleet economy [--window 24h|7d|30d]`
                 | `fleet run [--renew] [--once]` (keep one pilot alive per paired ship)
                 | `fleet unpair <world>` | `fleet rename <world> <computer name>`
                 | `fleet hull <world> <ship name>` | `fleet choose <world>`
                 | `fleet order <world> <verb> …` | `fleet orders <world>`
                 | `fleet names` | `fleet adopt-ids`
                 | `fleet serve [--bind <addr>]` (the companion app's bridge feed)
  autonomy       how much the pilot may do without asking (the dial):
                 `autonomy show <ship>` | `autonomy set <ship> <surface>=<level> …`
                 | `autonomy advice <ship> [--all]` | `autonomy approve <ship> <id>`
                 | `autonomy deny <ship> <id>`
  world          ship worlds (a world IS a store): `world list` |
                 `world commission --label <name> [--store-root DIR]` (the ceremony:
                 own store, own key, every gate shut) | `world lease <id> [--ttl-hours N]`
                 (sign an expiring projection of the CURRENT boundary into the ship store)
                 | `world rename <id> <label>` | `world decommission <id>`

options:
  --data-dir <dir>   the fleet's data directory (default: familiar_data)
  --store-root <dir> where ship stores live (default: `worlds/` beside the data dir)

the boundary is the human's lever: a ship is born with every gate shut, and the only
authority it ever holds is the signed, expiring lease `world lease` writes into its store.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: &[String] = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("fleet") => fleet::cmd_fleet(rest),
        Some("autonomy") => autonomy_cmd::cmd_autonomy(rest),
        Some("world") => cmd_world(rest),
        Some(cmd) => {
            eprintln!("ucf-familiar: unknown command '{cmd}'\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Parse `--key value` and `--key=value` flags into a map. Bare trailing `--key`
/// maps to an empty string.
pub(crate) fn flags(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(key) = args[i].strip_prefix("--") {
            if let Some((k, v)) = key.split_once('=') {
                m.insert(k.to_string(), v.to_string());
            } else if let Some(v) = args.get(i + 1).filter(|v| !v.starts_with("--")) {
                // a following token that is itself a flag is NOT this flag's value,
                // so bare booleans like `--affects-person` parse correctly
                m.insert(key.to_string(), v.clone());
                i += 1;
            } else {
                m.insert(key.to_string(), String::new());
            }
        }
        i += 1;
    }
    m
}

// ---------------------------------------------------------------------------
// world — ship worlds: the commissioning ceremony as a command
// ---------------------------------------------------------------------------

/// Where ship stores live by default: a `worlds/` directory BESIDE the fleet's data
/// dir, never inside it — a scan of the fleet's own store must traverse a store that
/// simply contains no ship data, and nesting would put ship files on that walk.
pub(crate) fn world_store_root(dir: &std::path::Path, flag: Option<&str>) -> std::path::PathBuf {
    match flag {
        Some(p) => std::path::PathBuf::from(p),
        None => dir
            .parent()
            .map(|p| p.join("worlds"))
            .unwrap_or_else(|| dir.join("..").join("worlds")),
    }
}

fn cmd_world(args: &[String]) -> ExitCode {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let f = flags(args);
    let dir = store::data_dir(f.get("data-dir").map(String::as_str));
    let positional: Vec<&String> = {
        let mut out = Vec::new();
        let mut skip_next = false;
        for a in args.iter().skip(1) {
            if skip_next {
                skip_next = false;
                continue;
            }
            if let Some(key) = a.strip_prefix("--") {
                skip_next = !key.contains('=');
                continue;
            }
            out.push(a);
        }
        out
    };

    match sub {
        "list" => match ucf_world::instance::load(&dir) {
            Err(e) => {
                eprintln!("world: {e}");
                ExitCode::FAILURE
            }
            Ok(all) if all.is_empty() => {
                println!(
                    "world: none commissioned. A ship world begins by a human's word — \
                     `ucf-familiar world commission --label <name>`."
                );
                ExitCode::SUCCESS
            }
            Ok(all) => {
                for w in all {
                    println!(
                        "{} — \"{}\" ({:?}, epoch {}) commissioned by {} — key {}",
                        w.id,
                        w.label,
                        w.lifecycle,
                        w.grant_epoch,
                        w.commissioner,
                        &w.instance_pubkey[..16.min(w.instance_pubkey.len())]
                    );
                }
                ExitCode::SUCCESS
            }
        },
        "commission" => {
            let Some(label) = f.get("label") else {
                eprintln!("world commission: --label is required (the ship's human-given name)");
                return ExitCode::FAILURE;
            };
            let commissioner = f
                .get("commissioner")
                .cloned()
                .or_else(|| ucf_persona::identity::current(&dir))
                .unwrap_or_default();
            if commissioner.is_empty() || commissioner == "observer" {
                eprintln!(
                    "world commission: no established commissioner — pass --commissioner <human>; \
                     a world begins by a HUMAN's word, not an observer's"
                );
                return ExitCode::FAILURE;
            }
            let root = world_store_root(&dir, f.get("store-root").map(String::as_str));
            let endpoint = f.get("endpoint").map(String::as_str).unwrap_or("");
            match ucf_world::instance::commission(
                &dir,
                &root,
                label,
                &commissioner,
                endpoint,
                now_secs(),
            ) {
                Err(e) => {
                    eprintln!("world commission: {e}");
                    ExitCode::FAILURE
                }
                Ok((w, ship_dir)) => {
                    // The ship must be able to verify future leases without ever reading
                    // the fleet's store: give it the ISSUER's public identity now, as
                    // part of the ceremony. Public key only — no other data crosses.
                    let issuer_written = ucf_node::NodeKey::load_or_mint(&dir, "")
                        .map_err(|e| e.to_string())
                        .and_then(|k| {
                            serde_json::to_vec_pretty(&k.identity()).map_err(|e| e.to_string())
                        })
                        .and_then(|bytes| {
                            std::fs::write(ship_dir.join("issuer.json"), bytes)
                                .map_err(|e| e.to_string())
                        });
                    println!("commissioned {} — \"{}\"", w.id, w.label);
                    println!("  store: {}", ship_dir.display());
                    println!("  instance key: {}", w.instance_pubkey);
                    println!("  boundary: every gate shut (authority arrives only as a lease)");
                    match issuer_written {
                        Ok(()) => println!("  issuer.json written — the ship can verify leases"),
                        Err(e) => eprintln!("  WARNING: issuer.json not written ({e}) — leases cannot verify until it is"),
                    }
                    ExitCode::SUCCESS
                }
            }
        }
        "lease" => {
            let Some(id) = positional.first() else {
                eprintln!("world lease: which instance? `ucf-familiar world lease <world-id>`");
                return ExitCode::FAILURE;
            };
            let w = match ucf_world::instance::find(&dir, id) {
                Err(e) => {
                    eprintln!("world lease: {e}");
                    return ExitCode::FAILURE;
                }
                Ok(None) => {
                    eprintln!("world lease: no instance {id}");
                    return ExitCode::FAILURE;
                }
                Ok(Some(w)) => w,
            };
            if w.lifecycle == ucf_world::instance::Lifecycle::Decommissioned {
                eprintln!("world lease: {id} is decommissioned — its authority has ended");
                return ExitCode::FAILURE;
            }
            let ttl_hours: i64 = f
                .get("ttl-hours")
                .and_then(|s| s.parse().ok())
                .unwrap_or(24);
            let root_boundary = match boundary::load(&dir) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("world lease: cannot read the root boundary: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let key = match ucf_node::NodeKey::load_or_mint(&dir, "") {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("world lease: issuer key: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let signed = match ucf_world::lease::issue(
                &root_boundary,
                id,
                ttl_hours.saturating_mul(3600),
                now_secs(),
                &key,
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("world lease: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let root = world_store_root(&dir, f.get("store-root").map(String::as_str));
            let ship_dir = root.join(id.as_str());
            let bytes = match serde_json::to_vec_pretty(&signed) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("world lease: {e}");
                    return ExitCode::FAILURE;
                }
            };
            if ship_dir.is_dir() {
                if let Err(e) = std::fs::write(ship_dir.join("lease.json"), &bytes) {
                    eprintln!("world lease: write: {e}");
                    return ExitCode::FAILURE;
                }
                println!(
                    "leased {} for {}h — projection of the CURRENT boundary, written to {}",
                    id,
                    ttl_hours,
                    ship_dir.join("lease.json").display()
                );
            } else {
                // No store here (it may live on another machine): print, caller carries it.
                println!("{}", String::from_utf8_lossy(&bytes));
            }
            ExitCode::SUCCESS
        }
        "rename" => {
            let (Some(id), Some(label)) = (positional.first(), positional.get(1)) else {
                eprintln!("world rename: `ucf-familiar world rename <world-id> <new label>`");
                return ExitCode::FAILURE;
            };
            match ucf_world::instance::rename(&dir, id, label) {
                Err(e) => {
                    eprintln!("world rename: {e}");
                    ExitCode::FAILURE
                }
                Ok(w) => {
                    println!("renamed {} — \"{}\"", w.id, w.label);
                    ExitCode::SUCCESS
                }
            }
        }
        "decommission" => {
            let Some(id) = positional.first() else {
                eprintln!("world decommission: `ucf-familiar world decommission <world-id>`");
                return ExitCode::FAILURE;
            };
            match ucf_world::instance::decommission(&dir, id) {
                Err(e) => {
                    eprintln!("world decommission: {e}");
                    ExitCode::FAILURE
                }
                Ok(w) => {
                    println!(
                        "decommissioned {} — authority ended, epoch now {}. The store is \
                         untouched: archive or removal is a separate human retention act.",
                        w.id, w.grant_epoch
                    );
                    ExitCode::SUCCESS
                }
            }
        }
        other => {
            eprintln!(
                "world: unknown subcommand `{other}` — list | commission | lease | rename | decommission"
            );
            ExitCode::FAILURE
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
