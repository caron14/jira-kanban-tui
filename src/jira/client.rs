use crate::jira::error::JiraError;
use reqwest::{header, Client};
use std::time::Duration;

#[derive(Clone)]
pub struct JiraClient {
    pub base_url: String,
    pub client: Client,
}

impl std::fmt::Debug for JiraClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JiraClient")
            .field("base_url", &self.base_url)
            .field("client", &"<redacted>")
            .finish()
    }
}

impl JiraClient {
    pub fn new(base_url: String, token: String) -> Result<Self, JiraError> {
        let mut headers = header::HeaderMap::new();
        let auth_value = format!("Bearer {}", token);
        let mut auth = header::HeaderValue::from_str(&auth_value)
            .map_err(|e| JiraError::Other(e.to_string()))?;
        auth.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth);
        headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .user_agent(format!("jira-kanban-tui/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(JiraError::Network)?;
        let base_url = base_url.trim_end_matches('/').to_string();
        Ok(Self { base_url, client })
    }

    pub fn new_with_basic_auth(
        base_url: String,
        username: String,
        token: String,
    ) -> Result<Self, JiraError> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        let credentials = BASE64.encode(format!("{}:{}", username, token));
        let mut headers = header::HeaderMap::new();
        let mut auth = header::HeaderValue::from_str(&format!("Basic {}", credentials))
            .map_err(|e| JiraError::Other(e.to_string()))?;
        auth.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth);
        headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .user_agent(format!("jira-kanban-tui/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(JiraError::Network)?;
        let base_url = base_url.trim_end_matches('/').to_string();
        Ok(Self { base_url, client })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn map_error(
        status: reqwest::StatusCode,
        body: &str,
        headers: &header::HeaderMap,
    ) -> JiraError {
        match status.as_u16() {
            401 => JiraError::Authentication(body.to_string()),
            403 | 404 => JiraError::PermissionOrNotFound(body.to_string()),
            429 => {
                let retry_after = headers
                    .get(header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                JiraError::RateLimited { retry_after, message: body.to_string() }
            }
            400 | 409 | 422 => JiraError::Validation(body.to_string()),
            408 | 504 => JiraError::TimeoutOrOffline(body.to_string()),
            _ => JiraError::Other(format!("{}: {}", status, body)),
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, JiraError> {
        let url = self.url(path);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let headers = resp.headers().clone();
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &body, &headers));
        }
        let dto = resp.json().await.map_err(JiraError::Network)?;
        Ok(dto)
    }

    pub async fn current_user(&self) -> Result<crate::jira::Choice, JiraError> {
        let value: serde_json::Value = self.get_json("/rest/api/2/myself").await?;
        let id = value["accountId"]
            .as_str()
            .or_else(|| value["name"].as_str())
            .or_else(|| value["emailAddress"].as_str())
            .ok_or_else(|| JiraError::Other("Jira current user has no identifier".into()))?;
        let label = value["displayName"].as_str().unwrap_or(id);
        Ok(crate::jira::Choice { id: id.into(), label: label.into() })
    }

    pub async fn get_board(&self, board_id: i64) -> Result<crate::jira::dto::BoardDto, JiraError> {
        self.get_json(&format!("/rest/agile/1.0/board/{}", board_id)).await
    }

    pub async fn get_board_configuration(
        &self,
        board_id: i64,
    ) -> Result<crate::jira::dto::BoardConfigurationDto, JiraError> {
        self.get_json(&format!("/rest/agile/1.0/board/{}/configuration", board_id)).await
    }

    /// Fetch all board issues with paging (maxResults=50). Returns all issues.
    pub async fn get_board_issues(
        &self,
        board_id: i64,
    ) -> Result<Vec<crate::jira::dto::IssueDto>, JiraError> {
        let mut start_at = 0i64;
        let max_results = 50i64;
        let mut all = Vec::new();
        loop {
            let path = format!(
                "/rest/agile/1.0/board/{}/issue?startAt={}&maxResults={}",
                board_id, start_at, max_results
            );
            let page: crate::jira::dto::SearchResultDto = self.get_json(&path).await?;
            let count = page.issues.len() as i64;
            all.extend(page.issues);
            if page.start_at + count >= page.total || count == 0 {
                break;
            }
            start_at += count;
        }
        Ok(all)
    }

    pub async fn get_board_activity(
        &self,
        board_id: i64,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<crate::domain::activity::Activity>, JiraError> {
        use crate::domain::activity::{Activity, ChangeKind};
        fn parse_timestamp(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
            chrono::DateTime::parse_from_rfc3339(value)
                .or_else(|_| chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.3f%z"))
                .ok()
                .map(|value| value.to_utc())
        }
        let mut activities = Vec::new();
        let mut start_at = 0_u64;
        loop {
            let path = format!(
                "/rest/agile/1.0/board/{board_id}/issue?startAt={start_at}&maxResults=50&expand=changelog&jql=updated%20%3E%3D%20%22{}%22",
                since.format("%Y-%m-%d")
            );
            let value: serde_json::Value = self.get_json(&path).await?;
            let issue_count = value["issues"].as_array().map(Vec::len).unwrap_or(0);
            for issue in value["issues"].as_array().into_iter().flatten() {
                let key = issue["key"].as_str().unwrap_or_default();
                let summary = issue["fields"]["summary"].as_str().unwrap_or_default();
                if let Some(created) = issue["fields"]["created"]
                    .as_str()
                    .and_then(parse_timestamp)
                    .filter(|created| *created >= since)
                {
                    activities.push(Activity {
                        key: key.into(),
                        summary: summary.into(),
                        kind: ChangeKind::Created,
                        from: None,
                        to: None,
                        at: created,
                    });
                }
                for history in issue["changelog"]["histories"].as_array().into_iter().flatten() {
                    let Some(at) = history["created"].as_str().and_then(parse_timestamp) else {
                        continue;
                    };
                    if at < since {
                        continue;
                    }
                    for item in history["items"].as_array().into_iter().flatten() {
                        let kind = match item["field"].as_str().unwrap_or_default() {
                            "status"
                                if item["toString"]
                                    .as_str()
                                    .map(|value| {
                                        let value = value.to_lowercase();
                                        value.contains("done") || value.contains("closed")
                                    })
                                    .unwrap_or(false) =>
                            {
                                ChangeKind::Completed
                            }
                            "status" => ChangeKind::Status,
                            "assignee" => ChangeKind::Assignee,
                            "duedate" => ChangeKind::DueDate,
                            "priority" => ChangeKind::Priority,
                            _ => continue,
                        };
                        activities.push(Activity {
                            key: key.into(),
                            summary: summary.into(),
                            kind,
                            from: item["fromString"].as_str().map(str::to_owned),
                            to: item["toString"].as_str().map(str::to_owned),
                            at,
                        });
                    }
                }
            }
            start_at += issue_count as u64;
            let total = value["total"].as_u64().unwrap_or(start_at);
            if issue_count == 0 || issue_count < 50 || start_at >= total {
                break;
            }
        }
        activities.sort_by_key(|item| std::cmp::Reverse(item.at));
        Ok(activities)
    }

    pub async fn get_issue(&self, key: &str) -> Result<crate::jira::dto::IssueDto, JiraError> {
        self.get_json(&format!("/rest/api/2/issue/{}", key)).await
    }

    pub async fn get_transitions(
        &self,
        key: &str,
    ) -> Result<crate::jira::dto::TransitionsDto, JiraError> {
        self.get_json(&format!("/rest/api/2/issue/{}/transitions", key)).await
    }

    pub async fn do_transition(&self, key: &str, transition_id: &str) -> Result<(), JiraError> {
        let url = self.url(&format!("/rest/api/2/issue/{}/transitions", key));
        let body = serde_json::json!({ "transition": { "id": transition_id } });
        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let headers = resp.headers().clone();
            let b = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &b, &headers));
        }
        Ok(())
    }

    pub async fn search_users(
        &self,
        query: &str,
    ) -> Result<Vec<crate::jira::dto::UserDto>, JiraError> {
        // Cloud: /rest/api/2/user/search?query= ; DC: /rest/api/2/user/search?username=
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let path = format!("/rest/api/2/user/search?query={encoded}");
        let users: Vec<crate::jira::dto::UserDto> = match self.get_json(&path).await {
            Ok(u) => u,
            Err(JiraError::Validation(_)) | Err(JiraError::PermissionOrNotFound(_)) => {
                let fallback = format!("/rest/api/2/user/search?username={encoded}");
                self.get_json(&fallback).await?
            }
            Err(error) => return Err(error),
        };
        Ok(users)
    }

    pub async fn assign_issue(
        &self,
        key: &str,
        account_id: Option<&str>,
        data_center: bool,
    ) -> Result<(), JiraError> {
        let url = self.url(&format!("/rest/api/2/issue/{}/assignee", key));
        let body = if data_center {
            serde_json::json!({ "name": account_id })
        } else {
            serde_json::json!({ "accountId": account_id })
        };
        let resp = self.client.put(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let headers = resp.headers().clone();
            let b = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &b, &headers));
        }
        Ok(())
    }

    pub async fn set_due_date(&self, key: &str, due: Option<&str>) -> Result<(), JiraError> {
        let url = self.url(&format!("/rest/api/2/issue/{}", key));
        let body = serde_json::json!({ "fields": { "duedate": due } });
        let resp = self.client.put(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let headers = resp.headers().clone();
            let b = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &b, &headers));
        }
        Ok(())
    }

    pub async fn get_priorities(&self) -> Result<Vec<crate::jira::dto::PriorityDto>, JiraError> {
        self.get_json("/rest/api/2/priority").await
    }

    pub async fn set_priority(&self, key: &str, priority_id: &str) -> Result<(), JiraError> {
        let url = self.url(&format!("/rest/api/2/issue/{}", key));
        let body = serde_json::json!({ "fields": { "priority": { "id": priority_id } } });
        let resp = self.client.put(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let headers = resp.headers().clone();
            let b = resp.text().await.unwrap_or_default();
            return Err(Self::map_error(status, &b, &headers));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{body_json, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn get_board_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42, "name": "Test Board", "type": "kanban"
            })))
            .mount(&server)
            .await;

        let client = JiraClient::new(server.uri(), "dummy".into()).unwrap();
        let board = client.get_board(42).await.unwrap();
        assert_eq!(board.name, "Test Board");
    }

    #[tokio::test]
    async fn paging_board_issues() {
        let server = MockServer::start().await;
        // page 1
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/1/issue"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 0, "maxResults": 50, "total": 2,
                "issues": [
                    {"id":"1","key":"P-1","self":"http://x","fields":{"summary":"a","issuetype":{"id":"1","name":"Task","subtask":false},"status":{"id":"1","name":"To Do"},"assignee":null,"priority":null,"duedate":null,"updated":null,"parent":null,"issuelinks":[],"subtasks":[],"customfield_10014":null}},
                    {"id":"2","key":"P-2","self":"http://x","fields":{"summary":"b","issuetype":{"id":"1","name":"Bug","subtask":false},"status":{"id":"1","name":"Done"},"assignee":null,"priority":null,"duedate":null,"updated":null,"parent":null,"issuelinks":[],"subtasks":[],"customfield_10014":null}}
                ]
            })))
            .mount(&server)
            .await;

        let client = JiraClient::new(server.uri(), "t".into()).unwrap();
        let issues = client.get_board_issues(1).await.unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].key, "P-1");
    }

    #[tokio::test]
    async fn error_mapping_401() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/99"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;
        let client = JiraClient::new(server.uri(), "t".into()).unwrap();
        let err = client.get_board(99).await.unwrap_err();
        assert!(matches!(err, JiraError::Authentication(_)));
    }

    #[tokio::test]
    async fn rate_limited_retains_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/7"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "120")
                    .set_body_string("rate limited"),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(server.uri(), "t".into()).unwrap();
        let err = client.get_board(7).await.unwrap_err();
        match err {
            JiraError::RateLimited { retry_after, .. } => assert_eq!(retry_after, Some(120)),
            _ => panic!("wrong error"),
        }
    }

    #[tokio::test]
    async fn assignee_payload_differs_for_cloud_and_data_center() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/rest/api/2/issue/P-1/assignee"))
            .and(body_json(serde_json::json!({"accountId":"cloud-id"})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/rest/api/2/issue/P-2/assignee"))
            .and(body_json(serde_json::json!({"name":"dc-user"})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let client = JiraClient::new(server.uri(), "t".into()).unwrap();
        client.assign_issue("P-1", Some("cloud-id"), false).await.unwrap();
        client.assign_issue("P-2", Some("dc-user"), true).await.unwrap();
    }

    #[tokio::test]
    async fn status_due_date_and_priority_payloads_are_explicit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/2/issue/P-1/transitions"))
            .and(body_json(serde_json::json!({"transition":{"id":"31"}})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/rest/api/2/issue/P-1"))
            .and(body_json(serde_json::json!({"fields":{"duedate":"2026-08-20"}})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/rest/api/2/issue/P-2"))
            .and(body_json(serde_json::json!({"fields":{"priority":{"id":"2"}}})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let client = JiraClient::new(server.uri(), "token".into()).unwrap();
        client.do_transition("P-1", "31").await.unwrap();
        client.set_due_date("P-1", Some("2026-08-20")).await.unwrap();
        client.set_priority("P-2", "2").await.unwrap();
    }

    #[tokio::test]
    async fn board_activity_maps_changelog_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/1/issue"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [{
                    "key": "P-1",
                    "fields": {"summary": "Work"},
                    "changelog": {"histories": [{
                        "created": "2026-08-18T00:00:00Z",
                        "items": [{"field":"status","fromString":"To Do","toString":"Done"}]
                    }]}
                }]
            })))
            .mount(&server)
            .await;
        let client = JiraClient::new(server.uri(), "t".into()).unwrap();
        let since = chrono::DateTime::parse_from_rfc3339("2026-08-17T00:00:00Z").unwrap().to_utc();
        let items = client.get_board_activity(1, since).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, crate::domain::activity::ChangeKind::Completed);
        assert_eq!(items[0].to.as_deref(), Some("Done"));
    }

    #[tokio::test]
    async fn board_activity_paginates_issues() {
        let server = MockServer::start().await;
        let first_page = (0..50)
            .map(|index| {
                serde_json::json!({
                    "key":format!("P-{index}"),
                    "fields":{"summary":"Work"},
                    "changelog":{"histories":[]}
                })
            })
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/1/issue"))
            .and(query_param("startAt", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total":51,"issues":first_page
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/1/issue"))
            .and(query_param("startAt", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total":51,
                "issues":[{"key":"P-50","fields":{"summary":"Last"},"changelog":{"histories":[]}}]
            })))
            .mount(&server)
            .await;
        let client = JiraClient::new(server.uri(), "t".into()).unwrap();
        let since = chrono::DateTime::parse_from_rfc3339("2026-08-17T00:00:00Z").unwrap().to_utc();
        assert!(client.get_board_activity(1, since).await.unwrap().is_empty());
    }
}
