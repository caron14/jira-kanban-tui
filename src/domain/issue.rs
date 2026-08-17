use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignee {
    pub display_name: String,
    pub account_id: Option<String>,
}

/// Priority with order for sorting/display
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Priority {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueType {
    Story,
    Task,
    Bug,
    SubTask,
    Epic,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueLink {
    pub link_type: String,
    pub outward_issue: Option<String>,
    pub inward_issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub issue_type: IssueType,
    pub status: String,
    pub assignee: Option<Assignee>,
    pub priority: Option<Priority>,
    pub due_date: Option<NaiveDate>,
    pub updated: Option<chrono::DateTime<chrono::Utc>>,
    pub epic_key: Option<String>,
    pub parent_key: Option<String>,
    pub links: Vec<IssueLink>,
    pub blocked: bool,
    pub overdue: bool,
}

impl Issue {
    /// Case-insensitive search predicate: key, summary, assignee
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        self.key.to_lowercase().contains(&q)
            || self.summary.to_lowercase().contains(&q)
            || self
                .assignee
                .as_ref()
                .map(|a| a.display_name.to_lowercase().contains(&q))
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_issue() -> Issue {
        Issue {
            key: "PROJ-123".to_string(),
            summary: "Fix critical bug".to_string(),
            issue_type: IssueType::Bug,
            status: "To Do".to_string(),
            assignee: Some(Assignee { display_name: "Alice".to_string(), account_id: None }),
            priority: None,
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: false,
            overdue: false,
        }
    }

    #[test]
    fn matches_query_case_insensitive() {
        let issue = sample_issue();
        assert!(issue.matches_query("alice"));
        assert!(issue.matches_query("ALICE"));
        assert!(issue.matches_query("proj-123"));
        assert!(issue.matches_query("critical"));
        assert!(!issue.matches_query("bob"));
    }
}
