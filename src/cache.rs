use std::{fs, path::Path};

use color_eyre::eyre::{Context, ContextCompat, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::AgendaSnapshot;

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    version: u32,
    snapshot: AgendaSnapshot,
}

pub fn load(path: &Path) -> Result<Option<AgendaSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes =
        fs::read(path).wrap_err_with(|| format!("failed to read cache at {}", path.display()))?;
    let raw: Value = serde_json::from_slice(&bytes).wrap_err("failed to parse cache file")?;
    let version = raw
        .get("version")
        .and_then(Value::as_u64)
        .wrap_err("cache file is missing a numeric version")? as u32;
    if version != CACHE_VERSION {
        return Ok(None);
    }
    let envelope: CacheEnvelope =
        serde_json::from_value(raw).wrap_err("failed to parse cache snapshot")?;

    Ok(Some(envelope.snapshot))
}

pub fn store(path: &Path, snapshot: &AgendaSnapshot) -> Result<()> {
    let parent = path
        .parent()
        .wrap_err_with(|| format!("cache path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .wrap_err_with(|| format!("failed to create cache directory {}", parent.display()))?;

    let envelope = CacheEnvelope {
        version: CACHE_VERSION,
        snapshot: snapshot.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&envelope).wrap_err("failed to serialize cache")?;
    let tmp_path = path.with_extension("json.tmp");

    fs::write(&tmp_path, bytes)
        .wrap_err_with(|| format!("failed to write cache temp file {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).wrap_err("failed to replace cache atomically")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgendaSnapshot, DateRange};
    use time::macros::datetime;

    #[test]
    fn ignores_unknown_version() {
        let dir = std::env::temp_dir().join(format!("canvastui-cache-test-{}", std::process::id()));
        let path = dir.join("snapshot.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, r#"{"version":99,"snapshot":{"loaded_range":{"start":"2026-01-01","end":"2026-01-02"},"fetched_at":"2026-01-01T00:00:00Z","days":[]}}"#).unwrap();

        let loaded = load(&path).unwrap();
        assert!(loaded.is_none());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn round_trips_snapshot() {
        let dir =
            std::env::temp_dir().join(format!("canvastui-cache-roundtrip-{}", std::process::id()));
        let path = dir.join("snapshot.json");
        let snapshot = AgendaSnapshot::empty(
            DateRange::new(
                datetime!(2026-01-01 0:00 UTC).date(),
                datetime!(2026-01-02 0:00 UTC).date(),
            ),
            datetime!(2026-01-01 0:00 UTC),
        );

        store(&path, &snapshot).unwrap();
        let loaded = load(&path).unwrap().unwrap();
        assert_eq!(loaded.loaded_range.start, snapshot.loaded_range.start);

        let _ = fs::remove_dir_all(dir);
    }
}
