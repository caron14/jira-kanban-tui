use crate::infrastructure::config::JiraConfig;
use anyhow::{Context, Result};

pub trait TokenProvider: Send + Sync {
    fn name(&self) -> &str;
    fn get_token(&self) -> Result<Option<String>>;
}

struct EnvProvider(String);
impl TokenProvider for EnvProvider {
    fn name(&self) -> &str {
        "environment"
    }
    fn get_token(&self) -> Result<Option<String>> {
        Ok(std::env::var(&self.0).ok().filter(|value| !value.is_empty()))
    }
}

struct KeyringProvider {
    service: String,
    user: String,
}
impl TokenProvider for KeyringProvider {
    fn name(&self) -> &str {
        "keyring"
    }
    fn get_token(&self) -> Result<Option<String>> {
        let entry = keyring::Entry::new(&self.service, &self.user)?;
        match entry.get_password() {
            Ok(value) => Ok((!value.is_empty()).then_some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

struct CommandProvider(Vec<String>);
impl CommandProvider {
    fn get_token_with_timeout(&self, timeout: std::time::Duration) -> Result<Option<String>> {
        use std::process::Stdio;

        let (program, args) = self.0.split_first().context("empty token command")?;
        let mut child = std::process::Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if child.try_wait()?.is_some() {
                let output = child.wait_with_output()?;
                if !output.status.success() {
                    anyhow::bail!("token command exited with {}", output.status);
                }
                let value = String::from_utf8(output.stdout)?.trim().to_string();
                return Ok((!value.is_empty()).then_some(value));
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("token command timed out after {} seconds", timeout.as_secs());
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

impl TokenProvider for CommandProvider {
    fn name(&self) -> &str {
        "external command"
    }
    fn get_token(&self) -> Result<Option<String>> {
        self.get_token_with_timeout(std::time::Duration::from_secs(30))
    }
}

pub fn build_providers(jira: &JiraConfig) -> Vec<Box<dyn TokenProvider>> {
    let mut providers: Vec<Box<dyn TokenProvider>> = vec![Box::new(KeyringProvider {
        service: jira.keyring_service(),
        user: jira.keyring_user().into(),
    })];
    if let Some(name) = &jira.token_env {
        providers.push(Box::new(EnvProvider(name.clone())));
    }
    if let Some(command) = &jira.token_command {
        providers.push(Box::new(CommandProvider(command.clone())));
    }
    providers
}

pub fn resolve_token(providers: &[Box<dyn TokenProvider>]) -> Result<Option<(String, String)>> {
    let mut failures = Vec::new();
    for provider in providers {
        match provider.get_token() {
            Ok(Some(value)) => return Ok(Some((provider.name().into(), value))),
            Ok(None) => {}
            Err(error) => failures.push(format!("{}: {error:#}", provider.name())),
        }
    }
    if failures.is_empty() {
        Ok(None)
    } else {
        anyhow::bail!(failures.join("; "))
    }
}

pub fn save_to_keyring(jira: &JiraConfig, token: &str) -> Result<()> {
    keyring::Entry::new(&jira.keyring_service(), jira.keyring_user())?.set_password(token)?;
    Ok(())
}

pub fn redact_token(value: &str, token: &str) -> String {
    if token.is_empty() {
        value.into()
    } else {
        value.replace(token, "***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_command_is_terminated_after_timeout() {
        let provider = CommandProvider(vec!["sleep".into(), "1".into()]);
        let started = std::time::Instant::now();
        let error =
            provider.get_token_with_timeout(std::time::Duration::from_millis(30)).unwrap_err();

        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
