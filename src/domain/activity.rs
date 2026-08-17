use crate::domain::Issue;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Status,
    Assignee,
    DueDate,
    Priority,
    Created,
    Completed,
}

#[derive(Debug, Clone)]
pub struct Activity {
    pub key: String,
    pub summary: String,
    pub kind: ChangeKind,
    pub from: Option<String>,
    pub to: Option<String>,
    pub at: DateTime<Utc>,
}

pub fn collect_activity(issues: &[Issue], since: DateTime<Utc>) -> Vec<Activity> {
    let mut acts = Vec::new();
    for iss in issues {
        if let Some(updated) = iss.updated {
            if updated < since {
                continue;
            }
            // Heuristic: if status == Done and updated is recent, treat as Completed
            if iss.status.to_lowercase().contains("done")
                || iss.status.to_lowercase().contains("closed")
            {
                acts.push(Activity {
                    key: iss.key.clone(),
                    summary: iss.summary.clone(),
                    kind: ChangeKind::Completed,
                    from: None,
                    to: Some(iss.status.clone()),
                    at: updated,
                });
            } else {
                acts.push(Activity {
                    key: iss.key.clone(),
                    summary: iss.summary.clone(),
                    kind: ChangeKind::Status,
                    from: None,
                    to: Some(iss.status.clone()),
                    at: updated,
                });
            }
            // Due date change is not tracked without changelog; placeholder
        }
    }
    // Sort by time, stable for same timestamp
    acts.sort_by_key(|item| std::cmp::Reverse(item.at));
    acts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Issue, IssueType};
    use chrono::{Duration, Utc};

    #[test]
    fn activity_filters_since() {
        let now = Utc::now();
        let issues = vec![
            Issue {
                key: "P-1".into(),
                summary: "a".into(),
                issue_type: IssueType::Task,
                status: "Done".into(),
                assignee: None,
                priority: None,
                due_date: None,
                updated: Some(now),
                epic_key: None,
                parent_key: None,
                links: vec![],
                blocked: false,
                overdue: false,
            },
            Issue {
                key: "P-2".into(),
                summary: "b".into(),
                issue_type: IssueType::Task,
                status: "To Do".into(),
                assignee: None,
                priority: None,
                due_date: None,
                updated: Some(now - Duration::days(5)),
                epic_key: None,
                parent_key: None,
                links: vec![],
                blocked: false,
                overdue: false,
            },
        ];
        let acts = collect_activity(&issues, now - Duration::days(1));
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].key, "P-1");
    }
}
