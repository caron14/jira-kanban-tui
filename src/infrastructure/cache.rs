use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 3;
const EXPIRY_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheData {
    pub version: u32,
    pub account: String,
    pub board_ref: String,
    pub issues: Vec<crate::domain::Issue>,
    pub board: crate::domain::Board,
    pub cached_at: chrono::DateTime<chrono::Utc>,
}

impl CacheData {
    fn root() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".jira-kanban-tui-cache")
    }
    pub fn path(account: &str, board_ref: &str) -> PathBuf {
        fn hex(value: &str) -> String {
            value.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
        }
        let safe = format!("{}--{}", hex(account), hex(board_ref));
        Self::root().join(format!("{safe}.json"))
    }
    pub fn load(account: &str, board_ref: &str, allow_stale: bool) -> Option<Self> {
        let data = std::fs::read_to_string(Self::path(account, board_ref)).ok()?;
        let cache: Self = serde_json::from_str(&data).ok()?;
        if cache.version != CACHE_VERSION
            || cache.account != account
            || cache.board_ref != board_ref
        {
            return None;
        }
        if !allow_stale && cache.is_expired() {
            return None;
        }
        Some(cache)
    }
    pub fn save(
        account: &str,
        board_ref: &str,
        board: &crate::domain::Board,
        issues: &[crate::domain::Issue],
    ) -> anyhow::Result<()> {
        let path = Self::path(account, board_ref);
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
        let value = Self {
            version: CACHE_VERSION,
            account: account.into(),
            board_ref: board_ref.into(),
            issues: issues.to_vec(),
            board: board.clone(),
            cached_at: chrono::Utc::now(),
        };
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, serde_json::to_vec(&value)?)?;
        set_private(&temp)?;
        std::fs::rename(&temp, &path)?;
        set_private(&path)?;
        Ok(())
    }
    pub fn is_expired(&self) -> bool {
        (chrono::Utc::now() - self.cached_at).num_hours() > EXPIRY_HOURS
    }
}

fn set_private(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_partitioned() {
        assert_ne!(CacheData::path("a", "1"), CacheData::path("b", "1"));
        assert_ne!(CacheData::path("a/b", "1"), CacheData::path("a_b", "1"));
        assert_ne!(CacheData::path("a", "b--c"), CacheData::path("a--b", "c"));
    }
}
