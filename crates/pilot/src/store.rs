//! The ship store's file I/O — every read and write the RUNNER makes on the pilot's
//! behalf, in one place — the rule being "one doctrine, two runtimes" (2026-09-05).
//!
//! The decision crates — [`crate::doctrine`], [`crate::trade`], [`crate::autonomy`],
//! [`crate::chain`], [`crate::outfit`] — own no file, socket or clock: facts in, a
//! decision out. That is what lets the same doctrine fly the hull from the host loop
//! and answer the captain from inside a companion app through an FFI shim, and later
//! sit behind a service somewhere else. Anything that touches the ship's data dir lives
//! here, and only the runner (`src/main.rs`) and the fleet commands call it.

use std::collections::BTreeSet;
use std::path::Path;

use crate::autonomy::{self, Approval, Dial, Proposal, DIAL_FILE};
use crate::trade::{self, Holding};
use crate::Automation;

/// The dial from `autonomy.json`; absent or unreadable is the default dial.
pub fn load_dial(ship_dir: &Path) -> Dial {
    std::fs::read_to_string(ship_dir.join(DIAL_FILE))
        .map(|t| Dial::parse(&t))
        .unwrap_or_default()
}

/// Write the dial to `autonomy.json`.
pub fn save_dial(ship_dir: &Path, dial: &Dial) -> std::io::Result<()> {
    std::fs::write(ship_dir.join(DIAL_FILE), dial.to_json())
}

pub fn load_proposals(ship_dir: &Path) -> Vec<Proposal> {
    std::fs::read_to_string(ship_dir.join("proposals.jsonl"))
        .map(|t| autonomy::parse_proposals(&t))
        .unwrap_or_default()
}

pub fn load_approvals(ship_dir: &Path) -> Vec<Approval> {
    std::fs::read_to_string(ship_dir.join("approvals.jsonl"))
        .map(|t| autonomy::parse_approvals(&t))
        .unwrap_or_default()
}

pub fn append_proposal(ship_dir: &Path, p: &Proposal) {
    append_jsonl(&ship_dir.join("proposals.jsonl"), p);
}

pub fn append_approval(ship_dir: &Path, a: &Approval) {
    append_jsonl(&ship_dir.join("approvals.jsonl"), a);
}

fn append_jsonl<T: serde::Serialize>(path: &Path, record: &T) {
    use std::io::Write;
    if let Ok(line) = serde_json::to_string(record) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// The speculative book from `holdings.json`; absent or unreadable is an empty book.
pub fn load_holdings(ship_dir: &Path) -> Vec<Holding> {
    std::fs::read_to_string(ship_dir.join("holdings.json"))
        .map(|t| trade::parse_holdings(&t))
        .unwrap_or_default()
}

/// Persist the book, zeroed-out positions dropped.
pub fn save_holdings(ship_dir: &Path, holdings: &[Holding]) {
    if let Some(bytes) = trade::holdings_json(holdings) {
        let _ = std::fs::write(ship_dir.join("holdings.json"), bytes);
    }
}

/// The automations this ship holds, read from `automations.json`. An absent file
/// grants nothing.
pub fn granted_automations(ship_dir: &Path) -> (BTreeSet<Automation>, Vec<String>) {
    match std::fs::read_to_string(ship_dir.join("automations.json")) {
        Ok(raw) => crate::parse_automations(&raw),
        Err(_) => (BTreeSet::new(), Vec::new()),
    }
}

/// Read `KEY=value` out of an env-format file — the same convention the MCP
/// declaration uses for its key files (mode 0600, never committed, never logged).
pub fn env_value(path: &Path, key: &str) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// The hull's co-pilot key, beside the captain's own `UCF_KEY` in `ucf.env`.
pub const COPILOT_KEY_VAR: &str = "UCF_COPILOT_KEY";

/// Every verb a co-pilot key cannot file. The exchange lets a co-pilot travel, book,
/// cancel a booking, collect, refuel and engage (`copilotVerbs`), and nothing else:
/// no trading, no yard, no crew, no lease, no tanker, and no ship change.
pub const COPILOT_DENIED: [&str; 10] = [
    "repair",
    "paws",
    "refit",
    "payLease",
    "expandFrame",
    "buy",
    "sell",
    "hire",
    "dismiss",
    "transferCaptain",
];

/// The human's papers: every key a captain holds on an exchange, filed on the
/// captain's record (`captains/<id>/papers.json`, 0600 like `ucf.env`).
///
/// A captain's OWN keys belong to the person, not to a hull: after a ship change
/// (metal#100) every one of them answers for the hull the captain stands on. A
/// co-pilot key belongs to its hull and stays with it. So which key flies a hull is
/// decided per fold, by asking — never by which directory a key was written into.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CaptainPapers {
    #[serde(default)]
    pub exchange_captain_id: String,
    #[serde(default)]
    pub keys: Vec<Paper>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Paper {
    pub server: String,
    /// The public id (first 8 hex of the secret), for display and revocation.
    pub key_id: String,
    pub secret: String,
    /// `captain` (`act` scope) or `copilot` (`auto:freight`).
    pub kind: String,
    /// The hull the key was issued for — a co-pilot's for good, a captain key's
    /// only until the captain changes ship.
    #[serde(default)]
    pub hull_actor: String,
    #[serde(default)]
    pub hull: String,
    #[serde(default)]
    pub filed_at: i64,
}

impl Paper {
    pub fn is_captain(&self) -> bool {
        self.kind == "captain"
    }
}

pub const PAPERS_FILE: &str = "papers.json";

/// The human's record directory for a ship store: `<root>/captains/<id>`, where the
/// ship store is `<root>/worlds/<world>`.
pub fn captain_dir(ship_dir: &Path, captain_id: &str) -> Option<std::path::PathBuf> {
    if captain_id.is_empty() {
        return None;
    }
    Some(
        ship_dir
            .parent()?
            .parent()?
            .join("captains")
            .join(captain_id),
    )
}

pub fn load_papers(captain_dir: &Path) -> CaptainPapers {
    std::fs::read_to_string(captain_dir.join(PAPERS_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Written whole, through a temp file, at 0600: it carries secrets.
pub fn save_papers(captain_dir: &Path, papers: &CaptainPapers) -> std::io::Result<()> {
    std::fs::create_dir_all(captain_dir)?;
    let bytes = serde_json::to_vec_pretty(papers)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let tmp = captain_dir.join(format!("{PAPERS_FILE}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, captain_dir.join(PAPERS_FILE))
}

/// File a key on the record, replacing any entry with the same key id.
pub fn file_paper(papers: &mut CaptainPapers, paper: Paper) {
    papers.keys.retain(|p| p.key_id != paper.key_id);
    papers.keys.push(paper);
}

/// Which of the keys flies the hull `want`, given what each answered for this fold:
/// `(is_captain_key, actor it answers for)`. The captain's own papers first — they
/// carry every verb — then the hull's co-pilot; `None` is a hold: nobody we hold a
/// key for may act for this hull right now.
pub fn pick_papers(want: &str, answers: &[(bool, &str)]) -> Option<usize> {
    let find = |captain: bool| {
        answers
            .iter()
            .position(|(is_captain, actor)| *is_captain == captain && *actor == want)
    };
    find(true).or_else(|| find(false))
}

#[cfg(test)]
mod papers_tests {
    use super::*;

    #[test]
    fn the_captains_key_flies_the_hull_the_captain_is_aboard() {
        // Before any ship change each captain key answers for its own hull.
        let answers = [
            (true, "player:kk"),
            (true, "player:kbc3"),
            (false, "player:kbc4"),
        ];
        assert_eq!(pick_papers("player:kk", &answers), Some(0));
        assert_eq!(pick_papers("player:kbc3", &answers), Some(1));
        assert_eq!(
            pick_papers("player:kbc4", &answers),
            Some(2),
            "its co-pilot"
        );
    }

    #[test]
    fn after_a_ship_change_the_captain_flies_where_they_stand() {
        // The captain boarded KBC-04: both captain keys now answer for it.
        let answers = [
            (true, "player:kbc4"),
            (true, "player:kbc4"),
            (false, "player:kk"),
        ];
        assert_eq!(
            pick_papers("player:kbc4", &answers),
            Some(0),
            "full papers aboard"
        );
        assert_eq!(
            pick_papers("player:kk", &answers),
            Some(2),
            "KK on its co-pilot"
        );
        assert_eq!(
            pick_papers("player:kbc3", &answers),
            None,
            "no co-pilot: hold"
        );
    }

    #[test]
    fn papers_round_trip_at_0600() {
        let d = std::env::temp_dir().join(format!("papers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut p = CaptainPapers::default();
        let paper = |id: &str, kind: &str| Paper {
            server: "s".into(),
            key_id: id.into(),
            secret: format!("ucfk_{id}"),
            kind: kind.into(),
            hull_actor: String::new(),
            hull: String::new(),
            filed_at: 0,
        };
        file_paper(&mut p, paper("aaaa", "captain"));
        file_paper(&mut p, paper("aaaa", "copilot"));
        assert_eq!(p.keys.len(), 1, "one entry per key id");
        save_papers(&d, &p).unwrap();
        assert_eq!(load_papers(&d), p);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.join(PAPERS_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn board_files_a_ship_change_to_the_hull_named() {
        let o = Order {
            id: "o1".into(),
            verb: "board".into(),
            station: None,
            when: "next-docking".into(),
            amount: None,
            ship: Some("player:kbc4".into()),
            ship_name: Some("KBC-04".into()),
            by: "Luke".into(),
            at: 0,
            done_at: None,
            waits: None,
        };
        assert_eq!(
            o.action(),
            Some(serde_json::json!({"type": "transferCaptain", "to": "player:kbc4"}))
        );
        assert!(!o.ready(false), "under way: a ship change needs a berth");
        assert!(o.ready(true));
        assert!(!o.is_course(), "boarding does not supersede the course");
    }

    #[test]
    fn a_copilot_files_none_of_the_captains_verbs() {
        for v in [
            "travel",
            "book",
            "cancelBooking",
            "collect",
            "refuel",
            "engage",
        ] {
            assert!(!COPILOT_DENIED.contains(&v), "{v} is a co-pilot verb");
        }
        for v in ["buy", "sell", "repair", "paws", "transferCaptain"] {
            assert!(COPILOT_DENIED.contains(&v), "{v} is the captain's");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::{Level, Surface};

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("whisker-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_dial_round_trips_through_the_store_including_market_margin() {
        let d = dir("dial");
        assert_eq!(load_dial(&d), Dial::default(), "absent = default");
        let mut dial = Dial::default();
        dial.set("market.margin", Level::Confirm).unwrap();
        dial.set("*", Level::Auto).unwrap();
        save_dial(&d, &dial).unwrap();
        let back = load_dial(&d);
        assert_eq!(back, dial);
        assert_eq!(back.level(Surface::MarketMargin), Level::Confirm);
        std::fs::write(d.join(DIAL_FILE), "not json").unwrap();
        assert_eq!(
            load_dial(&d),
            Dial::default(),
            "unreadable = default, never a panic"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn holdings_and_automations_read_back_what_was_written() {
        let d = dir("book");
        assert!(load_holdings(&d).is_empty());
        std::fs::write(d.join("automations.json"), r#"["freight","trade","warp"]"#).unwrap();
        let (granted, unknown) = granted_automations(&d);
        assert!(granted.contains(&Automation::Freight) && granted.contains(&Automation::Trade));
        assert_eq!(unknown, vec!["warp".to_string()]);
        std::fs::write(
            d.join("ucf.env"),
            "# key\nUCF_SERVER=\"http://x:1\"\nUCF_KEY=abc\n",
        )
        .unwrap();
        assert_eq!(
            env_value(&d.join("ucf.env"), "UCF_SERVER").as_deref(),
            Some("http://x:1")
        );
        assert_eq!(
            env_value(&d.join("ucf.env"), "UCF_KEY").as_deref(),
            Some("abc")
        );
        assert_eq!(env_value(&d.join("ucf.env"), "NOPE"), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// A captain's standing order on this hull (2026-09-10: "an interactive method for the
/// captain to do manual actions… 'repair at next docking'"). The
/// pilot files it under the captain's authority at the first fold that satisfies
/// it, ahead of its own doctrine; a verb the key cannot file leaves the order
/// waiting for the captain's own papers, and says so.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Order {
    pub id: String,
    /// `repair` | `refuel` | `payLease` | `travel` | `hold` — the last two being the
    /// verbs a captain actually says: "go to X", "wait there".
    pub verb: String,
    /// `travel`: where; `hold`: where the hull holds (None = wherever it is).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station: Option<String>,
    /// `next-docking` | `now`
    #[serde(default = "next_docking")]
    pub when: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    /// `board`: the hull the captain steps aboard — its durable actor id — and its
    /// name as the captain said it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship_name: Option<String>,
    pub by: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<i64>,
    /// Why the order has not been filed: the key cannot, or the exchange refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits: Option<String>,
}

fn next_docking() -> String {
    "next-docking".into()
}

impl Order {
    pub fn pending(&self) -> bool {
        self.done_at.is_none()
    }

    /// Ready to file: `now` always; `next-docking` when docked. A `hold` is never
    /// filed — it is a standing state, satisfied by being pending (see
    /// [`standing_course`]); a `travel` is filed at once.
    pub fn ready(&self, docked: bool) -> bool {
        match self.verb.as_str() {
            "hold" => false,
            "travel" | "paws" => self.pending(),
            _ => self.pending() && (self.when == "now" || docked),
        }
    }

    /// A course order (`travel` | `hold`): the captain's word on where the hull goes
    /// and stays. Every later course order supersedes it (see [`place_order`]).
    pub fn is_course(&self) -> bool {
        matches!(self.verb.as_str(), "travel" | "hold")
    }

    /// The action body the exchange takes for this verb; None for a verb the
    /// pilot does not know how to file.
    pub fn action(&self) -> Option<serde_json::Value> {
        match self.verb.as_str() {
            "travel" => self
                .station
                .as_ref()
                .map(|st| serde_json::json!({"type": "travel", "station": st})),
            "repair" => Some(serde_json::json!({"type": "repair"})),
            // The tanker, on the captain's word: the pilot's own doctrine calls it only
            // under --allow-paws; an order is the captain calling it.
            "paws" => Some(serde_json::json!({"type": "paws"})),
            // The captain changes ship (metal#100): filed on the papers of the hull
            // being left, whose money goes with them.
            "board" => self
                .ship
                .as_ref()
                .map(|to| serde_json::json!({"type": "transferCaptain", "to": to})),
            "refuel" => Some(match self.amount {
                Some(u) => serde_json::json!({"type": "refuel", "units": u}),
                None => serde_json::json!({"type": "refuel"}),
            }),
            "payLease" => {
                Some(serde_json::json!({"type": "payLease", "amount": self.amount.unwrap_or(0)}))
            }
            _ => None,
        }
    }
}

/// Place an order: a course order (`travel` | `hold`) supersedes every pending course
/// order before it — the captain's latest word is the word — and any other verb
/// simply joins the list. Returns the orders to save.
pub fn place_order(mut orders: Vec<Order>, order: Order, now: i64) -> Vec<Order> {
    if order.is_course() {
        // A hold AT a berth does not supersede the travel TO that berth: they are one
        // course, said in two words ("go to X, wait there"). Until 2026-09-20 the hold
        // marked the travel done a second after it was placed, so a hull not yet at X
        // never flew there — it just stood wherever it was, "under orders".
        let same_berth = |o: &Order| {
            order.verb == "hold"
                && o.verb == "travel"
                && order.station.is_some()
                && o.station == order.station
        };
        for o in orders
            .iter_mut()
            .filter(|o| o.is_course() && o.pending() && !same_berth(o))
        {
            o.done_at = Some(now);
            o.waits = Some(format!("superseded by {}", order.id));
        }
    }
    orders.push(order);
    orders
}

/// "Resume" — the captain's word that ends the standing course: every pending course
/// order is done ("resumed by the captain"), the resume itself is kept as a done
/// order so the list shows who ended it, and the doctrine flies again next fold.
pub fn resume(mut orders: Vec<Order>, mut resume: Order, now: i64) -> Vec<Order> {
    for o in orders.iter_mut().filter(|o| o.is_course() && o.pending()) {
        o.done_at = Some(now);
        o.waits = Some(format!("resumed by the captain ({})", resume.id));
    }
    resume.done_at = Some(now);
    orders.push(resume);
    orders
}

/// What the captain's standing course is, if any: the newest pending `travel` or
/// `hold`, as "travel to X" / "hold at X" / "hold here". While one stands, the pilot
/// files nothing of its own doctrine — the drive still engages for a laid course,
/// and standing orders still file — until the captain's next word or a withdrawal.
pub fn standing_course(orders: &[Order]) -> Option<String> {
    orders
        .iter()
        .rev()
        .find(|o| o.is_course() && o.pending())
        .map(|o| match (o.verb.as_str(), &o.station) {
            ("travel", Some(st)) => format!("travel to {st}"),
            ("hold", Some(st)) => format!("hold at {st}"),
            ("hold", None) => "hold here".to_string(),
            (v, _) => v.to_string(),
        })
}

pub fn load_orders(ship_dir: &Path) -> Vec<Order> {
    std::fs::read_to_string(ship_dir.join("orders.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_orders(ship_dir: &Path, orders: &[Order]) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(orders)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let tmp = ship_dir.join(format!("orders.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, ship_dir.join("orders.json"))
}

#[cfg(test)]
mod order_tests {
    use super::*;

    #[test]
    fn an_order_is_ready_when_its_moment_comes_and_files_the_right_verb() {
        let o = Order {
            id: "ord-1".into(),
            verb: "repair".into(),
            station: None,
            when: "next-docking".into(),
            amount: None,
            ship: None,
            ship_name: None,
            by: "Luke".into(),
            at: 1,
            done_at: None,
            waits: None,
        };
        assert!(!o.ready(false));
        assert!(o.ready(true));
        assert_eq!(o.action(), Some(serde_json::json!({"type": "repair"})));
        let pay = Order {
            verb: "payLease".into(),
            when: "now".into(),
            amount: Some(500),
            ..o.clone()
        };
        assert!(pay.ready(false));
        assert_eq!(
            pay.action(),
            Some(serde_json::json!({"type": "payLease", "amount": 500}))
        );
        assert!(Order {
            verb: "dance".into(),
            ..o.clone()
        }
        .action()
        .is_none());
        let done = Order {
            done_at: Some(2),
            ..o
        };
        assert!(!done.ready(true));
        let d = std::env::temp_dir().join(format!("orders_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        save_orders(&d, std::slice::from_ref(&done)).unwrap();
        assert_eq!(load_orders(&d), vec![done]);
    }

    /// "bring all the ships to paws truck stop, wait there" is a `travel` that
    /// files at once and a `hold` that never files but stands; the newest course
    /// order supersedes the older; a withdrawn hold ends the standing course.
    #[test]
    fn a_course_order_stands_until_the_captains_next_word() {
        let base = Order {
            id: "ord-1".into(),
            verb: "travel".into(),
            station: Some("paws-truckstop".into()),
            when: "now".into(),
            amount: None,
            ship: None,
            ship_name: None,
            by: "Luke".into(),
            at: 1,
            done_at: None,
            waits: None,
        };
        assert!(base.ready(false));
        assert_eq!(
            base.action(),
            Some(serde_json::json!({"type": "travel", "station": "paws-truckstop"}))
        );
        let hold = Order {
            id: "ord-2".into(),
            verb: "hold".into(),
            ..base.clone()
        };
        assert!(!hold.ready(true), "a hold is never filed");
        assert_eq!(hold.action(), None);
        let orders = place_order(Vec::new(), base.clone(), 1);
        let orders = place_order(orders, hold.clone(), 2);
        assert_eq!(
            standing_course(&orders).as_deref(),
            Some("hold at paws-truckstop")
        );
        assert!(
            orders[0].done_at.is_none(),
            "the travel to the same berth still files; the hold stands behind it"
        );
        // A hold somewhere ELSE does supersede the travel.
        let elsewhere = Order {
            id: "ord-2b".into(),
            verb: "hold".into(),
            station: Some("cannery-row".into()),
            ..base.clone()
        };
        let other = place_order(vec![base.clone()], elsewhere, 2);
        assert_eq!(other[0].waits.as_deref(), Some("superseded by ord-2b"));
        // The captain's next word: a new travel supersedes the hold.
        let next = Order {
            id: "ord-3".into(),
            station: Some("cannery-row".into()),
            ..base.clone()
        };
        let orders = place_order(orders, next, 3);
        assert_eq!(
            standing_course(&orders).as_deref(),
            Some("travel to cannery-row")
        );
        assert_eq!(orders[1].waits.as_deref(), Some("superseded by ord-3"));
        // A repair beside it changes nothing about the course.
        let repair = Order {
            id: "ord-4".into(),
            verb: "repair".into(),
            station: None,
            ..base.clone()
        };
        let orders = place_order(orders, repair, 4);
        assert_eq!(
            standing_course(&orders).as_deref(),
            Some("travel to cannery-row")
        );
        assert_eq!(orders.iter().filter(|o| o.pending()).count(), 2);
        assert_eq!(standing_course(&[]), None);
        // "As you were": the course ends, the repair still stands.
        let orders = resume(
            orders,
            Order {
                id: "ord-5".into(),
                verb: "resume".into(),
                station: None,
                ..base.clone()
            },
            5,
        );
        assert_eq!(standing_course(&orders), None);
        assert_eq!(
            orders
                .iter()
                .filter(|o| o.pending())
                .map(|o| o.verb.as_str())
                .collect::<Vec<_>>(),
            ["repair"]
        );
        assert_eq!(
            orders[2].waits.as_deref(),
            Some("resumed by the captain (ord-5)")
        );
        // The tanker on the captain's word files at once, wherever the hull is.
        let paws = Order {
            id: "ord-6".into(),
            verb: "paws".into(),
            station: None,
            ..base.clone()
        };
        assert!(paws.ready(false));
        assert_eq!(paws.action(), Some(serde_json::json!({"type": "paws"})));
    }
}
