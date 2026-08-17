use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::infrastructure::cli::Cli;

pub const CONFIG_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JiraAuth {
    CloudBasicApiToken,
    DataCenterBearerPat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JiraConfig {
    pub url: String,
    pub auth: JiraAuth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub board_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_command: Option<Vec<String>>,
}

impl JiraConfig {
    pub fn board_refs(&self) -> Vec<String> {
        self.board_ids.iter().map(ToString::to_string).collect()
    }

    pub fn keyring_service(&self) -> String {
        let host = url::Url::parse(&self.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "jira".into());
        format!("jira-kanban-tui/{host}")
    }

    pub fn keyring_user(&self) -> &str {
        match self.auth {
            JiraAuth::CloudBasicApiToken => self.username.as_deref().unwrap_or("cloud"),
            JiraAuth::DataCenterBearerPat => "data-center-pat",
        }
    }

    pub fn validate(&self) -> Result<()> {
        let parsed = url::Url::parse(&self.url).context("Jira URL is invalid")?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            anyhow::bail!("Jira URL must be an http(s) URL without embedded credentials");
        }
        if self.auth == JiraAuth::CloudBasicApiToken
            && self.username.as_deref().map(str::trim).unwrap_or_default().is_empty()
        {
            anyhow::bail!("Email is required for Jira Cloud");
        }
        if self.board_ids.is_empty() || self.board_ids.iter().any(|id| *id <= 0) {
            anyhow::bail!("at least one positive Jira Board ID is required");
        }
        let mut ids = std::collections::HashSet::new();
        if self.board_ids.iter().any(|id| !ids.insert(*id)) {
            anyhow::bail!("Jira Board IDs must be unique");
        }
        if self.token_env.as_deref().map(str::trim) == Some("") {
            anyhow::bail!("token_env must not be empty");
        }
        if let Some(command) = &self.token_command {
            if command.is_empty() || command[0].trim().is_empty() {
                anyhow::bail!("token_command must be a non-empty argv array");
            }
        }
        Ok(())
    }

    pub fn with_board_override(&self, board_id: i64) -> Result<Self> {
        if board_id <= 0 {
            anyhow::bail!("--board must be a positive Jira Board ID");
        }
        let mut jira = self.clone();
        jira.board_ids = vec![board_id];
        Ok(jira)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default = "config_version")]
    pub version: u32,
    pub jira: JiraConfig,
}

fn config_version() -> u32 {
    CONFIG_VERSION
}

impl Config {
    pub fn path(cli: &Cli) -> PathBuf {
        cli.config.clone().unwrap_or_else(|| {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".jira-kanban-tui.toml")
        })
    }

    pub fn load(cli: &Cli) -> Result<Option<Self>> {
        let path = Self::path(cli);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;

        let mut config = match toml::from_str::<Config>(&content) {
            Ok(config) if config.version == CONFIG_VERSION => config,
            _ => migrate_v2(&path, &content)?,
        };
        config.validate()?;
        if let Some(board_id) = cli.board {
            config.jira = config.jira.with_board_override(board_id)?;
        }
        Ok(Some(config))
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != CONFIG_VERSION {
            anyhow::bail!(
                "unsupported config version {}; expected {}",
                self.version,
                CONFIG_VERSION
            );
        }
        self.jira.validate()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("toml.tmp");
        std::fs::write(&temp, toml::to_string_pretty(self)?)?;
        set_private(&temp)?;
        std::fs::rename(&temp, path)?;
        set_private(path)
    }
}

#[derive(Debug, Deserialize)]
struct LegacyConfig {
    version: u32,
    sources: Vec<LegacySource>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
enum LegacySource {
    Jira {
        name: String,
        url: String,
        username: String,
        auth: JiraAuth,
        board_ids: Vec<i64>,
        #[serde(default)]
        token_env: Option<String>,
        #[serde(default)]
        token_command: Option<Vec<String>>,
    },
    Github {},
    Linear {},
}

fn migrate_v2(path: &Path, content: &str) -> Result<Config> {
    let legacy: LegacyConfig = toml::from_str(content).with_context(|| {
        "configuration is neither Config v3 nor a migratable Config v2; run with a new --config path to start Setup"
    })?;
    if legacy.version != 2 {
        anyhow::bail!("unsupported config version {}; expected 3", legacy.version);
    }
    if legacy.sources.len() != 1 {
        anyhow::bail!(
            "Config v2 migration requires exactly one Jira source; split each Jira connection into a separate Config v3 file"
        );
    }
    let source = legacy.sources.into_iter().next().expect("length checked");
    let LegacySource::Jira { name, url, username, auth, board_ids, token_env, token_command } =
        source
    else {
        anyhow::bail!(
            "Config v3 is Jira-only; the existing GitHub/Linear source was left unchanged"
        );
    };
    let jira = JiraConfig {
        url,
        username: (auth == JiraAuth::CloudBasicApiToken).then_some(username),
        auth,
        board_ids,
        token_env,
        token_command,
    };
    jira.validate()?;
    let config = Config { version: CONFIG_VERSION, jira };

    let backup = backup_path(path);
    if !backup.exists() {
        std::fs::write(&backup, content)?;
        set_private(&backup)?;
    }
    migrate_keyring(&name, &config.jira);
    config.save(path)?;
    Ok(config)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".v2.bak");
    PathBuf::from(value)
}

fn migrate_keyring(old_name: &str, jira: &JiraConfig) {
    let Ok(old) = keyring::Entry::new(&format!("jira-kanban-tui/{old_name}"), old_name) else {
        return;
    };
    let Ok(secret) = old.get_password() else { return };
    if let Ok(new) = keyring::Entry::new(&jira.keyring_service(), jira.keyring_user()) {
        let _ = new.set_password(&secret);
    }
}

fn set_private(path: &Path) -> Result<()> {
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
    use tempfile::tempdir;

    fn config() -> Config {
        Config {
            version: CONFIG_VERSION,
            jira: JiraConfig {
                url: "https://example.atlassian.net".into(),
                auth: JiraAuth::CloudBasicApiToken,
                username: Some("alice@example.com".into()),
                board_ids: vec![42, 99],
                token_env: Some("JIRA_TOKEN".into()),
                token_command: None,
            },
        }
    }

    #[test]
    fn save_roundtrip_does_not_store_token() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        config().save(&path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("token ="));
        assert_eq!(toml::from_str::<Config>(&content).unwrap(), config());
    }

    #[test]
    fn data_center_does_not_require_username() {
        let mut config = config();
        config.jira.auth = JiraAuth::DataCenterBearerPat;
        config.jira.username = None;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn migrates_single_jira_v2_and_keeps_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let legacy = r#"version = 2
active_source = "work"
[[sources]]
backend = "jira"
name = "work"
url = "https://example.atlassian.net"
username = "alice@example.com"
auth = "cloud_basic_api_token"
board_ids = [42]
"#;
        std::fs::write(&path, legacy).unwrap();
        let cli = Cli {
            board: None,
            config: Some(path.clone()),
            doctor: false,
            command: None,
            verbose: 0,
        };
        let migrated = Config::load(&cli).unwrap().unwrap();
        assert_eq!(migrated.version, 3);
        assert_eq!(migrated.jira.board_ids, vec![42]);
        assert!(backup_path(&path).exists());
    }

    #[test]
    fn refuses_mixed_v2_without_overwriting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let legacy = r#"version = 2
active_source = "work"
[[sources]]
backend = "jira"
name = "work"
url = "https://example.atlassian.net"
username = "alice@example.com"
auth = "cloud_basic_api_token"
board_ids = [42]
[[sources]]
backend = "github"
name = "repo"
owner = "acme"
repo = "app"
"#;
        std::fs::write(&path, legacy).unwrap();
        let cli = Cli {
            board: None,
            config: Some(path.clone()),
            doctor: false,
            command: None,
            verbose: 0,
        };
        assert!(Config::load(&cli).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), legacy);
        assert!(!backup_path(&path).exists());
    }
}
