use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use super::UrlRepository;
use crate::error::{AppError, AppResult};
use crate::models::Link;

// süreç bellekte tutulur, uygulama kapanınca veri kaybolur.
#[derive(Debug, Default)]
pub struct MemoryRepo {
    links: RwLock<HashMap<String, Link>>,
}

impl MemoryRepo {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl UrlRepository for MemoryRepo {
    async fn insert(&self, link: Link) -> AppResult<()> {
        // kontrol + ekleme tek yazma kilidi altında: yarış durumu yok
        let mut links = self.links.write().await;

        if links.contains_key(&link.code) {
            return Err(AppError::Conflict(format!(
                "code '{}' is already in use",
                link.code
            )));
        }

        links.insert(link.code.clone(), link);
        Ok(())
    }

    async fn get(&self, code: &str) -> AppResult<Option<Link>> {
        Ok(self.links.read().await.get(code).cloned())
    }

    async fn increment_click(&self, code: &str) -> AppResult<Link> {
        let mut links = self.links.write().await;
        let link = links
            .get_mut(code)
            .ok_or_else(|| AppError::NotFound(format!("link '{code}' not found")))?;
        link.click_count += 1;
        Ok(link.clone())
    }

    async fn list(&self, offset: usize, limit: usize) -> AppResult<(Vec<Link>, usize)> {
        let links = self.links.read().await;
        let total = links.len();

        let mut all: Vec<&Link> = links.values().collect();

        all.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.code.cmp(&b.code))
        });

        let page = all.into_iter().skip(offset).take(limit).cloned().collect();

        Ok((page, total))
    }

    async fn delete(&self, code: &str) -> AppResult<bool> {
        Ok(self.links.write().await.remove(code).is_some())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};

    use super::*;

    fn link(code: &str) -> Link {
        Link::new(code.into(), "https://example.com/".into())
    }

    #[tokio::test]
    async fn insert_then_get_returns_link() {
        let repo = MemoryRepo::new();
        repo.insert(link("abc")).await.unwrap();

        let found = repo.get("abc").await.unwrap().unwrap();
        assert_eq!(found.code, "abc");
        assert_eq!(found.click_count, 0);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let repo = MemoryRepo::new();
        assert!(repo.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn insert_duplicate_code_is_conflict() {
        let repo = MemoryRepo::new();
        repo.insert(link("abc")).await.unwrap();

        let err = repo.insert(link("abc")).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[tokio::test]
    async fn increment_click_updates_and_returns_link() {
        let repo = MemoryRepo::new();
        repo.insert(link("abc")).await.unwrap();

        assert_eq!(repo.increment_click("abc").await.unwrap().click_count, 1);
        assert_eq!(repo.increment_click("abc").await.unwrap().click_count, 2);
        assert_eq!(repo.get("abc").await.unwrap().unwrap().click_count, 2);
    }

    #[tokio::test]
    async fn increment_click_missing_is_not_found() {
        let repo = MemoryRepo::new();
        let err = repo.increment_click("nope").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_reports_whether_something_was_removed() {
        let repo = MemoryRepo::new();
        repo.insert(link("abc")).await.unwrap();

        assert!(repo.delete("abc").await.unwrap());
        assert!(!repo.delete("abc").await.unwrap());
        assert!(repo.get("abc").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_is_newest_first_with_total() {
        let repo = MemoryRepo::new();
        let now = Utc::now();
        for (code, age_secs) in [("old", 30), ("mid", 20), ("new", 10)] {
            let mut l = link(code);
            l.created_at = now - Duration::seconds(age_secs);
            repo.insert(l).await.unwrap();
        }

        let (page, total) = repo.list(0, 10).await.unwrap();
        let codes: Vec<_> = page.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["new", "mid", "old"]);
        assert_eq!(total, 3);
    }

    #[tokio::test]
    async fn list_paginates_with_offset_and_limit() {
        let repo = MemoryRepo::new();
        let now = Utc::now();
        for i in 0..5 {
            let mut l = link(&format!("c{i}"));
            l.created_at = now - Duration::seconds(i);
            repo.insert(l).await.unwrap();
        }

        let (page, total) = repo.list(1, 2).await.unwrap();
        let codes: Vec<_> = page.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["c1", "c2"]);
        assert_eq!(total, 5);

        // Offset toplamın ötesindeyse boş sayfa, toplam yine doğru.
        let (page, total) = repo.list(10, 2).await.unwrap();
        assert!(page.is_empty());
        assert_eq!(total, 5);
    }

    #[tokio::test]
    async fn concurrent_clicks_are_not_lost() {
        let repo = Arc::new(MemoryRepo::new());
        repo.insert(link("abc")).await.unwrap();

        let mut handles = Vec::new();
        for _ in 0..50 {
            let repo = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                repo.increment_click("abc").await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(repo.get("abc").await.unwrap().unwrap().click_count, 50);
    }

    #[tokio::test]
    async fn works_behind_dyn_trait_object() {
        // AppState'te kullanılacak biçim: Arc<dyn UrlRepository>.
        let repo: Arc<dyn UrlRepository> = Arc::new(MemoryRepo::new());
        repo.insert(link("abc")).await.unwrap();
        assert!(repo.get("abc").await.unwrap().is_some());
    }
}
