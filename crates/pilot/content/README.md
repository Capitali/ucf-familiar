# The dispatch deck

`ucf-events.json` is a verbatim copy of `Content/market/events.json` from the
UCF exchange's own content pack (SpaceTrucker2196/ucf-exchange, last synced from
commit ef8e9ab 2026-08-28, on 2026-09-15). `crates/pilot/src/chain.rs` reads
`/v1/news` against it. Refresh it by copying the file across from the exchange;
never edit it by hand.
