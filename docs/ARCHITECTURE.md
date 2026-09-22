# Architecture

A Rust workspace of six crates producing two binaries: `whisker`, the pilot that flies one
hull, and `ucf-familiar`, the fleet CLI that commissions ships, leases them their authority,
supervises their pilots, and serves a read/write feed for a companion app.

Dependencies are deliberately few: `serde`/`serde_json` everywhere, `rustls` in the wire,
and `ed25519-dalek`/`sha2`/`getrandom` in the one crate that signs. There is no async
runtime, no HTTP framework, no database, and no argument-parsing crate.

## The crates

| Crate | Binary | What it owns |
|---|---|---|
| `ucf-wire` | — | The only socket. A parsed `Url`, a blocking HTTP/1.1 client with a bounded response, and TLS that verifies the platform trust store or refuses. Plain `http` is permitted to loopback only. |
| `ucf-persona` | — | Who the ship's computer is (`persona.json`: name, role phrase, style, pronouns) and its append-only naming trail; plus `store::data_dir`, `identity::current`, and the `Boundary` capability policy. |
| `ucf-node` | — | The ed25519 keypair a node is known by, its short fingerprint (`SHA-256(pubkey)[..8]`, hex), and signature verification. The fleet signs leases with its key; each ship mints its own. |
| `ucf-world` | — | The partition. `instance` (the provisioning record for a commissioned ship), `bridge` (typed envelopes and payload-free receipts), `lease` (the signed, expiring projection of the human-owned boundary). |
| `ucf-pilot` | `whisker` | The doctrine, pure, plus the runner. `doctrine` (freight, fuel, repair, the tour), `trade` (the merchant), `outfit` (fittings, debt, frames), `chain` (supply-chain arithmetic and the dispatch deck), `autonomy` (the dial), `wire` (JSON in, decision out), `store` (every file the runner touches), and `main.rs` (the fold loop). |
| `ucf-cli` | `ucf-familiar` | The captain's side: `fleet` (pair, status, run, rename, orders, economy, names), `autonomy` (show, set, advice, approve, deny), `world` (commission, lease, rename, decommission), and `fleet serve` (the feed). |

The decision crates own no file, socket or clock: facts in, a decision out. That is what lets
the same doctrine fly the hull from the runner and answer a companion app through a seam.

## The fold loop

One pass of `whisker` — a *fold* — runs in this order, and files at most one intent:

1. **Read the hull.** `GET /v1/me`, and `/v1/status` for the tick and tick length.
2. **Re-read the dial.** `autonomy.json`, every fold, so a captain's change lands at once.
3. **Gate 1 — the lease.** `lease.json` must verify against `issuer.json`, must not have
   expired, and its projected boundary must open `allow_network`. A shut gate is a journaled
   refusal and a patient sleep, never an exit.
4. **Read the standing course.** While a captain's `hold`/`travel` order stands, every act of
   the pilot's own doctrine is refused at the dial gate; standing orders file outside it.
5. **Reconcile.** The active contract and the companions beside it are re-read from the
   exchange's own ledger — the fold is the truth, not the pilot's memory. A closed contract
   leaves the bay; a restart mid-fold recovers from the journal's last acted line.
6. **Outfit** (`Automation::Outfit`). Berthed and freight-idle: debt before fittings, a
   sister's lease before a fitting of one's own, a frame rung only on evidence.
7. **Merchant** (`Automation::Trade`). The chain forecast is built once per fold for every
   hull; sells run even while hauling, buys only when freight is idle and nothing is held.
8. **Standing orders.** An order the fold can satisfy is filed under the captain's own
   authority, ahead of the doctrine and ahead of the dial.
9. **Money on the desk.** One delivered-but-uncollected contract is collected per fold.
10. **Decide freight.** The board is scored, the chain's word breaks near-ties, the tour is
    planned, and `doctrine::decide_with` returns one decision.
11. **Gate 2 — the automation.** A decision whose `Automation` the ship store does not grant
    is refused and journaled.
12. **The dial gate.** `Act`, `Advise`, `Proposed` or `Lapsed` (see below).
13. **File it.** `POST /v1/actions` with an `actionId` that is an idempotency handle: a
    re-send carries the *same* id, never a fresh one.
14. **Journal, then sleep** three-fifths of a tick (with a floor), so the pilot wakes inside
    the window its last act belongs to.

## The dial

Every decision names a control surface: `navigation[.course|.fuel|.rescue]`,
`freight[.book|.collect|.cancel]`, `market[.buy|.sell|.carry]`,
`ship[.repair|.refit|.crew|.frame|.lease]`, `racing[.plot|.line|.refusal]`. The dial is
`autonomy.json` — a map of surface, family, or `*` to a level, most specific winning:

- `advise` — say what would be done, do nothing.
- `confirm` — write a `Proposal` to `proposals.jsonl` and wait; the captain's `Approval` in
  `approvals.jsonl` releases it on a later fold, and an unapproved proposal lapses at its
  `expires_tick`.
- `auto` — act, and journal what was done.

Unset surfaces are `auto`, with two exceptions that default to `advise`: `navigation.rescue`
(a tanker call is a multi-day strand that also pins the hull) and `market.margin` (trading on
the credit line). A proposal's id is derived from its surface and body, so the same intent on
the next fold finds its own earlier proposal instead of filing a second.

**Standing orders outrank the dial.** While a course stands, the pilot's own acts are refused
at the gate whatever the dial says, and the captain's orders are filed under the captain's
authority rather than proposed back to them. A verb the ship's key may not file leaves the
order waiting for the captain's own papers, and says so.

## Freight: pricing and the tour

A load is scored on **net per tick of pilot time** — `estimated_net / (deadhead + haul)` —
against the spare hold, never the whole one. The top candidates are then priced for real:
the router is asked for the deadhead and laden legs, fuel is charged at the world's price, and
a burn rung is chosen — standard where it reaches, a lower rung when it does not, never a
hotter one. Among loads within a narrow band of the best rate, the chain's word (a works
whose input shelf is draining, or an output shelf filling inside the horizon) breaks the tie;
it never lifts a load that pays materially less.

With more than one contract in the bay, `plan_tour` enumerates every feasible order of the
required stops — each booked contract's pickup, then every delivery, pickups before their own
deliveries — and picks the best by, in order: fewest deliveries past their deadline, fewest
ticks, least fuel. The winning order's first stop is the next leg, and its stations define
what "on the way" means: a second load is booked only where it rides for free or where its net
beats the detour's cost, priced as the extra fuel plus the extra ticks at the rate the held
contracts already earn.

## On disk

A ship store — `worlds/<world-id>/` — is plain files, one line each:

| File | What it holds |
|---|---|
| `ucf.env` | `UCF_SERVER` and `UCF_KEY` (0600): the exchange and the captain's key. |
| `issuer.json` | The lease issuer's public node identity, written at commissioning. |
| `lease.json` | The signed, expiring boundary projection — the ship's whole authority. |
| `automations.json` | The granted automation scopes, as a JSON array of names. |
| `captain.json` | Who this hull flies for: captain id, display name, key id, server, pairing time. |
| `autonomy.json` | The dial: surface → level. |
| `proposals.jsonl` | Acts proposed under `confirm`, awaiting the captain. |
| `approvals.jsonl` | The captain's yes or no on each proposal. |
| `orders.json` | The captain's standing orders and their state. |
| `holdings.json` | The merchant's speculative book: lot, basis, hold clock, sell target. |
| `deliveries.jsonl` | One line per delivered contract: what was hauled and what it paid. |
| `journal.jsonl` | Every fold in words: acts, advice, refusals, forecasts, accounts. |
| `persona.json` | The ship-local computer record, where a captain store does not hold it. |
| `persona-names.jsonl` | That computer's naming trail, append-only. |
| `mesh/node_key`, `mesh/node.json` | The ship's own ed25519 key (0600) and its public record. |
| `whisker.pid` | The running pilot's pid, so the fleet knows it is aboard. |

Beside the ship stores, in the fleet's own data dir:

- `worlds/instances.json` — the provisioning registry: one record per commissioned ship.
- `worlds/receipts.jsonl` — payload-free receipts of outward crossings.
- `captains/<captain-id>/persona.json` + `persona-names.jsonl` — one computer per captain,
  which every hull that captain pairs answers as.
- `captains/names.jsonl` — the fleet-wide names ledger: every name any captain, hull or
  computer has ever worn. Append-only; nothing is ever removed.
- `boundary.json` — the human-owned capability policy the leases are projected from. There is
  no code path that widens it; a missing or unreadable file reads as fully closed.

## Two client modes

A companion app can reach a fleet two ways:

1. **Direct to the exchange.** The app holds the captain's key, fetches the documents itself,
   and calls `ucf_pilot::wire::advise` with them through an FFI shim — the same doctrine,
   running app-side. `SEAM_VERSION` names the shape of that seam's input and output, and is
   bumped whenever a new input fact becomes load-bearing, so a client built against one seam
   can tell it has been handed another. Shared fixtures under
   `crates/pilot/tests/fixtures/contract/` are pinned on both sides.
2. **Through the host feed.** `ucf-familiar fleet serve` exposes the ship stores over plain
   HTTP/1.1 with a bearer from `fleet-serve.token` (minted 0600 on first run): one thread per
   connection, a 64 KiB request cap, no TLS — bind it to loopback or a private network
   address. Reads: `/ships`, `/ships/{world}/journal|proposals|dial|book|fuel|orders|brief`,
   `/captains/{id}/brief|economy`, `/brief`, `/names`. Writes: approve a proposal, set the
   dial, set automations, rename, set the captain, place or withdraw orders, `POST /pair`,
   `POST /unpair`, `DELETE /ships/{id}`. Every reply carries `tick` and `tick_seconds` so the
   client settles proposal lapse exactly as the pilot does.

## The wire surface

Everything this workspace asks of the exchange:

**Reads** — `/v1/me` (the hull, its contracts, cargo, credits and debt), `/v1/status` (tick
and tick length), `/v1/reference` (the pack: recipes, decay, yard rates, fuel price),
`/v1/loadboard`, `/v1/route?from&to[&hull&serviceClass]` (fuel and leg separations, and the
world's own price for a burn rung), `/v1/stations` (which berths sell fuel),
`/v1/stations/{id}/quotes` (a shelf's stock, capacity, equilibrium and prices),
`/v1/galaxy/prices`, `/v1/news` (announcements, read against the vendored deck),
`/v1/stations/{id}/production` and `/v1/industry/series` (what the lines actually made),
`/v1/receipts` (what the exchange did with a filed intent), `/v1/cash`.

**Writes** — `POST /v1/actions` with an `actionId`, carrying one of: `travel`, `engage`,
`book`, `collect`, `refuel`, `repair`, `refit`, `expandFrame`, `payLease`, `paws`, `buy`,
`sell`. `POST /v1/profile` files the ship's or the computer's name under the captain's own
papers.

Deliberately not read: `/v1/events`, the route that publishes an announcement's resolved
outcome. The pilot plays from the prior, which is the information game as designed.
