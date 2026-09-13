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
        Self::load_from_path(&Self::path(account, board_ref), account, board_ref, allow_stale)
    }
    fn load_from_path(
        path: &Path,
        account: &str,
        board_ref: &str,
        allow_stale: bool,
    ) -> Option<Self> {
        let data = std::fs::read_to_string(path).ok()?;
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
        Self::save_to_path(&path, account, board_ref, board, issues)
    }
    fn save_to_path(
        path: &Path,
        account: &str,
        board_ref: &str,
        board: &crate::domain::Board,
        issues: &[crate::domain::Issue],
    ) -> anyhow::Result<()> {
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
        std::fs::rename(&temp, path)?;
        set_private(path)?;
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
    use crate::domain::{BoardColumn, Issue, IssueType};

    #[test]
    fn paths_are_partitioned() {
        assert_ne!(CacheData::path("a", "1"), CacheData::path("b", "1"));
        assert_ne!(CacheData::path("a/b", "1"), CacheData::path("a_b", "1"));
        assert_ne!(CacheData::path("a", "b--c"), CacheData::path("a--b", "c"));
    }

    #[test]
    fn successful_edit_is_present_after_cache_reload() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("board.json");
        let board = crate::domain::Board {
            id: 1,
            name: "Board".into(),
            columns: vec![BoardColumn {
                name: "In Progress".into(),
                statuses: vec!["In Progress".into()],
            }],
        };
        let mut issue = Issue {
            key: "P-1".into(),
            summary: "Edited issue".into(),
            issue_type: IssueType::Task,
            status: "To Do".into(),
            assignee: None,
            priority: None,
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: false,
            overdue: false,
        };

        CacheData::save_to_path(&path, "account", "1", &board, &[issue.clone()]).unwrap();
        issue.status = "In Progress".into();
        issue.due_date = chrono::NaiveDate::from_ymd_opt(2026, 9, 30);
        CacheData::save_to_path(&path, "account", "1", &board, &[issue]).unwrap();
        let reloaded = CacheData::load_from_path(&path, "account", "1", true).unwrap();

        assert_eq!(reloaded.issues[0].status, "In Progress");
        assert_eq!(reloaded.issues[0].due_date, chrono::NaiveDate::from_ymd_opt(2026, 9, 30));
    }
}
