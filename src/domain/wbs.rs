use crate::domain::Issue;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct WbsNode {
    pub issue: Issue,
    pub children: Vec<WbsNode>,
    pub depth: usize,
    pub progress: f64, // 0..100 based on children done
}

pub fn build_wbs(issues: &[Issue], done_statuses: &[String]) -> Vec<WbsNode> {
    // Map key -> issue
    let mut by_key: HashMap<String, Issue> = HashMap::new();
    for iss in issues {
        by_key.insert(iss.key.clone(), iss.clone());
    }
    // parent -> children keys
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut orphan_keys = Vec::new();
    for iss in issues {
        if let Some(parent) = &iss.parent_key {
            if by_key.contains_key(parent) {
                children_map.entry(parent.clone()).or_default().push(iss.key.clone());
            } else {
                orphan_keys.push(iss.key.clone());
            }
        }
        // Epic link as parent fallback
        if let Some(epic) = &iss.epic_key {
            if iss.parent_key.is_none() && by_key.contains_key(epic) {
                children_map.entry(epic.clone()).or_default().push(iss.key.clone());
            }
        }
    }

    // Roots: issues without parent/epic or orphan
    let mut roots = Vec::new();
    let mut visited = HashSet::new();
    for iss in issues {
        let is_child = issues.iter().any(|p| {
            if let Some(children) = children_map.get(&p.key) {
                children.contains(&iss.key)
            } else {
                false
            }
        });
        if !is_child {
            if let Some(node) =
                build_node(iss, &by_key, &children_map, done_statuses, 0, &mut visited)
            {
                roots.push(node);
            }
        }
    }
    // Add orphans that were missed due to cycle
    for key in orphan_keys {
        if !visited.contains(&key) {
            if let Some(iss) = by_key.get(&key) {
                if let Some(node) =
                    build_node(iss, &by_key, &children_map, done_statuses, 0, &mut visited)
                {
                    roots.push(node);
                }
            }
        }
    }
    roots
}

fn build_node(
    issue: &Issue,
    by_key: &HashMap<String, Issue>,
    children_map: &HashMap<String, Vec<String>>,
    done_statuses: &[String],
    depth: usize,
    visited: &mut HashSet<String>,
) -> Option<WbsNode> {
    if !visited.insert(issue.key.clone()) {
        return None; // cycle guard
    }
    let mut children = Vec::new();
    if let Some(keys) = children_map.get(&issue.key) {
        for k in keys {
            if let Some(child_issue) = by_key.get(k) {
                if let Some(node) =
                    build_node(child_issue, by_key, children_map, done_statuses, depth + 1, visited)
                {
                    children.push(node);
                }
            }
        }
    }
    let progress = if children.is_empty() {
        if done_statuses.contains(&issue.status) {
            100.0
        } else {
            0.0
        }
    } else {
        let done = children
            .iter()
            .filter(|c| c.progress == 100.0 || done_statuses.contains(&c.issue.status))
            .count();
        done as f64 / children.len() as f64 * 100.0
    };
    Some(WbsNode { issue: issue.clone(), children, depth, progress })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::IssueType;

    fn issue(key: &str, parent: Option<&str>, epic: Option<&str>, status: &str) -> Issue {
        Issue {
            key: key.into(),
            summary: "s".into(),
            issue_type: IssueType::Task,
            status: status.into(),
            assignee: None,
            priority: None,
            due_date: None,
            updated: None,
            epic_key: epic.map(|s| s.into()),
            parent_key: parent.map(|s| s.into()),
            links: vec![],
            blocked: false,
            overdue: false,
        }
    }

    #[test]
    fn wbs_basic() {
        let issues = vec![
            issue("EPIC-1", None, None, "To Do"),
            issue("P-1", Some("EPIC-1"), None, "Done"),
            issue("P-2", Some("EPIC-1"), None, "To Do"),
        ];
        let roots = build_wbs(&issues, &["Done".into()]);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].children.len(), 2);
        assert!((roots[0].progress - 50.0).abs() < 1e-6);
    }

    #[test]
    fn wbs_cycle_guard() {
        let issues =
            vec![issue("A", Some("B"), None, "To Do"), issue("B", Some("A"), None, "To Do")];
        let roots = build_wbs(&issues, &["Done".into()]);
        // Should not panic, may be empty due to cycle
        assert!(roots.len() <= 2);
    }

    #[test]
    fn orphan_handling() {
        let issues = vec![issue("P-1", Some("MISSING"), None, "To Do")];
        let roots = build_wbs(&issues, &["Done".into()]);
        assert_eq!(roots.len(), 1);
    }
}
