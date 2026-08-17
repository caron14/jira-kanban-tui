use crate::domain::board::{Board, BoardColumn};
use crate::domain::issue::{Assignee, Issue, IssueLink, IssueType, Priority};
use crate::jira::dto;
use chrono::{NaiveDate, Utc};

pub fn map_board(dto: dto::BoardDto, config: dto::BoardConfigurationDto) -> Board {
    let columns = config
        .column_config
        .columns
        .into_iter()
        .map(|c| BoardColumn {
            name: c.name,
            statuses: c.statuses.into_iter().filter_map(|s| s.name).collect(),
        })
        .collect();
    Board { id: dto.id, name: dto.name, columns }
}

pub fn map_issue(dto: dto::IssueDto, blocked_statuses: &[String]) -> Issue {
    let f = dto.fields;
    let issue_type = match f.issue_type.name.as_str() {
        "Story" => IssueType::Story,
        "Task" => IssueType::Task,
        "Bug" => IssueType::Bug,
        "Epic" => IssueType::Epic,
        _ if f.issue_type.subtask => IssueType::SubTask,
        other => IssueType::Other(other.to_string()),
    };

    let assignee = f.assignee.and_then(|u| {
        let name = u.display_name.or(u.name).or(u.email_address)?;
        Some(Assignee { display_name: name, account_id: u.account_id })
    });

    let priority = f.priority.map(|p| Priority { name: p.name, id: p.id });

    let due_date = f.due_date.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());

    let updated = f.updated.and_then(|u| {
        // Jira returns like "2024-01-02T03:04:05.000+0900"
        chrono::DateTime::parse_from_str(&u, "%Y-%m-%dT%H:%M:%S%.3f%z")
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| {
                chrono::DateTime::parse_from_rfc3339(&u).ok().map(|dt| dt.with_timezone(&Utc))
            })
    });

    let links = f
        .issuelinks
        .into_iter()
        .map(|l| IssueLink {
            link_type: l.link_type.name,
            outward_issue: l.outward_issue.map(|o| o.key),
            inward_issue: l.inward_issue.map(|i| i.key),
        })
        .collect::<Vec<_>>();

    // Blocked: status in Blocked column OR link type contains blocks/is blocked by
    let status_blocked = blocked_statuses.contains(&f.status.name);
    let link_blocked = links.iter().any(|l| {
        let t = l.link_type.to_lowercase();
        t.contains("blocks") || t.contains("blocked")
    });
    let blocked = status_blocked || link_blocked;

    // Overdue computed with local date (domain::filter will recompute, but store initial)
    let overdue = if let Some(due) = due_date {
        let today = chrono::Local::now().date_naive();
        due < today
    } else {
        false
    };

    let epic_key = f.epic_link.or_else(|| f.epic.map(|e| e.key));
    let parent_key = f.parent.map(|p| p.key);

    Issue {
        key: dto.key,
        summary: f.summary,
        issue_type,
        status: f.status.name,
        assignee,
        priority,
        due_date,
        updated,
        epic_key,
        parent_key,
        links,
        blocked,
        overdue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::dto::*;

    fn make_issue_dto(status: &str, links: Vec<IssueLinkDto>) -> IssueDto {
        IssueDto {
            id: "1".into(),
            key: "PROJ-1".into(),
            self_url: "http://x".into(),
            fields: IssueFieldsDto {
                summary: "hello".into(),
                issue_type: IssueTypeDto { id: "1".into(), name: "Task".into(), subtask: false },
                status: StatusDetailDto {
                    id: "1".into(),
                    name: status.into(),
                    status_category: None,
                },
                assignee: None,
                priority: None,
                due_date: None,
                updated: None,
                parent: None,
                issuelinks: links,
                subtasks: vec![],
                epic: None,
                epic_link: None,
            },
        }
    }

    #[test]
    fn blocked_via_status() {
        let dto = make_issue_dto("Blocked", vec![]);
        let issue = map_issue(dto, &["Blocked".to_string()]);
        assert!(issue.blocked);
    }

    #[test]
    fn blocked_via_link() {
        let dto = make_issue_dto(
            "In Progress",
            vec![IssueLinkDto {
                id: "1".into(),
                link_type: LinkTypeDto {
                    id: "1".into(),
                    name: "Blocks".into(),
                    inward: "is blocked by".into(),
                    outward: "blocks".into(),
                },
                outward_issue: Some(LinkedIssueDto {
                    id: "2".into(),
                    key: "PROJ-2".into(),
                    self_url: "".into(),
                    fields: None,
                }),
                inward_issue: None,
            }],
        );
        let issue = map_issue(dto, &["Blocked".to_string()]);
        assert!(issue.blocked);
    }

    #[test]
    fn not_blocked() {
        let dto = make_issue_dto("To Do", vec![]);
        let issue = map_issue(dto, &["Blocked".to_string()]);
        assert!(!issue.blocked);
    }
}
