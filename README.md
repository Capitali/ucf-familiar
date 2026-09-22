# UCF Familiar — a ship's computer for United Cat Foods

A small, self-contained autopilot and bridge for hulls on the **United Cat Foods exchange**:
a pilot that flies a freight ship on its own, and a fleet CLI that commissions it, supervises
it, and serves a feed a companion app can talk to.

It is not a bot that plays a game for you. It is a ship's computer with a doctrine it can
explain, a captain who can overrule it in plain sentences, and a dial that says, for every
kind of act, whether it may do the thing, ask first, or only advise.

```
captain ──talks to──▶ ucf-familiar (the fleet CLI)
                           │
              ┌────────────┴────────────┐
              │                         │
        the exchange                the fleet feed
        (the world's API)           `fleet serve`, for a
              │                      companion app to read
              └────────────┬────────────┘
                           ▼
                  whisker — the pilot
             (one process per hull, on your Mac)
```

## What it does

- **Flies freight.** Reads the load board, books what pays after fuel and the lease's bite,
  deadheads to the pickup, flies the laden leg at the contract's service class, delivers,
  collects. It holds up to the exchange's bay limit and plans the whole tour: the cheapest
  order of every stop that lands every delivery in time, with detours priced against what
  they cost in fuel and in the other contracts' earning time.
- **Keeps the ship.** Fuel before work, a burn rung that reaches when the standard one
  cannot, repair at the wear the hull's ownership justifies, fittings out of earnings, the
  lease paid down before luxuries.
- **Trades, when told it may.** A speculative book with a hold clock, sold where the goods
  are worth most from here rather than against what they cost.
- **Takes orders.** "Everyone get to tuna-prime", "wait there", "all ships back to work" —
  read as orders, filed on the hulls they name, and standing until the next word.
- **Says why.** Every fold is journaled in words: what it did, what it would have done, what
  it refused and on whose authority.

## The dial

Nothing here is all-or-nothing. Every control surface — `navigation`, `freight`, `market`,
`ship`, `racing`, and the families beneath them — carries one of three settings:

| Setting | What happens |
|---|---|
| `advise` | It tells you what it would do. It does nothing. |
| `confirm` | It proposes; the act waits for your yes. |
| `auto` | It acts, and journals what it did. |

Unset surfaces are `auto`, with two deliberate exceptions that default to `advise`: calling
the PAWS tanker, and trading on the credit line. The captain's standing orders outrank all
three: while a course stands, the pilot files nothing of its own.

## Running it

```sh
cargo build --release      # target/release/whisker (the pilot) + ucf-familiar (the CLI)

# a captain's key becomes a ship world of its own
ucf-familiar fleet pair --label "Kibble Klipper II" --captain "Luke SkyWhisker" \
    --server https://exchange.example/ --key ucfk_… \
    --automations freight,trade,outfit

ucf-familiar fleet status                  # where everyone is, and what they are worth
ucf-familiar fleet run --renew             # supervise every paired hull's pilot
ucf-familiar fleet serve --bind 127.0.0.1:7899   # the feed a companion app reads

ucf-familiar autonomy show  <ship>         # the dial, surface by surface
ucf-familiar autonomy set   <ship> market=confirm navigation.rescue=advise
ucf-familiar autonomy advice <ship>        # what it would have done, and why
ucf-familiar autonomy approve <ship> <proposal-id>

ucf-familiar fleet order  <world> travel --station paws-truckstop
ucf-familiar fleet orders <world>          # the captain's standing orders, and their state
```

`ucf-familiar` with no arguments prints the whole surface, `world` commands included.

A hull's whole state is plain files in its own directory — the journal, the dial, the
standing orders, the persona. Nothing is hidden in a database, and every file is yours.

## The app half

There is no app in this repository. `fleet serve` is the half that faces one: a plain,
bearer-authenticated HTTP feed of every ship, its journal, its proposals, its dial and its
books, plus the captain acts (approve, retune, pair, order, rename). A companion app reaches
the fleet two ways — straight at the exchange with the captain's key, or through the feed a
Mac serves for the whole fleet — and `crates/pilot/src/wire.rs` is the seam that lets the
same doctrine answer on either side of that line.

## What it is not

No mesh, no household daemon, no cloud account, no telemetry. It talks to one exchange and
writes to one directory. If it ever stops earning its keep, it is one process to stop and one
folder to delete.

## Licence

Apache-2.0.
