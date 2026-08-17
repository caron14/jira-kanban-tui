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
impl TokenProvider for CommandProvider {
    fn name(&self) -> &str {
        "external command"
    }
    fn get_token(&self) -> Result<Option<String>> {
        let (program, args) = self.0.split_first().context("empty token command")?;
        let output = std::process::Command::new(program).args(args).output()?;
        if !output.status.success() {
            anyhow::bail!("token command exited with {}", output.status);
        }
        let value = String::from_utf8(output.stdout)?.trim().to_string();
        Ok((!value.is_empty()).then_some(value))
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
