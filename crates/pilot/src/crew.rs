//! The crew doctrine, pure: who to sign on, and how to keep them fed (2026-10-19:
//! "Start crew doctrine from dials on #83"). The numbers are the world's —
//! `/v1/reference.params` publishes what a hand costs and what one is worth since
//! ucf-exchange#83 — and the evidence is the ship's own.
//!
//! What the engine prices (united-cat-foods-metal `Ship/Crew.swift`,
//! `Economy/CrewActions.swift`, `Economy/Galley.swift`):
//! - **engine** — a leg's drive wear is cut by `crewEngineWearReliefBps` times the
//!   post's share (effective skill, morale folded in, capped at one full share);
//! - **hold** — the gantry clock is cut by `crewHoldHandlingReliefBps` the same way;
//! - **galley** — a missed wage costs less morale (capped at half a bad day). It pays
//!   nobody, so a pilot that always makes payroll gets nothing from a cook;
//! - **bridge** — no rules yet.
//!
//! A hand costs `crewHireCost` once and `crewWagePerDay` every day roll; the galley
//! takes `galleyCreditsPerHandDay` and `galleyLoafPerHandDay` of kibble-loaf from the
//! hopper per hand, and an unfed day costs morale (`galleyHungerMoraleFallBps`)
//! whatever the cause — switched off, jammed, an empty hopper, a short account.
//! Morale multiplies skill, so an unfed engine room is a worse engine room.
//!
//! The rules, and the reasoning:
//! - **Crew after title.** On a leased hull the yard clears wear for nothing, so an
//!   engineer's job is already done for free (the standing rule in `outfit`).
//! - **The engine room first, on measured wear.** The drive's wear is the one cost a
//!   hand demonstrably takes off, and the ship can measure it: the bps it accrues per
//!   day at the yard's own rate. No day of measurement, no hire — a hire on a guess
//!   is a guess dressed as a decision.
//! - **A hand must earn twice its keep.** Wage, meal credits, loaf and the hire fee
//!   spread over [`HIRE_AMORTIZE_DAYS`] against the wear the relief saves, with
//!   [`HIRE_MARGIN`] of headroom because the measurement is a week's weather, not a
//!   climate.
//! - **One hand per post.** The post's share is capped at one full share, so a second
//!   engineer at full skill adds nothing: a crowded engine room is a rostering
//!   decision, not an exploit (the engine's own words).
//! - **Feed who is aboard.** Galley on, dispenser cleared at a berth, hopper kept above
//!   [`LOAF_LOW_DAYS`] and stocked toward [`LOAF_STOCK_DAYS`] — loaf decays in the
//!   hold, so it goes into the hopper the fold after it is bought.
//! - **The hold post waits for its evidence.** Its worth is dock time saved times
//!   what a tick earns; until the pilot measures its own gantry clock, it says so
//!   and hires nobody there.

/// The hire fee is spread over this many days when a hand's keep is priced.
pub const HIRE_AMORTIZE_DAYS: i64 = 30;
/// The relief must be worth this many times a hand's daily keep.
pub const HIRE_MARGIN: i64 = 2;
/// Days of payroll and meal credits kept in hand after a hire, above the ship's
/// own reserve.
pub const CREW_RESERVE_DAYS: i64 = 3;
/// Restock the hopper when it holds fewer than this many days of loaf…
pub const LOAF_LOW_DAYS: i64 = 2;
/// …and fill it toward this many.
pub const LOAF_STOCK_DAYS: i64 = 5;
/// The shipped pack's kibble-loaf base price, for pricing a hand's keep where the
/// berth under us does not quote loaf.
pub const LOAF_BASE_PRICE: i64 = 20;
/// The engine's day (`MarketState.ticksPerDay`, five-minute ticks).
pub const TICKS_PER_DAY: i64 = 288;
/// The good the galley's hopper takes.
pub const LOAF: &str = "kibble-loaf";

/// Names a pilot gives the hands it signs on, in order, skipping any already on the
/// articles (the engine refuses a duplicate: dismissal finds a hand by name).
const HAND_NAMES: [&str; 8] = [
    "Marmalade", "Tuxedo", "Pumpkin", "Socks", "Mochi", "Saffron", "Pepper", "Clementine",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hand {
    pub name: String,
    /// `bridge` | `engine` | `hold` | `galley`, as the wire spells it.
    pub station: String,
    /// `effectiveSkillBps` on `/v1/me`: skill as the post gets it, morale folded in.
    pub effective_skill_bps: i64,
    pub morale_bps: i64,
}

/// What the world charges and pays for crew (`/v1/reference.params`, #83). Zero
/// means not priced here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrewPrices {
    pub hire_cost: i64,
    pub wage_per_day: i64,
    pub max_aboard: i64,
    pub engine_relief_bps: i64,
    pub hold_relief_bps: i64,
    pub galley_credits_per_hand_day: i64,
    pub galley_loaf_per_hand_day: i64,
}

/// The galley as `/v1/me.galley` shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Galley {
    pub on: bool,
    pub jammed: bool,
    /// Loaf in the hopper.
    pub loaf: i64,
}

#[derive(Debug, Clone, Default)]
pub struct CrewView {
    pub titled: bool,
    pub docked: bool,
    pub credits: i64,
    /// The ship's own reserve (`outfit::reserve`): never spent on crew.
    pub reserve: i64,
    pub berths: i64,
    pub prices: CrewPrices,
    pub crew: Vec<Hand>,
    pub galley: Galley,
    /// Kibble-loaf already in the hold.
    pub loaf_aboard: i64,
    /// What the berth under us asks for loaf and how much it will sell, when it
    /// quotes it.
    pub loaf_ask: Option<(i64, i64)>,
    pub spare_hold: i64,
    /// What the drive's wear costs per day at the yard's rate, measured on this hull
    /// ([`WearMeter`]); None until a day has been watched.
    pub wear_cost_per_day: Option<i64>,
    /// The tick the measurement began, for the waiting line (stable across folds, so
    /// the journal says it once).
    pub wear_watch_from: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrewDecision {
    Idle { why: String },
    /// `hire {name, station}`.
    Hire { name: String, station: String, why: String },
    /// `galley {on: true}`.
    GalleyOn,
    /// `galley {service: true}`: clear a jammed dispenser.
    ServiceGalley,
    /// `galley {units}`: loaf from the hold into the hopper.
    StockGalley { units: i64 },
    /// `buy {station, good: kibble-loaf, units}` at this berth.
    BuyLoaf { units: i64, ask: i64 },
}

/// The post's share, in bps, capped at one full share (`share(at:)` in the engine).
fn share(crew: &[Hand], station: &str) -> i64 {
    crew.iter()
        .filter(|h| h.station == station)
        .map(|h| h.effective_skill_bps.max(0))
        .sum::<i64>()
        .min(10_000)
}

/// One hand's keep per day, ℳ: wage, meal credits, loaf, and the hire fee spread.
pub fn keep_per_day(v: &CrewView) -> i64 {
    let loaf_price = v.loaf_ask.map(|(ask, _)| ask).unwrap_or(LOAF_BASE_PRICE);
    v.prices.wage_per_day
        + v.prices.galley_credits_per_hand_day
        + v.prices.galley_loaf_per_hand_day * loaf_price
        + v.prices.hire_cost / HIRE_AMORTIZE_DAYS
}

fn next_name(crew: &[Hand]) -> String {
    HAND_NAMES
        .iter()
        .find(|n| !crew.iter().any(|h| h.name == **n))
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("Hand {}", crew.len() + 1))
}

pub fn decide_crew(v: &CrewView) -> CrewDecision {
    let p = &v.prices;
    let hands = v.crew.len() as i64;

    // Feed who is aboard before signing anyone else on.
    if hands > 0 && p.galley_loaf_per_hand_day > 0 {
        if !v.galley.on {
            return CrewDecision::GalleyOn;
        }
        if v.galley.jammed && v.docked {
            return CrewDecision::ServiceGalley;
        }
        let per_day = hands * p.galley_loaf_per_hand_day;
        if v.galley.loaf < per_day * LOAF_LOW_DAYS {
            let want = per_day * LOAF_STOCK_DAYS - v.galley.loaf;
            if v.loaf_aboard > 0 {
                return CrewDecision::StockGalley { units: v.loaf_aboard.min(want) };
            }
            if let (true, Some((ask, available))) = (v.docked, v.loaf_ask) {
                let afford = if ask > 0 { (v.credits - v.reserve).max(0) / ask } else { 0 };
                let units = want.min(available).min(v.spare_hold).min(afford);
                if units > 0 {
                    return CrewDecision::BuyLoaf { units, ask };
                }
            }
        }
    }

    if !v.titled {
        return CrewDecision::Idle {
            why: "crew waits for title: the yard clears a leased hull's wear for nothing".into(),
        };
    }
    if p.wage_per_day <= 0 || p.engine_relief_bps <= 0 || p.hire_cost <= 0 {
        return CrewDecision::Idle {
            why: "the reference prices no wage or no engine relief; nobody is hired on a guess"
                .into(),
        };
    }
    let max_aboard = if p.max_aboard > 0 { p.max_aboard.min(v.berths) } else { v.berths };
    if hands >= max_aboard {
        return CrewDecision::Idle {
            why: format!("every berth is taken ({hands} of {max_aboard})"),
        };
    }
    let manned = share(&v.crew, "engine");
    if manned >= 10_000 {
        return CrewDecision::Idle {
            why: "the engine room is manned; the hold post waits for the gantry clock's \
                  evidence"
                .into(),
        };
    }
    let Some(wear_cost) = v.wear_cost_per_day else {
        return CrewDecision::Idle {
            why: format!(
                "measuring the drive's wear for a day ({TICKS_PER_DAY} ticks) from t{} before \
                 hiring an engineer",
                v.wear_watch_from
            ),
        };
    };
    // A new hand at full skill fills what the post lacks, never more than one share.
    let added = (10_000 - manned).min(10_000);
    let saves = wear_cost * p.engine_relief_bps * added / 10_000 / 10_000;
    let keep = keep_per_day(v);
    if saves < HIRE_MARGIN * keep {
        return CrewDecision::Idle {
            why: format!(
                "an engineer would save ~ℳ{saves}/day of wear (ℳ{wear_cost}/day measured) \
                 against ~ℳ{keep}/day of keep — not {HIRE_MARGIN}× its keep"
            ),
        };
    }
    let hold_back = v.reserve
        + CREW_RESERVE_DAYS * (p.wage_per_day + p.galley_credits_per_hand_day) * (hands + 1);
    if v.credits - p.hire_cost < hold_back {
        return CrewDecision::Idle {
            why: format!(
                "saving to sign an engineer: ℳ{} + ℳ{hold_back} kept back > ℳ{} in hand",
                p.hire_cost, v.credits
            ),
        };
    }
    if !v.docked {
        return CrewDecision::Idle { why: "signing on needs a berth".into() };
    }
    CrewDecision::Hire {
        name: next_name(&v.crew),
        station: "engine".into(),
        why: format!(
            "an engineer saves ~ℳ{saves}/day of the ℳ{wear_cost}/day the drive wears, against \
             ~ℳ{keep}/day of keep (wage ℳ{}, galley ℳ{} + {} loaf, hire ℳ{} over \
             {HIRE_AMORTIZE_DAYS} days)",
            p.wage_per_day, p.galley_credits_per_hand_day, p.galley_loaf_per_hand_day,
            p.hire_cost
        ),
    }
}

/// The drive's wear, measured: bps accrued between folds (a drop is a repair and
/// starts nothing over), priced at the yard's rate per hundred bps.
#[derive(Debug, Clone, Default)]
pub struct WearMeter {
    start_tick: Option<i64>,
    last_bps: i64,
    accrued_bps: i64,
}

impl WearMeter {
    pub fn observe(&mut self, tick: i64, wear_bps: i64) {
        match self.start_tick {
            None => self.start_tick = Some(tick),
            Some(_) if wear_bps > self.last_bps => self.accrued_bps += wear_bps - self.last_bps,
            Some(_) => {}
        }
        self.last_bps = wear_bps;
    }

    pub fn observed_ticks(&self, tick: i64) -> i64 {
        self.start_tick.map_or(0, |s| tick - s)
    }

    /// The tick the watch began (0 before the first reading).
    pub fn since(&self) -> i64 {
        self.start_tick.unwrap_or(0)
    }

    /// ℳ per day at `rate` per hundred bps, once a full day has been watched.
    pub fn cost_per_day(&self, tick: i64, rate: i64) -> Option<i64> {
        let watched = self.observed_ticks(tick);
        (watched >= TICKS_PER_DAY)
            .then(|| self.accrued_bps * rate / 100 * TICKS_PER_DAY / watched.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PROD's dials as #83 published them (2026-10-01).
    fn prices() -> CrewPrices {
        CrewPrices {
            hire_cost: 500,
            wage_per_day: 100,
            max_aboard: 4,
            engine_relief_bps: 2500,
            hold_relief_bps: 3000,
            galley_credits_per_hand_day: 25,
            galley_loaf_per_hand_day: 1,
        }
    }

    fn titled() -> CrewView {
        CrewView {
            titled: true,
            docked: true,
            credits: 40_000,
            reserve: 3_000,
            berths: 4,
            prices: prices(),
            galley: Galley { on: true, jammed: false, loaf: 0 },
            spare_hold: 500,
            // KBC-03, titled, wears a drive out in ~2 days at ℳ40 per hundred bps.
            wear_cost_per_day: Some(2_000),
            wear_watch_from: 1_000,
            ..Default::default()
        }
    }

    fn engineer() -> Hand {
        Hand {
            name: "Marmalade".into(),
            station: "engine".into(),
            effective_skill_bps: 10_000,
            morale_bps: 10_000,
        }
    }

    #[test]
    fn a_titled_hull_that_wears_its_drive_signs_an_engineer() {
        match decide_crew(&titled()) {
            CrewDecision::Hire { name, station, why } => {
                assert_eq!((name.as_str(), station.as_str()), ("Marmalade", "engine"));
                // 25% of ℳ2,000/day against 100 + 25 + 1×20 + 500/30.
                assert!(why.contains("ℳ500/day") && why.contains("ℳ161/day"), "{why}");
            }
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn a_leased_hull_hires_nobody() {
        let mut v = titled();
        v.titled = false;
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("title")));
    }

    #[test]
    fn nobody_is_hired_on_a_guess() {
        let mut v = titled();
        v.wear_cost_per_day = None;
        v.wear_watch_from = 4_040;
        assert!(
            matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("from t4040"))
        );
        let mut v = titled();
        v.prices.engine_relief_bps = 0;
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("guess")));
    }

    #[test]
    fn a_gentle_drive_is_not_worth_an_engineer() {
        let mut v = titled();
        v.wear_cost_per_day = Some(600); // saves 150/day against 161 of keep
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("not 2×")));
    }

    #[test]
    fn the_hire_never_spends_the_reserve_or_the_payroll() {
        let mut v = titled();
        v.credits = 3_000 + 375 + 499; // reserve + 3 days of one hand's keep + not quite the fee
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("saving")));
        v.credits += 1;
        assert!(matches!(decide_crew(&v), CrewDecision::Hire { .. }));
    }

    #[test]
    fn one_engineer_is_enough() {
        let mut v = titled();
        v.crew = vec![engineer()];
        v.galley.loaf = 10;
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("manned")));
        // An unhappy engineer is half an engineer, and the post has room again —
        // worth a second hand only where the drive wears hard enough for half a share.
        v.crew[0].effective_skill_bps = 5_000;
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { why } if why.contains("not 2×")));
        v.wear_cost_per_day = Some(4_000);
        match decide_crew(&v) {
            CrewDecision::Hire { name, .. } => assert_eq!(name, "Tuxedo"),
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn the_crew_is_fed_before_anyone_else_signs_on() {
        let mut v = titled();
        v.crew = vec![engineer()];
        v.galley = Galley { on: false, jammed: false, loaf: 10 };
        assert_eq!(decide_crew(&v), CrewDecision::GalleyOn);
        v.galley = Galley { on: true, jammed: true, loaf: 10 };
        assert_eq!(decide_crew(&v), CrewDecision::ServiceGalley);
        v.docked = false;
        assert!(!matches!(decide_crew(&v), CrewDecision::ServiceGalley), "a jam needs a berth");
    }

    #[test]
    fn the_hopper_is_stocked_from_the_hold_then_from_the_berth() {
        let mut v = titled();
        v.crew = vec![engineer()];
        v.galley.loaf = 1;
        v.loaf_aboard = 9;
        assert_eq!(decide_crew(&v), CrewDecision::StockGalley { units: 4 });
        v.loaf_aboard = 0;
        v.loaf_ask = Some((22, 100));
        assert_eq!(decide_crew(&v), CrewDecision::BuyLoaf { units: 4, ask: 22 });
        v.loaf_ask = None;
        assert!(matches!(decide_crew(&v), CrewDecision::Idle { .. }), "no loaf here: fly on");
        v.galley.loaf = 2;
        v.loaf_aboard = 9;
        assert!(!matches!(decide_crew(&v), CrewDecision::StockGalley { .. }), "two days in hand");
    }

    #[test]
    fn the_meter_counts_wear_and_not_repairs() {
        let mut m = WearMeter::default();
        m.observe(1_000, 100);
        m.observe(1_100, 1_100);
        m.observe(1_150, 0); // repaired
        m.observe(1_288, 400);
        assert_eq!(m.cost_per_day(1_287, 40), None, "not yet a day");
        // 1,400 bps in 288 ticks at ℳ40 per hundred.
        assert_eq!(m.cost_per_day(1_288, 40), Some(560));
    }
}
