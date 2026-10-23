//! Close fleet operations — the captain walked from hull to hull to keep every drive
//! repaired (Ian, 2026-10-22: "Get me aboard each of the ships in turn. Repair. Keep
//! that rotation going … Make it an option. 'Close fleet operations'").
//!
//! A co-pilot's papers cannot repair, so a hull the captain is not aboard wears and
//! stays worn: KBC-03 and Kibble Klipper both sat at 100% wear, half drive, on
//! 2026-10-22. A ship change (metal#100) is filed from the hull the captain leaves,
//! with both hulls berthed at one station. So with the option on, the supervisor
//! brings a worn hull to the berth the captain's hull is next at, holds whichever
//! arrives first, boards the captain across, files the repair once the pilot flies
//! on the captain's papers, and releases the holds. The captain then stays aboard
//! until another hull wants them: that is the rotation.
//!
//! [`plan`] is pure, over a [`Hull`] view the supervisor reads from the exchange;
//! [`pass`] reads, plans and files through the same order path the iPad uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::fleet::Ship;

/// Worn enough to fetch the captain for: the doctrine's own titled-hull threshold
/// (half worn is a quarter of the drive gone).
pub(crate) const WORN_AT_BPS: i64 = ucf_pilot::doctrine::REPAIR_TITLED_AT_BPS;
/// Beside the captain's papers in their record directory.
pub(crate) const FILE: &str = "close-ops.json";
/// Who the orders say gave them.
pub(crate) const BY: &str = "close fleet operations";

/// How long a done board or repair still counts as in flight: two ticks at 180 s.
const SETTLE_SECS: i64 = 360;

/// The option and the meeting under way, per captain.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CloseOps {
    #[serde(default)]
    pub on: bool,
    /// The worn hull (world id) the captain is being brought to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Where the two hulls meet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station: Option<String>,
    /// Hulls this meeting put on a hold; released when it ends, and only these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<String>,
    #[serde(default)]
    pub since: i64,
}

pub(crate) fn path(captain_dir: &Path) -> PathBuf {
    captain_dir.join(FILE)
}

pub(crate) fn load(captain_dir: &Path) -> CloseOps {
    std::fs::read_to_string(path(captain_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub(crate) fn save(captain_dir: &Path, c: &CloseOps) -> std::io::Result<()> {
    std::fs::create_dir_all(captain_dir)?;
    let tmp = captain_dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(c).unwrap_or_default())?;
    std::fs::rename(tmp, path(captain_dir))
}

/// One hull as the coordinator sees it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Hull {
    pub world: String,
    /// The register says the captain stands on this hull.
    pub aboard: bool,
    /// Its pilot already flies on the captain's papers (the journal's `papers`).
    pub captain_papers: bool,
    pub wear_bps: i64,
    pub docked: Option<String>,
    pub bound_for: Option<String>,
    /// Holds contracts: its time is spoken for, so it is not sent anywhere.
    pub carrying: bool,
    /// Verbs of its pending orders.
    pub pending: Vec<String>,
}

impl Hull {
    /// The berth this hull will next stand at.
    fn next_berth(&self) -> Option<&str> {
        self.docked.as_deref().or(self.bound_for.as_deref())
    }
    fn worn(&self) -> bool {
        self.wear_bps >= WORN_AT_BPS
    }
    fn has(&self, verb: &str) -> bool {
        self.pending.iter().any(|v| v == verb)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    /// A meeting begins (or moves): `target` is fetched to `station`.
    Meet {
        target: String,
        station: String,
    },
    /// Hold `world` at `station` (a hold at a berth not under the hull flies there first).
    Hold {
        world: String,
        station: String,
    },
    /// The captain leaves `world` for `to`.
    Board {
        world: String,
        to: String,
    },
    Repair {
        world: String,
    },
    /// End a hold this meeting placed.
    Release {
        world: String,
    },
    /// The meeting is over.
    Done,
}

/// What to file this pass. Empty when nothing is due.
pub(crate) fn plan(state: &CloseOps, hulls: &[Hull]) -> Vec<Step> {
    if !state.on {
        return Vec::new();
    }
    let Some(cap) = hulls.iter().find(|h| h.aboard) else {
        return Vec::new();
    };
    // The hull the captain stands on repairs first: walking off a worn hull leaves it
    // worn, since only the captain's papers can repair (a boarding made by hand left
    // KBC-03 so, 2026-10-22).
    if cap.worn() && cap.captain_papers && cap.docked.is_some() && !cap.has("repair") {
        return vec![Step::Repair {
            world: cap.world.clone(),
        }];
    }
    let release_all = |mut steps: Vec<Step>| {
        steps.extend(
            state
                .held
                .iter()
                .map(|w| Step::Release { world: w.clone() }),
        );
        steps.push(Step::Done);
        steps
    };

    if let (Some(t), Some(st)) = (&state.target, &state.station) {
        let Some(w) = hulls.iter().find(|h| &h.world == t) else {
            return release_all(Vec::new());
        };
        // A hull held for this meeting that the captain has since left (and that is
        // not the one being fetched) flies on at once.
        let left: Vec<Step> = state
            .held
            .iter()
            .filter(|h| *h != t && hulls.iter().any(|x| &x.world == *h && !x.aboard))
            .map(|h| Step::Release { world: h.clone() })
            .collect();
        if !left.is_empty() {
            return left;
        }
        if w.aboard {
            // Boarded. Repair once the pilot flies the captain's papers (filed sooner,
            // a co-pilot key refuses it and the order waits for good), then let go.
            if !w.worn() {
                return release_all(Vec::new());
            }
            if w.captain_papers && w.docked.is_some() && !w.has("repair") {
                return vec![Step::Repair { world: t.clone() }];
            }
            return Vec::new();
        }
        if !w.worn() {
            // Repaired some other way (or never was): nothing to fetch the captain for.
            return release_all(Vec::new());
        }
        let cap_there = cap.docked.as_deref() == Some(st.as_str());
        let w_there = w.docked.as_deref() == Some(st.as_str());
        if cap_there && w_there {
            return if cap.has("board") {
                Vec::new()
            } else {
                vec![Step::Board {
                    world: cap.world.clone(),
                    to: t.clone(),
                }]
            };
        }
        let cap_coming = cap.next_berth() == Some(st.as_str());
        let w_coming = w_there || w.bound_for.as_deref() == Some(st.as_str());
        if cap_coming && w_coming && !state.held.contains(&cap.world) {
            // Both bound for the berth: the captain's hull waits there too, or its
            // pilot books a load and leaves in the fold it arrives. Not before the
            // worn hull is on its way — a hold while it idles elsewhere could cost
            // the captain's own deadlines a whole slow leg.
            return vec![Step::Hold {
                world: cap.world.clone(),
                station: st.clone(),
            }];
        }
        if !cap_coming {
            // The captain's hull went elsewhere. An idle worn hull follows it;
            // one with work aboard is let go to fly it.
            return match cap.next_berth() {
                Some(next) if !w.carrying => vec![
                    Step::Meet {
                        target: t.clone(),
                        station: next.to_string(),
                    },
                    Step::Hold {
                        world: t.clone(),
                        station: next.to_string(),
                    },
                ],
                _ => release_all(Vec::new()),
            };
        }
        return Vec::new();
    }

    // No meeting: the most worn hull that can meet the captain's next berth.
    let Some(meet) = cap.next_berth() else {
        return Vec::new();
    };
    let mut worn: Vec<&Hull> = hulls.iter().filter(|h| !h.aboard && h.worn()).collect();
    worn.sort_by(|a, b| b.wear_bps.cmp(&a.wear_bps).then(a.world.cmp(&b.world)));
    for w in worn {
        let same_berth = w.next_berth() == Some(meet);
        let idle = !w.carrying && w.bound_for.is_none() && w.docked.is_some();
        if same_berth || idle {
            return vec![
                Step::Meet {
                    target: w.world.clone(),
                    station: meet.to_string(),
                },
                Step::Hold {
                    world: w.world.clone(),
                    station: meet.to_string(),
                },
            ];
        }
    }
    Vec::new()
}

/// Read every hull of every captain with the option on, plan, and file. Called by
/// `fleet run` once a pass; quiet when no captain has the option on.
pub(crate) fn pass(root: &Path, ships: &[Ship]) {
    let mut by_captain: BTreeMap<String, Vec<&Ship>> = BTreeMap::new();
    for s in ships {
        if !s.captain.captain_id.trim().is_empty() {
            by_captain
                .entry(s.captain.captain_id.clone())
                .or_default()
                .push(s);
        }
    }
    for (captain_id, mine) in by_captain {
        let Some(cdir) = ucf_pilot::store::captain_dir(&mine[0].dir, &captain_id) else {
            continue;
        };
        let mut state = load(&cdir);
        if !state.on {
            continue;
        }
        let hulls = view(&mine);
        let steps = plan(&state, &hulls);
        if steps.is_empty() {
            continue;
        }
        let now = crate::now_secs();
        for step in steps {
            apply(root, ships, &mine, &mut state, &step, now);
        }
        if let Err(e) = save(&cdir, &state) {
            eprintln!("close fleet operations: {captain_id}: {e}");
        }
    }
}

fn apply(
    _root: &Path,
    fleet: &[Ship],
    mine: &[&Ship],
    state: &mut CloseOps,
    step: &Step,
    now: i64,
) {
    let ship = |w: &str| mine.iter().find(|s| s.world.id == w).copied();
    let name = |w: &str| {
        ship(w)
            .map(|s| s.captain.hull_name.clone())
            .unwrap_or_else(|| w.to_string())
    };
    let file = |w: &str, body: Value, why: String| {
        let Some(s) = ship(w) else { return };
        match crate::fleet_serve::place_order(s, &body, now, fleet) {
            Ok(o) => {
                println!(
                    "close fleet operations: {} — {why} ({})",
                    s.captain.hull_name, o.id
                );
                note(&s.dir, now, &why);
            }
            Err((_, e)) => {
                eprintln!(
                    "close fleet operations: {} — {why}: refused: {e}",
                    s.captain.hull_name
                );
                note(&s.dir, now, &format!("{why} — refused: {e}"));
            }
        }
    };
    match step {
        Step::Meet { target, station } => {
            state.target = Some(target.clone());
            state.station = Some(station.clone());
            state.since = now;
        }
        Step::Hold { world, station } => {
            file(
                world,
                json!({"verb": "hold", "station": station, "by": BY}),
                format!("hold at {station} to meet the captain"),
            );
            if !state.held.contains(world) {
                state.held.push(world.clone());
            }
        }
        Step::Board { world, to } => file(
            world,
            json!({"verb": "board", "ship": to, "when": "now", "by": BY}),
            format!("the captain boards {} to repair it", name(to)),
        ),
        Step::Repair { world } => file(
            world,
            json!({"verb": "repair", "when": "now", "by": BY}),
            "repair, with the captain aboard".into(),
        ),
        Step::Release { world } => {
            // Only a hold still standing is ended: "resume" ends whatever course stands.
            let holding = ship(world).is_some_and(|s| {
                ucf_pilot::store::load_orders(&s.dir)
                    .iter()
                    .any(|o| o.verb == "hold" && o.pending() && o.by == BY)
            });
            if holding {
                file(
                    world,
                    json!({"verb": "resume", "by": BY}),
                    "the meeting is over: fly on".into(),
                );
            }
            state.held.retain(|h| h != world);
        }
        Step::Done => {
            *state = CloseOps {
                on: state.on,
                ..Default::default()
            };
        }
    }
}

/// A line in the hull's own journal, so the feed and the iPad see why it waits.
fn note(ship_dir: &Path, now: i64, why: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ship_dir.join("journal.jsonl"))
    {
        let _ = writeln!(
            f,
            "{}",
            json!({"at": now, "event": "close-ops", "why": why})
        );
    }
}

/// The hulls of one captain, from the exchange: each through the papers its pilot
/// flies on, and the register for who stands where.
pub(crate) fn view(mine: &[&Ship]) -> Vec<Hull> {
    let now = crate::now_secs();
    let mut register: Option<Value> = None;
    let mut hulls = Vec::new();
    for s in mine {
        let (server, key) = crate::fleet::hull_papers(s);
        // Who stands where, through the captain's OWN key: a register read through a
        // co-pilot key says aboard of the hull that key flies, which put the captain
        // on KBC-03 while he stood on KBC-04 (2026-10-22).
        if register.is_none() {
            if let Some(cd) = ucf_pilot::store::captain_dir(&s.dir, &s.captain.captain_id) {
                register = ucf_pilot::store::load_papers(&cd)
                    .keys
                    .iter()
                    .filter(|p| p.is_captain())
                    .filter(|p| p.server.trim_end_matches('/') == server.trim_end_matches('/'))
                    .find_map(|p| {
                        crate::fleet::wire_get(&server, &p.secret, "/v1/captain")
                            .ok()
                            .filter(|v| v.get("fleet").is_some())
                    });
            }
        }
        let Ok(me) = crate::fleet::wire_get(&server, &key, "/v1/me") else {
            continue;
        };
        let text = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(String::from)
        };
        hulls.push(Hull {
            world: s.world.id.clone(),
            aboard: false,
            captain_papers: crate::fleet::flying_papers(&s.dir).as_deref() == Some("captain"),
            wear_bps: me.get("wearBps").and_then(Value::as_i64).unwrap_or(0),
            docked: text(&me, "docked"),
            bound_for: text(&me, "enRouteTo"),
            carrying: me
                .get("contracts")
                .and_then(Value::as_array)
                .is_some_and(|c| !c.is_empty()),
            // An order done but not yet settled still counts: the register and the
            // wear lag a tick behind, and a board re-filed in that gap was a second
            // boarding from a hull the captain had left (2026-10-22).
            pending: ucf_pilot::store::load_orders(&s.dir)
                .iter()
                .filter(|o| o.pending() || o.done_at.is_some_and(|d| now - d < SETTLE_SECS))
                .map(|o| o.verb.clone())
                .collect(),
        });
    }
    let aboard = register
        .as_ref()
        .and_then(|r| r.get("fleet")?.as_array().cloned())
        .unwrap_or_default();
    for h in &mut hulls {
        let actor = mine
            .iter()
            .find(|s| s.world.id == h.world)
            .map(|s| s.captain.hull_actor.clone())
            .unwrap_or_default();
        h.aboard = aboard.iter().any(|e| {
            e.get("actorId").and_then(Value::as_str) == Some(actor.as_str())
                && e.get("aboard").and_then(Value::as_bool) == Some(true)
        });
    }
    hulls
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull(world: &str, wear: i64) -> Hull {
        Hull {
            world: world.into(),
            wear_bps: wear,
            ..Default::default()
        }
    }
    fn on() -> CloseOps {
        CloseOps {
            on: true,
            ..Default::default()
        }
    }
    fn meeting(target: &str, station: &str, held: &[&str]) -> CloseOps {
        CloseOps {
            on: true,
            target: Some(target.into()),
            station: Some(station.into()),
            held: held.iter().map(|s| s.to_string()).collect(),
            since: 1,
        }
    }

    #[test]
    fn off_files_nothing_however_worn_the_fleet() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.docked = Some("x".into());
        let mut w = hull("w", 10_000);
        w.docked = Some("x".into());
        assert!(plan(&CloseOps::default(), &[cap, w]).is_empty());
    }

    /// 2026-10-22: KBC-03 (100% worn) and the captain's KBC-04 both bound for
    /// foxys-diner — the worn hull is held there to meet the captain.
    #[test]
    fn two_hulls_bound_for_one_berth_meet_there() {
        let mut cap = hull("c", 1_131);
        cap.aboard = true;
        cap.bound_for = Some("foxys-diner".into());
        let mut w = hull("w", 10_000);
        w.bound_for = Some("foxys-diner".into());
        w.carrying = true;
        assert_eq!(
            plan(&on(), &[cap, w]),
            vec![
                Step::Meet {
                    target: "w".into(),
                    station: "foxys-diner".into()
                },
                Step::Hold {
                    world: "w".into(),
                    station: "foxys-diner".into()
                },
            ]
        );
    }

    /// Kibble Klipper idle at paws-truckstop: an idle worn hull flies to the captain.
    #[test]
    fn an_idle_worn_hull_is_sent_to_the_captains_next_berth() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.bound_for = Some("foxys-diner".into());
        let mut w = hull("w", 9_000);
        w.docked = Some("paws-truckstop".into());
        let steps = plan(&on(), &[cap, w]);
        assert_eq!(
            steps[1],
            Step::Hold {
                world: "w".into(),
                station: "foxys-diner".into()
            }
        );
    }

    #[test]
    fn the_captain_is_held_only_once_the_worn_hull_is_on_its_way() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.bound_for = Some("x".into());
        let mut w = hull("w", 10_000);
        w.docked = Some("y".into());
        let state = meeting("w", "x", &["w"]);
        assert!(plan(&state, &[cap.clone(), w.clone()]).is_empty());
        w.docked = None;
        w.bound_for = Some("x".into());
        assert_eq!(
            plan(&state, &[cap, w]),
            vec![Step::Hold {
                world: "c".into(),
                station: "x".into()
            }]
        );
    }

    #[test]
    fn a_worn_hull_with_work_elsewhere_is_left_to_fly_it() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.bound_for = Some("foxys-diner".into());
        let mut w = hull("w", 9_000);
        w.bound_for = Some("cannery-row".into());
        w.carrying = true;
        assert!(plan(&on(), &[cap, w]).is_empty());
    }

    #[test]
    fn the_most_worn_hull_goes_first() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.docked = Some("x".into());
        let mut a = hull("a", 6_000);
        a.docked = Some("x".into());
        let mut b = hull("b", 10_000);
        b.docked = Some("x".into());
        assert_eq!(
            plan(&on(), &[cap, a, b])[0],
            Step::Meet {
                target: "b".into(),
                station: "x".into()
            }
        );
    }

    #[test]
    fn the_captain_arriving_first_waits_then_boards() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.docked = Some("x".into());
        let mut w = hull("w", 10_000);
        w.bound_for = Some("x".into());
        let state = meeting("w", "x", &["w"]);
        assert_eq!(
            plan(&state, &[cap.clone(), w.clone()]),
            vec![Step::Hold {
                world: "c".into(),
                station: "x".into()
            }]
        );
        // Both berthed: the captain boards.
        w.docked = Some("x".into());
        w.bound_for = None;
        let state = meeting("w", "x", &["w", "c"]);
        assert_eq!(
            plan(&state, &[cap.clone(), w.clone()]),
            vec![Step::Board {
                world: "c".into(),
                to: "w".into()
            }]
        );
        // Filed once: the pending board is not filed again.
        cap.pending = vec!["board".into()];
        assert!(plan(&state, &[cap, w]).is_empty());
    }

    #[test]
    fn boarded_it_repairs_only_once_the_pilot_flies_the_captains_papers() {
        let mut cap_left = hull("c", 0);
        cap_left.docked = Some("x".into());
        let mut w = hull("w", 10_000);
        w.aboard = true;
        w.docked = Some("x".into());
        // The captain's old hull, held for the meeting, flies on as soon as he leaves it.
        let state = meeting("w", "x", &["w", "c"]);
        assert_eq!(
            plan(&state, &[cap_left.clone(), w.clone()]),
            vec![Step::Release { world: "c".into() }]
        );
        let state = meeting("w", "x", &["w"]);
        assert!(plan(&state, &[cap_left.clone(), w.clone()]).is_empty());
        w.captain_papers = true;
        assert_eq!(
            plan(&state, &[cap_left.clone(), w.clone()]),
            vec![Step::Repair { world: "w".into() }]
        );
        w.pending = vec!["repair".into()];
        assert!(plan(&state, &[cap_left.clone(), w.clone()]).is_empty());
        // Repaired: every hold this meeting placed is released, and it ends.
        w.wear_bps = 0;
        w.pending.clear();
        assert_eq!(
            plan(&state, &[cap_left, w]),
            vec![Step::Release { world: "w".into() }, Step::Done]
        );
    }

    #[test]
    fn the_captain_moving_on_takes_an_idle_hull_along_and_lets_a_busy_one_go() {
        let mut cap = hull("c", 0);
        cap.aboard = true;
        cap.bound_for = Some("y".into());
        let mut w = hull("w", 10_000);
        w.bound_for = Some("x".into());
        let state = meeting("w", "x", &["w"]);
        assert_eq!(
            plan(&state, &[cap.clone(), w.clone()]),
            vec![
                Step::Meet {
                    target: "w".into(),
                    station: "y".into()
                },
                Step::Hold {
                    world: "w".into(),
                    station: "y".into()
                },
            ]
        );
        w.carrying = true;
        assert_eq!(
            plan(&state, &[cap, w]),
            vec![Step::Release { world: "w".into() }, Step::Done]
        );
    }

    #[test]
    fn the_captains_own_hull_is_never_fetched() {
        let mut cap = hull("c", 10_000);
        cap.aboard = true;
        cap.bound_for = Some("x".into());
        assert!(plan(&on(), &[cap]).is_empty());
    }

    #[test]
    fn a_worn_hull_the_captain_stands_on_repairs_before_the_captain_moves_on() {
        let mut cap = hull("c", 10_000);
        cap.aboard = true;
        cap.docked = Some("x".into());
        let mut w = hull("w", 10_000);
        w.docked = Some("x".into());
        // Papers not yet flipped: nothing for the captain's hull, the meeting may begin.
        assert_eq!(
            plan(&on(), &[cap.clone(), w.clone()])[0],
            Step::Meet {
                target: "w".into(),
                station: "x".into()
            }
        );
        cap.captain_papers = true;
        assert_eq!(
            plan(&on(), &[cap.clone(), w.clone()]),
            vec![Step::Repair { world: "c".into() }]
        );
        cap.pending = vec!["repair".into()];
        assert_ne!(
            plan(&on(), &[cap, w])[0],
            Step::Repair { world: "c".into() }
        );
    }
}
