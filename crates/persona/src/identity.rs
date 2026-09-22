//! Who is present — the one question this workspace asks of an identity store.
//!
//! A full identity store keeps a whole roster: people it has met, how it came to recognize
//! them, what standing each has. None of that is needed to fly a hull. What is needed is
//! the single file such a roster writes last — `observer.txt`, the handle of
//! whoever is at the terminal right now — because a world "begins by a human's word" and
//! the commissioning ceremony needs a name to record when the human does not pass one.
//!
//! Read-only on purpose: there is no `set_current` here. A ship's computer does not decide
//! who is standing in front of it.

use std::path::Path;

/// The file written when someone introduces themselves.
pub const OBSERVER_FILE: &str = "observer.txt";

/// The handle of whoever is present now (`None` until someone introduces themselves).
pub fn current(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(OBSERVER_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absent, blank, and whitespace-only all read as "nobody is here" — an empty handle
    /// is not a person, and a commissioner defaulted from one would be a fiction.
    #[test]
    fn a_blank_observer_is_nobody() {
        let d = std::env::temp_dir().join(format!("ucf_identity_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();

        assert_eq!(current(&d), None, "no file at all");
        std::fs::write(d.join(OBSERVER_FILE), "   \n").unwrap();
        assert_eq!(current(&d), None, "whitespace is not a handle");
        std::fs::write(d.join(OBSERVER_FILE), " skipper\n").unwrap();
        assert_eq!(current(&d).as_deref(), Some("skipper"));

        let _ = std::fs::remove_dir_all(&d);
    }
}
