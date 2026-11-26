use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub code: String,
    pub original_url: String,
    pub created_at: DateTime<Utc>,
    pub click_count: u64,
}

impl Link {
    pub fn new(code: String, original_url: String) -> Self {
        Self {
            code,
            original_url,
            created_at: Utc::now(),
            click_count: 0,
        }
    }
}

// POST /api/links
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLinkRequest {
    pub url: String,
    #[serde(default)]
    pub custom_alias: Option<String>,
}

// API'nin dışarı döndüğü link gösterimi
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkResponse {
    pub code: String,
    pub short_url: String,
    pub original_url: String,
    pub created_at: DateTime<Utc>,
    pub click_count: u64,
}

impl LinkResponse {
    pub fn from_link(link: &Link, base_url: &str) -> Self {
        Self {
            code: link.code.clone(),
            short_url: format!("{}/{}", base_url.trim_end_matches("/"), link.code),
            original_url: link.original_url.clone(),
            created_at: link.created_at,
            click_count: link.click_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub items: Vec<LinkResponse>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_link_start_with_zero_click() {
        let link = Link::new("abc123".into(), "https://example.com/".into());
        assert_eq!(link.click_count, 0);
    }

    #[test]
    fn response_builds_short_url_from_base() {
        let link = Link::new("abc123".into(), "https://example.com/".into());
        let response = LinkResponse::from_link(&link, "http://localhost:3000");
        assert_eq!(response.short_url, "http://localhost:3000/abc123");
    }

    #[test]
    fn create_request_alias_is_optional() {
        let request: CreateLinkRequest =
            serde_json::from_str(r#"{"url":"https::/example.com"}"#).unwrap();
        assert!(request.custom_alias.is_none());
    }
}
