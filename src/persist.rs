use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;

pub fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn unix_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("unix:{}", d.as_secs()))
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_json_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "mcp-gpui-docs-persist-{}-{}",
            std::process::id(),
            unix_stamp().replace(':', "")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.json");
        save_json(&path, &serde_json::json!({ "a": 1 })).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["a"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unix_stamp_prefix() {
        let s = unix_stamp();
        assert!(s.starts_with("unix:") || s == "unknown");
    }
}
