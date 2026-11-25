use std::sync::Arc;

use rand::Rng;

use crate::{
    error::{AppError, AppResult},
    models::{CreateLinkRequest, Link, LinkResponse},
    repository::UrlRepository,
    validation::{RESERVED_ALIASES, validate_alias, validate_url},
};

// kod üretim çakışma halinde kaç kez yeniden denenir
pub const MAX_CODE_ATTEMPTS: usize = 5;
// list için izin verilen en büyük sayfa boyutu
pub const MAX_PAGE_SIZE: usize = 100;
const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

// iş kuralların bulunduğu katman: doğrulama, kod üretim, çakışma yönetimi
pub struct LinkService {
    repo: Arc<dyn UrlRepository>,
    base_url: String,
    code_length: usize,
}

impl LinkService {
    pub fn new(repo: Arc<dyn UrlRepository>, base_url: String, code_length: usize) -> Self {
        Self {
            repo,
            base_url,
            code_length,
        }
    }

    // yeni kısa link oluştur
    pub async fn create(&self, request: CreateLinkRequest) -> AppResult<Link> {
        let url = validate_url(&request.url, &self.base_url)?;
        if let Some(alias) = &request.custom_alias {
            validate_alias(alias)?;
        }

        match request.custom_alias {
            Some(alias) => self.create_with_alias(alias, url).await,
            None => self.create_with_generated_code(url).await,
        }
    }

    // kullanıcının seçtiği alias ile ekler.
    async fn create_with_alias(&self, alias: String, url: String) -> AppResult<Link> {
        let link = Link::new(alias.clone(), url);
        match self.repo.insert(link.clone()).await {
            Ok(()) => Ok(link),
            Err(AppError::Conflict(_)) => Err(AppError::Conflict(format!(
                "alias '{alias} is already in use'"
            ))),
            Err(e) => Err(e),
        }
    }

    // rastgele kod üretir, çakışırsa sınırla sayıda yeniden denenir
    async fn create_with_generated_code(&self, url: String) -> AppResult<Link> {
        for _ in 0..MAX_CODE_ATTEMPTS {
            let code = generate_code(self.code_length);

            if is_reserved(&code) {
                continue;
            }

            let link = Link::new(code, url.clone());

            match self.repo.insert(link.clone()).await {
                Ok(()) => return Ok(link),
                // kod doluydu, yeni kod üretip tekrar dene
                Err(AppError::Conflict(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(AppError::Internal(format!(
            "could not generate a unique code after {MAX_CODE_ATTEMPTS} attempts"
        )))
    }

    pub async fn get(&self, code: &str) -> AppResult<Link> {
        self.repo.get(code).await?.ok_or_else(|| not_found(code))
    }

    // yönlendirme için: tıklamayı artırır ve hedef url döndürür
    pub async fn resolve(&self, code: &str) -> AppResult<String> {
        Ok(self.repo.increment_click(code).await?.original_url)
    }

    pub async fn list(&self, offset: usize, limit: usize) -> AppResult<(Vec<Link>, usize)> {
        let limit = limit.clamp(1, MAX_PAGE_SIZE);
        self.repo.list(offset, limit).await
    }

    pub async fn delete(&self, code: &str) -> AppResult<()> {
        if self.repo.delete(code).await? {
            return Ok(());
        }

        Err(not_found(code))
    }

    pub fn to_response(&self, link: &Link) -> LinkResponse {
        LinkResponse::from_link(link, &self.base_url)
    }
}

fn generate_code(len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| BASE62[rng.random_range(0..BASE62.len())] as char)
        .collect()
}

fn is_reserved(code: &str) -> bool {
    RESERVED_ALIASES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(code))
}

fn not_found(code: &str) -> AppError {
    AppError::NotFound(format!("link '{code}' not found"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::repository::memory::MemoryRepo;

    const BASE: &str = "http://localhost:3000";

    fn service() -> LinkService {
        LinkService::new(Arc::new(MemoryRepo::new()), BASE.into(), 7)
    }

    fn request(url: &str, alias: Option<&str>) -> CreateLinkRequest {
        CreateLinkRequest {
            url: url.into(),
            custom_alias: alias.map(String::from),
        }
    }

    /// İlk `remaining` insert çağrısında `Conflict` dönen, sonra
    /// gerçek depoya devreden test deposu. Yeniden deneme mantığını sınar.
    struct FlakyRepo {
        remaining: AtomicUsize,
        inner: MemoryRepo,
    }

    impl FlakyRepo {
        fn new(conflicts: usize) -> Self {
            Self {
                remaining: AtomicUsize::new(conflicts),
                inner: MemoryRepo::new(),
            }
        }
    }

    #[async_trait]
    impl UrlRepository for FlakyRepo {
        async fn insert(&self, link: Link) -> AppResult<()> {
            if self.remaining.load(Ordering::SeqCst) > 0 {
                self.remaining.fetch_sub(1, Ordering::SeqCst);
                return Err(AppError::Conflict("forced conflict".into()));
            }
            self.inner.insert(link).await
        }
        async fn get(&self, code: &str) -> AppResult<Option<Link>> {
            self.inner.get(code).await
        }
        async fn increment_click(&self, code: &str) -> AppResult<Link> {
            self.inner.increment_click(code).await
        }
        async fn list(&self, offset: usize, limit: usize) -> AppResult<(Vec<Link>, usize)> {
            self.inner.list(offset, limit).await
        }
        async fn delete(&self, code: &str) -> AppResult<bool> {
            self.inner.delete(code).await
        }
    }

    #[test]
    fn generate_code_uses_base62_and_requested_length() {
        for len in [4, 7, 16] {
            let code = generate_code(len);
            assert_eq!(code.len(), len);
            assert!(code.bytes().all(|b| BASE62.contains(&b)));
        }
    }

    #[tokio::test]
    async fn create_without_alias_generates_code() {
        let svc = service();
        let link = svc
            .create(request("https://example.com/a", None))
            .await
            .unwrap();

        assert_eq!(link.code.len(), 7);
        assert_eq!(link.original_url, "https://example.com/a");
        assert_eq!(svc.get(&link.code).await.unwrap(), link);
    }

    #[tokio::test]
    async fn create_normalizes_url() {
        let svc = service();
        let link = svc
            .create(request("  HTTPS://Example.COM  ", None))
            .await
            .unwrap();
        assert_eq!(link.original_url, "https://example.com/");
    }

    #[tokio::test]
    async fn create_with_alias_uses_it() {
        let svc = service();
        let link = svc
            .create(request("https://example.com", Some("my-link")))
            .await
            .unwrap();
        assert_eq!(link.code, "my-link");
        assert!(svc.get("my-link").await.is_ok());
    }

    #[tokio::test]
    async fn create_alias_conflict_is_not_retried() {
        let svc = service();
        svc.create(request("https://example.com", Some("taken")))
            .await
            .unwrap();

        let err = svc
            .create(request("https://other.com", Some("taken")))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));

        // İlk kayıt ezilmemiş olmalı.
        assert_eq!(
            svc.get("taken").await.unwrap().original_url,
            "https://example.com/"
        );
    }

    #[tokio::test]
    async fn create_rejects_invalid_alias_without_storing() {
        let svc = service();
        for alias in ["ab", "api", "bad alias"] {
            let err = svc
                .create(request("https://example.com", Some(alias)))
                .await
                .unwrap_err();
            assert!(matches!(err, AppError::Validation(_)), "{alias}");
        }
        assert_eq!(svc.list(0, 10).await.unwrap().1, 0);
    }

    #[tokio::test]
    async fn create_rejects_invalid_url() {
        let svc = service();
        for url in [
            "",
            "not a url",
            "ftp://example.com",
            "http://localhost:3000/x",
        ] {
            let err = svc.create(request(url, None)).await.unwrap_err();
            assert!(matches!(err, AppError::Validation(_)), "{url}");
        }
    }

    #[tokio::test]
    async fn create_retries_on_collision() {
        let repo = Arc::new(FlakyRepo::new(2));
        let svc = LinkService::new(repo, BASE.into(), 7);

        let link = svc
            .create(request("https://example.com", None))
            .await
            .unwrap();
        assert!(svc.get(&link.code).await.is_ok());
    }

    #[tokio::test]
    async fn create_gives_up_after_max_attempts() {
        let repo = Arc::new(FlakyRepo::new(MAX_CODE_ATTEMPTS));
        let svc = LinkService::new(repo, BASE.into(), 7);

        let err = svc
            .create(request("https://example.com", None))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[tokio::test]
    async fn get_missing_is_not_found() {
        let err = service().get("nope").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_increments_clicks_and_returns_target() {
        let svc = service();
        let link = svc
            .create(request("https://example.com/a", Some("go-1")))
            .await
            .unwrap();

        assert_eq!(
            svc.resolve(&link.code).await.unwrap(),
            "https://example.com/a"
        );

        svc.resolve(&link.code).await.unwrap();
        assert_eq!(svc.get(&link.code).await.unwrap().click_count, 2);
    }

    #[tokio::test]
    async fn resolve_missing_is_not_found() {
        let err = service().resolve("nope").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_then_not_found() {
        let svc = service();
        svc.create(request("https://example.com", Some("bye")))
            .await
            .unwrap();

        svc.delete("bye").await.unwrap();
        assert!(matches!(
            svc.get("bye").await.unwrap_err(),
            AppError::NotFound(_)
        ));
        assert!(matches!(
            svc.delete("bye").await.unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn list_clamps_limit() {
        let svc = service();
        for alias in ["one", "two", "three"] {
            svc.create(request("https://example.com", Some(alias)))
                .await
                .unwrap();
        }

        // limit=0 en az 1'e çekilir.
        let (page, total) = svc.list(0, 0).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(total, 3);

        // Çok büyük limit MAX_PAGE_SIZE'a kırpılır; elimizde 3 kayıt var.
        let (page, _) = svc.list(0, 10_000).await.unwrap();
        assert_eq!(page.len(), 3);
    }

    #[tokio::test]
    async fn to_response_derives_short_url() {
        let svc = service();
        let link = svc
            .create(request("https://example.com", Some("abc")))
            .await
            .unwrap();
        assert_eq!(
            svc.to_response(&link).short_url,
            "http://localhost:3000/abc"
        );
    }
}
