use crate::error::AppResult;
use crate::models::Link;
use async_trait::async_trait;

pub mod memory;
pub mod json_file;

#[async_trait]
pub trait UrlRepository: Send + Sync {
    // yeni link ekle, code var ise conflict döner
    async fn insert(&self, link: Link) -> AppResult<()>;
    async fn get(&self, code: &str) -> AppResult<Option<Link>>;
    // otomatik tıklama sayacı artırır ve güncel kayıt döner. Kod yok ise notfound döner
    async fn increment_click(&self, code: &str) -> AppResult<Link>;
    // yeniden eskiye sıralı sayfa ve toplam kayıt sayısı döner
    async fn list(&self, offset: usize, limit: usize) -> AppResult<(Vec<Link>, usize)>;
    async fn delete(&self, code: &str) -> AppResult<bool>;
}
