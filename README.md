# URL Shortener

Rust ve Axum ile yazılmış, basit bir URL kısaltıcı. Veritabanı ve kullanıcı sistemi yoktur; veriler bellekte ya da bir JSON dosyasında tutulur.

## Özellikler

- Uzun URL'den kısa kod üretme veya özel alias belirleme
- 307 ile yönlendirme ve tıklama sayacı
- Listeleme (sayfalama), detay görüntüleme, silme
- İstek kimliği (`x-request-id`), istek logları, zaman aşımı, panic koruması
- Graceful shutdown (Ctrl+C / SIGTERM)
- Bellek veya JSON dosyası ile depolama

## Hızlı başlangıç

```bash
cp .env.example .env
cargo run
```

Sunucu varsayılan olarak `http://127.0.0.1:3000` adresinde açılır.

## Yapılandırma

Ayarlar ortam değişkenlerinden (veya `.env` dosyasından) okunur.

| Değişken | Varsayılan | Açıklama |
|---|---|---|
| `HOST` | `127.0.0.1` | Dinlenecek IP |
| `PORT` | `3000` | Dinlenecek port |
| `BASE_URL` | `http://localhost:3000` | Kısa linklerin önekleri. http/https olmalı |
| `SHORT_CODE_LENGTH` | `7` | Üretilen kod uzunluğu (4-16) |
| `STORAGE` | `memory` | `memory` veya `json` |
| `DATA_FILE` | `data/links.json` | JSON depolama dosyası |
| `REQUEST_TIMEOUT_SECS` | `10` | İstek zaman aşımı |
| `RUST_LOG` | `url_shortener=debug,tower_http=info` | Log filtresi |

> Varsayılan değerleri `config.rs`'teki gerçek varsayılanlarla eşitle.

## API

| Yöntem | Yol | Açıklama |
|---|---|---|
| `POST` | `/api/links` | Link oluştur (`201`) |
| `GET` | `/api/links?offset=0&limit=20` | Listele (limit en fazla 100) |
| `GET` | `/api/links/{code}` | Detay ve tıklama sayısı |
| `DELETE` | `/api/links/{code}` | Sil (`204`) |
| `GET` | `/{code}` | `307` ile yönlendir |
| `GET` | `/health` | Sağlık kontrolü |

Örnekler:

```bash
# Oluştur
curl -X POST localhost:3000/api/links \
  -H 'content-type: application/json' \
  -d '{"url":"https://example.com/cok/uzun/bir/adres","custom_alias":"merhaba"}'

# Yönlendirmeyi dene
curl -i localhost:3000/merhaba

# Listele
curl 'localhost:3000/api/links?limit=5'
```

Hatalar her zaman şu biçimdedir:

```json
{ "error": { "code": "validation_error", "message": "..." } }
```

## Depolama

- **memory:** En hızlısıdır, ancak sunucu kapanınca tüm veri kaybolur.
- **json:** Veriler `DATA_FILE` içine yazılır ve açılışta geri yüklenir. Yazma atomiktir (geçici dosya + rename). Bozuk dosyada sunucu açılışta hata verir, veriyi silmez.

Bilinen sınırlama: JSON modunda her değişiklik (tıklama dahil) dosyanın tamamını yeniden yazar. Küçük veri ve düşük trafik için uygundur, yüksek trafikte darboğaz olur.

## Mimari

```
HTTP → Middleware → Router → Handler → Service → Repository (trait) → MemoryRepo | JsonFileRepo
```

## Geliştirme

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```
