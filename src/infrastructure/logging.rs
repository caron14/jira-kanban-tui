use crate::infrastructure::cli::Cli;
use tracing_subscriber::{fmt, EnvFilter};

pub fn init(cli: &Cli) {
    let filter = if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::from_default_env()
    } else if cli.verbose >= 2 {
        EnvFilter::new("debug")
    } else if cli.verbose == 1 {
        EnvFilter::new("info")
    } else {
        EnvFilter::new("warn")
    };

    let log_path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".jira-kanban-tui.log");

    // Credentials are represented only inside sensitive HTTP headers and are never
    // passed to tracing fields. The writer is deliberately a plain file writer.
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();

    #[cfg(unix)]
    if file.is_some() {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o600));
    }

    if let Some(file) = file {
        let _ = fmt()
            .with_env_filter(filter)
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .try_init();
    } else {
        let _ = fmt().with_env_filter(filter).try_init();
    }
}

/// Redact token from any string before logging
pub fn redact(s: &str, token: &str) -> String {
    if token.is_empty() {
        return s.to_string();
    }
    s.replace(token, "***")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redact_hides_token() {
        assert_eq!(redact("Bearer s3cr3t", "s3cr3t"), "Bearer ***");
        assert_eq!(redact("no secret", "s3cr3t"), "no secret");
    }
}
