use crate::domain::Issue;
use chrono::Local;
use serde::{Deserialize, Serialize};

pub fn is_blocked(issue: &Issue) -> bool {
    issue.blocked
}

pub fn is_overdue(issue: &Issue) -> bool {
    if let Some(due) = issue.due_date {
        let today = Local::now().date_naive();
        due < today
    } else {
        false
    }
}

pub fn is_my_issue(issue: &Issue, username: &str) -> bool {
    issue
        .assignee
        .as_ref()
        .map(|assignee| {
            assignee.display_name.eq_ignore_ascii_case(username)
                || assignee
                    .account_id
                    .as_deref()
                    .map(|id| id.eq_ignore_ascii_case(username))
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInFilter {
    MyIssues,
    Overdue,
    Blocked,
}

impl BuiltInFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MyIssues => "My Issues",
            Self::Overdue => "Overdue",
            Self::Blocked => "Blocked",
        }
    }

    pub fn matches(&self, issue: &Issue, username: Option<&str>) -> bool {
        match self {
            BuiltInFilter::MyIssues => username.map(|u| is_my_issue(issue, u)).unwrap_or(false),
            BuiltInFilter::Overdue => is_overdue(issue),
            BuiltInFilter::Blocked => is_blocked(issue),
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::MyIssues, Self::Overdue, Self::Blocked]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Assignee, IssueType};

    #[test]
    fn my_issues_matches_backend_account_id() {
        let issue = Issue {
            key: "P-1".into(),
            summary: "work".into(),
            issue_type: IssueType::Task,
            status: "To Do".into(),
            assignee: Some(Assignee {
                display_name: "Alice Example".into(),
                account_id: Some("account-123".into()),
            }),
            priority: None,
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: false,
            overdue: false,
        };
        assert!(is_my_issue(&issue, "account-123"));
        assert!(is_my_issue(&issue, "alice example"));
        assert!(!is_my_issue(&issue, "someone-else"));
    }
}
