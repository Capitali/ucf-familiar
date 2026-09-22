//! Where the data lives — the whole of it.
//!
//! There is no database and no schema. A ship's computer keeps its world in plain files
//! (`ucf.env`, `journal.jsonl`, `orders.json`, `captain.json`, `holdings.json`, …), which a
//! human can read with `cat` and a backup can carry without a migration. So the whole of
//! this module is the one thing every command still needs: the resolution of `--data-dir`.

use std::path::{Path, PathBuf};

/// Default data directory when no override is given.
pub const DEFAULT_DATA_DIR: &str = "familiar_data";

/// Resolve the data directory from an optional override.
pub fn data_dir(override_dir: Option<&str>) -> PathBuf {
    PathBuf::from(override_dir.unwrap_or(DEFAULT_DATA_DIR))
}

/// Read one JSON document out of `dir/file`. A missing file is `None`, not an error — the
/// distinction the fail-closed loaders in this crate are built on (see `boundary::load`).
pub(crate) fn load_one<T: serde::de::DeserializeOwned>(
    dir: &Path,
    file: &str,
) -> std::io::Result<Option<T>> {
    match std::fs::read_to_string(dir.join(file)) {
        Ok(s) => Ok(Some(serde_json::from_str(&s).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_dir_defaults_and_overrides() {
        assert_eq!(data_dir(None), PathBuf::from(DEFAULT_DATA_DIR));
        assert_eq!(
            data_dir(Some("/tmp/elsewhere")),
            PathBuf::from("/tmp/elsewhere")
        );
    }
}
