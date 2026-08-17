use chrono::NaiveDate;

use crate::domain::{activity::Activity, Board, Issue};
use crate::infrastructure::config::{JiraAuth, JiraConfig};
use crate::jira::{JiraClient, JiraError};

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateCommand {
    Transition { transition_id: String },
    Assignee { account_id: Option<String> },
    DueDate { value: Option<NaiveDate> },
    Priority { id: String },
}

impl UpdateCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Transition { .. } => "Status",
            Self::Assignee { .. } => "Assignee",
            Self::DueDate { .. } => "Due date",
            Self::Priority { .. } => "Priority",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionOption {
    pub id: String,
    pub name: String,
    pub target_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

pub struct JiraService {
    pub client: JiraClient,
    auth: JiraAuth,
    board_ids: Vec<i64>,
    blocked_statuses: std::sync::RwLock<Vec<String>>,
}

impl JiraService {
    pub fn new(config: &JiraConfig, token: String) -> Result<Self, JiraError> {
        let client = match config.auth {
            JiraAuth::CloudBasicApiToken => JiraClient::new_with_basic_auth(
                config.url.clone(),
                config.username.clone().unwrap_or_default(),
                token,
            )?,
            JiraAuth::DataCenterBearerPat => JiraClient::new(config.url.clone(), token)?,
        };
        Ok(Self {
            client,
            auth: config.auth.clone(),
            board_ids: config.board_ids.clone(),
            blocked_statuses: std::sync::RwLock::new(Vec::new()),
        })
    }

    pub fn board_refs(&self) -> Vec<String> {
        self.board_ids.iter().map(ToString::to_string).collect()
    }

    pub fn issue_url(&self, key: &str) -> String {
        format!("{}/browse/{key}", self.client.base_url)
    }

    pub async fn inspect_board(&self, board_ref: &str) -> Result<Board, JiraError> {
        let id = parse_board_id(board_ref)?;
        let (board, config) =
            tokio::try_join!(self.client.get_board(id), self.client.get_board_configuration(id))?;
        Ok(crate::domain::mapping::map_board(board, config))
    }

    pub async fn load_board(&self, board_ref: &str) -> Result<Board, JiraError> {
        let mapped = self.inspect_board(board_ref).await?;
        let blocked = mapped
            .columns
            .iter()
            .filter(|column| column.name.to_lowercase().contains("block"))
            .flat_map(|column| column.statuses.clone())
            .collect();
        *self.blocked_statuses.write().unwrap_or_else(|error| error.into_inner()) = blocked;
        Ok(mapped)
    }

    pub async fn load_issues(&self, board_ref: &str) -> Result<Vec<Issue>, JiraError> {
        let id = parse_board_id(board_ref)?;
        let board = self.load_board(board_ref).await?;
        let blocked: Vec<String> = board
            .columns
            .iter()
            .filter(|column| column.name.to_lowercase().contains("block"))
            .flat_map(|column| column.statuses.clone())
            .collect();
        Ok(self
            .client
            .get_board_issues(id)
            .await?
            .into_iter()
            .map(|dto| crate::domain::mapping::map_issue(dto, &blocked))
            .collect())
    }

    pub async fn load_board_and_issues(
        &self,
        board_ref: &str,
    ) -> Result<(Board, Vec<Issue>), JiraError> {
        let board = self.load_board(board_ref).await?;
        let id = parse_board_id(board_ref)?;
        let blocked: Vec<String> = board
            .columns
            .iter()
            .filter(|column| column.name.to_lowercase().contains("block"))
            .flat_map(|column| column.statuses.clone())
            .collect();
        let issues = self
            .client
            .get_board_issues(id)
            .await?
            .into_iter()
            .map(|dto| crate::domain::mapping::map_issue(dto, &blocked))
            .collect();
        Ok((board, issues))
    }

    pub async fn get_issue(&self, key: &str) -> Result<Issue, JiraError> {
        let dto = self.client.get_issue(key).await?;
        let blocked = self.blocked_statuses.read().unwrap_or_else(|error| error.into_inner());
        Ok(crate::domain::mapping::map_issue(dto, &blocked))
    }

    pub async fn transitions(&self, key: &str) -> Result<Vec<TransitionOption>, JiraError> {
        Ok(self
            .client
            .get_transitions(key)
            .await?
            .transitions
            .into_iter()
            .map(|transition| TransitionOption {
                target_status: transition
                    .to
                    .map(|status| status.name)
                    .unwrap_or_else(|| transition.name.clone()),
                id: transition.id,
                name: transition.name,
            })
            .collect())
    }

    pub async fn update(&self, key: &str, command: UpdateCommand) -> Result<(), JiraError> {
        match command {
            UpdateCommand::Transition { transition_id } => {
                self.client.do_transition(key, &transition_id).await
            }
            UpdateCommand::Assignee { account_id } => {
                self.client
                    .assign_issue(
                        key,
                        account_id.as_deref(),
                        self.auth == JiraAuth::DataCenterBearerPat,
                    )
                    .await
            }
            UpdateCommand::DueDate { value } => {
                let value = value.map(|date| date.format("%Y-%m-%d").to_string());
                self.client.set_due_date(key, value.as_deref()).await
            }
            UpdateCommand::Priority { id } => self.client.set_priority(key, &id).await,
        }
    }

    pub async fn activity(
        &self,
        board_ref: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Activity>, JiraError> {
        self.client.get_board_activity(parse_board_id(board_ref)?, since).await
    }

    pub async fn assignees(&self, query: &str) -> Result<Vec<Choice>, JiraError> {
        Ok(self
            .client
            .search_users(query)
            .await?
            .into_iter()
            .filter_map(|user| {
                let id = user.account_id.or(user.name)?;
                Some(Choice {
                    id,
                    label: user.display_name.unwrap_or_else(|| "Unknown user".into()),
                })
            })
            .collect())
    }

    pub async fn priorities(&self) -> Result<Vec<Choice>, JiraError> {
        Ok(self
            .client
            .get_priorities()
            .await?
            .into_iter()
            .map(|priority| Choice { id: priority.id, label: priority.name })
            .collect())
    }

    pub async fn viewer(&self) -> Result<Choice, JiraError> {
        self.client.current_user().await
    }
}

fn parse_board_id(value: &str) -> Result<i64, JiraError> {
    value.parse().map_err(|_| JiraError::Validation("invalid Jira Board ID".into()))
}
