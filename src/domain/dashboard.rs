use crate::domain::Issue;
use chrono::Local;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    pub total: usize,
    pub open: usize,
    pub in_progress: usize,
    pub done: usize,
    pub blocked: usize,
    pub overdue: usize,
    pub due_today: usize,
    pub due_this_week: usize,
    pub stale: usize,
    pub unassigned: usize,
    pub no_due_date: usize,
    pub high_priority: usize,
    pub progress_pct: f64, // done / total (issue count)
}

#[derive(Debug, Clone)]
pub struct Workload {
    pub assignee: String,
    pub todo: usize,
    pub doing: usize,
    pub blocked: usize,
    pub overdue: usize,
    pub total: usize,
}

pub fn compute_stats(
    issues: &[Issue],
    done_statuses: &[String],
    in_progress_statuses: &[String],
) -> DashboardStats {
    let today = Local::now().date_naive();
    let week_end = today + chrono::Duration::days(6);

    let mut s = DashboardStats { total: issues.len(), ..Default::default() };
    for iss in issues {
        if done_statuses.contains(&iss.status) {
            s.done += 1;
        } else if in_progress_statuses.contains(&iss.status) {
            s.in_progress += 1;
        } else {
            s.open += 1;
        }
        if iss.blocked {
            s.blocked += 1;
        }
        if crate::domain::filter::is_overdue(iss) {
            s.overdue += 1;
        }
        if let Some(due) = iss.due_date {
            if due == today {
                s.due_today += 1;
            }
            if due >= today && due <= week_end {
                s.due_this_week += 1;
            }
        } else {
            s.no_due_date += 1;
        }
        if iss.assignee.is_none() {
            s.unassigned += 1;
        }
        if iss.priority.as_ref().map(|p| p.name == "Highest" || p.name == "High").unwrap_or(false) {
            s.high_priority += 1;
        }
        // Stale: updated > threshold ago (default 3d for InProgress, 2d for Blocked)
        if let Some(updated) = iss.updated {
            let days = (Local::now().with_timezone(&chrono::Utc) - updated).num_days();
            let threshold = if iss.blocked {
                2
            } else if in_progress_statuses.contains(&iss.status) {
                3
            } else {
                7
            };
            if days >= threshold {
                s.stale += 1;
            }
        }
    }
    if s.total > 0 {
        s.progress_pct = s.done as f64 / s.total as f64 * 100.0;
    }
    s
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttentionKind {
    Overdue,
    Blocked,
    DueToday,
    DueSoon,
    Stale,
    Unassigned,
    NoDueDate,
}

#[derive(Debug, Clone)]
pub struct AttentionItem {
    pub issue: Issue,
    pub kind: AttentionKind,
}

pub fn attention_sorted(
    issues: &[Issue],
    done_statuses: &[String],
    in_progress: &[String],
) -> Vec<AttentionItem> {
    let today = Local::now().date_naive();
    let mut items = Vec::new();
    for iss in issues {
        if done_statuses.contains(&iss.status) {
            continue;
        }
        let stale = if let Some(updated) = iss.updated {
            let days = (Local::now().with_timezone(&chrono::Utc) - updated).num_days();
            let threshold = if iss.blocked {
                2
            } else if in_progress.contains(&iss.status) {
                3
            } else {
                7
            };
            days >= threshold
        } else {
            false
        };
        let kind = if crate::domain::filter::is_overdue(iss) {
            AttentionKind::Overdue
        } else if iss.blocked {
            AttentionKind::Blocked
        } else if iss.due_date == Some(today) {
            AttentionKind::DueToday
        } else if iss
            .due_date
            .map(|d| d >= today && d <= today + chrono::Duration::days(3))
            .unwrap_or(false)
        {
            AttentionKind::DueSoon
        } else if stale {
            AttentionKind::Stale
        } else if iss.assignee.is_none() {
            AttentionKind::Unassigned
        } else if iss.due_date.is_none() {
            AttentionKind::NoDueDate
        } else {
            continue;
        };
        items.push(AttentionItem { issue: iss.clone(), kind });
    }
    // Sort by priority order: Overdue > Blocked > DueToday > DueSoon > Stale > Unassigned > NoDueDate
    let order = |k: &AttentionKind| match k {
        AttentionKind::Overdue => 0,
        AttentionKind::Blocked => 1,
        AttentionKind::DueToday => 2,
        AttentionKind::DueSoon => 3,
        AttentionKind::Stale => 4,
        AttentionKind::Unassigned => 5,
        AttentionKind::NoDueDate => 6,
    };
    items.sort_by_key(|a| order(&a.kind));
    items
}

pub fn workload_by_assignee(issues: &[Issue]) -> Vec<Workload> {
    let mut map: HashMap<String, Workload> = HashMap::new();
    for iss in issues {
        let key = iss
            .assignee
            .as_ref()
            .map(|a| a.display_name.clone())
            .unwrap_or_else(|| "(Unassigned)".into());
        let e = map.entry(key.clone()).or_insert(Workload {
            assignee: key,
            todo: 0,
            doing: 0,
            blocked: 0,
            overdue: 0,
            total: 0,
        });
        e.total += 1;
        if iss.blocked {
            e.blocked += 1;
        }
        if crate::domain::filter::is_overdue(iss) {
            e.overdue += 1;
        }
        // Heuristic: status contains "progress" => doing else todo/done
        if iss.status.to_lowercase().contains("progress")
            || iss.status.to_lowercase().contains("doing")
        {
            e.doing += 1;
        } else if iss.status.to_lowercase().contains("done")
            || iss.status.to_lowercase().contains("closed")
        {
        } else {
            e.todo += 1;
        }
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by_key(|item| std::cmp::Reverse(item.total));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Assignee, IssueType, Priority};
    use chrono::{NaiveDate, Utc};

    fn issue_with(
        status: &str,
        due: Option<NaiveDate>,
        blocked: bool,
        assignee: Option<&str>,
        updated_days_ago: Option<i64>,
    ) -> Issue {
        Issue {
            key: "P-1".into(),
            summary: "s".into(),
            issue_type: IssueType::Task,
            status: status.into(),
            assignee: assignee.map(|n| Assignee { display_name: n.into(), account_id: None }),
            priority: Some(Priority { name: "High".into(), id: "2".into() }),
            due_date: due,
            updated: updated_days_ago.map(|d| Utc::now() - chrono::Duration::days(d)),
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked,
            overdue: due.map(|d| d < Local::now().date_naive()).unwrap_or(false),
        }
    }

    #[test]
    fn stats_counts() {
        let today = Local::now().date_naive();
        let issues = vec![
            issue_with("To Do", Some(today), false, Some("Alice"), Some(0)),
            issue_with("Blocked", None, true, None, Some(10)),
            issue_with("Done", None, false, Some("Bob"), Some(0)),
        ];
        let s = compute_stats(&issues, &["Done".into()], &["In Progress".into(), "Blocked".into()]);
        assert_eq!(s.total, 3);
        assert_eq!(s.done, 1);
        assert_eq!(s.blocked, 1);
        assert_eq!(s.unassigned, 1);
        assert_eq!(s.due_today, 1);
    }

    #[test]
    fn attention_order() {
        let today = Local::now().date_naive();
        let overdue =
            issue_with("To Do", Some(today - chrono::Duration::days(1)), false, Some("A"), Some(0));
        let blocked = issue_with("Blocked", None, true, Some("B"), Some(0));
        let due_today = issue_with("To Do", Some(today), false, Some("C"), Some(0));
        let items = attention_sorted(
            &[due_today.clone(), blocked.clone(), overdue.clone()],
            &["Done".into()],
            &["Blocked".into()],
        );
        assert_eq!(items[0].kind, AttentionKind::Overdue);
        assert_eq!(items[1].kind, AttentionKind::Blocked);
        assert_eq!(items[2].kind, AttentionKind::DueToday);
    }

    #[test]
    fn workload_grouping() {
        let issues = vec![
            issue_with("To Do", None, false, Some("Alice"), None),
            issue_with("In Progress", None, false, Some("Alice"), None),
            issue_with("To Do", None, false, None, None),
        ];
        let w = workload_by_assignee(&issues);
        assert_eq!(w.len(), 2);
        assert!(w.iter().any(|x| x.assignee == "Alice" && x.total == 2));
    }
}
