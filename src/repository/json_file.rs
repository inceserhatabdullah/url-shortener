use async_trait::async_trait;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::RwLock,
};

use crate::{
    error::{AppError, AppResult},
    models::Link,
    repository::UrlRepository,
};

pub struct JsonFileRepo {
    links: RwLock<HashMap<String, Link>>,
    path: PathBuf,
    // dosya yazımlarını sıraya sokar (aynı anda iki yazma olmasın)
    write_lock: tokio::sync::Mutex<()>,
}

impl JsonFileRepo {
    // dosya açar boş depo ile başlatır.
    pub async fn open(path: impl Into<PathBuf>) -> AppResult<Self> {
        let path = path.into();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let links = match tokio::fs::read(&path).await {
            Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => HashMap::new(),
            Ok(bytes) => {
                let list: Vec<Link> = serde_json::from_slice(&bytes)?;
                list.into_iter().map(|l| (l.code.clone(), l)).collect()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e.into()),
        };

        Ok(Self {
            links: RwLock::new(links),
            path,
            write_lock: tokio::sync::Mutex::new(()),
        })
    }

    // mevcut durumu atomik olarak diske yazar (tmp dosya + rename)
    async fn persist(&self) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;

        // kilit altında sadece klonla, sonra bırak (await boyunca tutma)
        let mut snapshot: Vec<Link> = {
            let map = self.links.read().expect("links lock poisoned");
            map.values().cloned().collect()
        };

        // dosya içeriği deterministik, eskiden yeniye; eşitlikte code'a göre
        snapshot.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.code.cmp(&b.code))
        });

        // json dosyasını biçimlendirir
        let json = serde_json::to_vec_pretty(&snapshot)?;
        let tmp = tmp_path(&self.path);
        tokio::fs::write(&tmp, json).await?;
        tokio::fs::rename(&tmp, &self.path).await?;

        Ok(())
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

#[async_trait]
impl UrlRepository for JsonFileRepo {
    async fn insert(&self, link: Link) -> AppResult<()> {
        {
            let mut map = self.links.write().expect("links lock poisoned");
            // kontrol + ekleme tek kilit altında: atomik
            if map.contains_key(&link.code) {
                return Err(AppError::Conflict(format!(
                    "code '{}' already exists",
                    link.code
                )));
            }
            map.insert(link.code.clone(), link);
        }
        self.persist().await
    }

    async fn get(&self, code: &str) -> AppResult<Option<Link>> {
        let map = self.links.read().expect("links lock poisoned");
        Ok(map.get(code).cloned())
    }

    async fn increment_click(&self, code: &str) -> AppResult<Link> {
        let updated = {
            let mut map = self.links.write().expect("links lock poisoned");
            let link = map
                .get_mut(code)
                .ok_or_else(|| AppError::NotFound(format!("code '{code}' not found")))?;
            link.click_count += 1;
            link.clone()
        };

        self.persist().await?;
        Ok(updated)
    }

    async fn list(&self, offset: usize, limit: usize) -> AppResult<(Vec<Link>, usize)> {
        let map = self.links.read().expect("links lock poisoned");
        let mut all: Vec<Link> = map.values().cloned().collect();

        all.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.code.cmp(&b.code))
        });
        let total = all.len();
        let items = all.into_iter().skip(offset).take(limit).collect();
        Ok((items, total))
    }

    async fn delete(&self, code: &str) -> AppResult<bool> {
        let removed = {
            let mut map = self.links.write().expect("links lock poisoned");
            map.remove(code).is_some()
        };

        if removed {
            self.persist().await?;
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Her test için benzersiz geçici dosya yolu (ek bağımlılık gerektirmez).
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "url_shortener_{}_{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    fn link(code: &str) -> Link {
        Link::new(code.to_string(), format!("https://example.com/{code}"))
    }

    #[tokio::test]
    async fn open_missing_file_starts_empty() {
        let repo = JsonFileRepo::open(temp_path("missing")).await.unwrap();
        let (items, total) = repo.list(0, 10).await.unwrap();
        assert!(items.is_empty());
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn data_survives_reopen() {
        let path = temp_path("reopen");
        {
            let repo = JsonFileRepo::open(&path).await.unwrap();
            repo.insert(link("abc")).await.unwrap();
            repo.increment_click("abc").await.unwrap();
        }
        let repo = JsonFileRepo::open(&path).await.unwrap();
        let got = repo
            .get("abc")
            .await
            .unwrap()
            .expect("kayıt dosyadan okunmalı");
        assert_eq!(got.click_count, 1);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn insert_duplicate_is_conflict() {
        let path = temp_path("dup");
        let repo = JsonFileRepo::open(&path).await.unwrap();
        repo.insert(link("abc")).await.unwrap();
        let err = repo.insert(link("abc")).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn delete_persists() {
        let path = temp_path("delete");
        {
            let repo = JsonFileRepo::open(&path).await.unwrap();
            repo.insert(link("abc")).await.unwrap();
            assert!(repo.delete("abc").await.unwrap());
            assert!(!repo.delete("abc").await.unwrap());
        }
        let repo = JsonFileRepo::open(&path).await.unwrap();
        assert!(repo.get("abc").await.unwrap().is_none());
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn corrupt_file_is_an_error() {
        let path = temp_path("corrupt");
        tokio::fs::write(&path, b"{ bu json degil").await.unwrap();
        assert!(JsonFileRepo::open(&path).await.is_err());
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn increment_missing_is_not_found() {
        let path = temp_path("missing_click");
        let repo = JsonFileRepo::open(&path).await.unwrap();
        let err = repo.increment_click("yok").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
