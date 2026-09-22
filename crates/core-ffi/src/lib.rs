//! The ship's computer's MIND, packaged for an Apple shell — `whisker_advise`, and
//! nothing else.
//!
//! The iOS app is a bridge, not a second pilot: it fetches `/v1/me`, `/v1/loadboard`,
//! `/v1/stations` and the routes it wants priced with the captain's own key, hands them
//! over as one JSON object, and is told what the pilot would do now — the decision, the
//! dial surface it spends, the captain's level on it, the automation it needs. That is
//! the rule "one doctrine, two runtimes" (2026-09-05): the phone and the host runner
//! read the SAME [`ucf_pilot::wire::advise`], so a verdict can never depend on which
//! machine asked.
//!
//! Exactly one function crosses the boundary, and that is deliberate. Everything the
//! doctrine needs arrives as JSON in the call, so this crate holds no state, opens no
//! socket, reads no file and keeps no clock; there is nothing to found, join, start or
//! stop. The ship's store, its key and its lease stay on the host that flies the hull —
//! the phone never becomes a peer holding ship authority, it only asks a pure question
//! and gets a pure answer. The seam being one pure function is also why the app can pin
//! the shipped archive against current source in a test (`ios/UCFFamiliarTests`): a
//! fixture in, a verdict out, no world to set up.
//!
//! And the doctrine never ACTS here. Acting is the shell's, under the captain's tap.

uniffi::setup_scaffolding!();

/// The pilot's doctrine over wire JSON the shell fetched itself. Input is the object
/// [`ucf_pilot::wire::advise`] documents (`me`, `board`, `stations`, the priced routes,
/// the key's `denied` list...); the answer is that function's verdict JSON, or an
/// `{"error": ...}` object if the input was not JSON at all. Never touches the world.
#[uniffi::export]
pub fn whisker_advise(input_json: String) -> String {
    let input: serde_json::Value = match serde_json::from_str(&input_json) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({"error": format!("input is not JSON: {e}")}).to_string()
        }
    };
    ucf_pilot::wire::advise(&input).to_string()
}
