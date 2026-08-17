use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BoardDto {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub board_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BoardConfigurationDto {
    #[serde(rename = "columnConfig")]
    pub column_config: ColumnConfigDto,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ColumnConfigDto {
    pub columns: Vec<ColumnDto>,
    #[serde(rename = "constraintType")]
    pub constraint_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ColumnDto {
    pub name: String,
    pub statuses: Vec<StatusDto>,
    pub min: Option<i32>,
    pub max: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StatusDto {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "self")]
    pub self_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchResultDto {
    #[serde(default)]
    pub expand: Option<String>,
    #[serde(rename = "startAt")]
    pub start_at: i64,
    #[serde(rename = "maxResults")]
    pub max_results: i64,
    pub total: i64,
    pub issues: Vec<IssueDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueDto {
    pub id: String,
    pub key: String,
    #[serde(rename = "self")]
    pub self_url: String,
    pub fields: IssueFieldsDto,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueFieldsDto {
    pub summary: String,
    #[serde(rename = "issuetype")]
    pub issue_type: IssueTypeDto,
    pub status: StatusDetailDto,
    pub assignee: Option<UserDto>,
    pub priority: Option<PriorityDto>,
    #[serde(rename = "duedate")]
    pub due_date: Option<String>,
    pub updated: Option<String>,
    pub parent: Option<ParentDto>,
    #[serde(default)]
    pub issuelinks: Vec<IssueLinkDto>,
    #[serde(default)]
    pub subtasks: Vec<SubtaskDto>,
    pub epic: Option<EpicDto>,
    #[serde(rename = "customfield_10014")]
    pub epic_link: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueTypeDto {
    pub id: String,
    pub name: String,
    pub subtask: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StatusDetailDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "statusCategory")]
    pub status_category: Option<StatusCategoryDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StatusCategoryDto {
    pub id: i64,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserDto {
    #[serde(rename = "accountId")]
    pub account_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PriorityDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ParentDto {
    pub id: String,
    pub key: String,
    pub fields: Option<ParentFieldsDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ParentFieldsDto {
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueLinkDto {
    pub id: String,
    #[serde(rename = "type")]
    pub link_type: LinkTypeDto,
    #[serde(rename = "outwardIssue")]
    pub outward_issue: Option<LinkedIssueDto>,
    #[serde(rename = "inwardIssue")]
    pub inward_issue: Option<LinkedIssueDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LinkTypeDto {
    pub id: String,
    pub name: String,
    pub inward: String,
    pub outward: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LinkedIssueDto {
    pub id: String,
    pub key: String,
    #[serde(rename = "self")]
    pub self_url: String,
    pub fields: Option<LinkedIssueFieldsDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LinkedIssueFieldsDto {
    pub summary: Option<String>,
    pub status: Option<StatusDetailDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SubtaskDto {
    pub id: String,
    pub key: String,
    pub fields: Option<SubtaskFieldsDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SubtaskFieldsDto {
    pub summary: Option<String>,
    pub status: Option<StatusDetailDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EpicDto {
    pub id: String,
    pub key: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransitionsDto {
    pub transitions: Vec<TransitionDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransitionDto {
    pub id: String,
    pub name: String,
    pub to: Option<StatusDetailDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PrioritiesDto(pub Vec<PriorityDto>);

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UsersDto(pub Vec<UserDto>);
