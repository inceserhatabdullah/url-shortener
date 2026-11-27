use std::{any::Any, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

/*
* Bu yaklaşım, cross-cutting concerns (kesitsel sorumluluklar) olarak adlandırılır.

Cross-cutting concerns, uygulamanın birçok bölümünü etkileyen ancak tek bir iş mantığına ait olmayan sorumluluklardır.

Örneğin:
- Logging
- Authentication
- Authorization
- Rate limiting
- Timeout
- Metrics
- Tracing
- Error handling
Bunları middleware ile yönetmek, kodun bakımını ve genişletilmesini kolaylaştırır.

*/
// istek kimliği başlığı
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/* Axum'da en son eklenen layer, request'in ilk karşılaştığı (en dış) katmandır.

REQUEST
   |
   SetRequestIdLayer (UUID üretir / mevcut ID'yi korur)
   |
   TraceLayer
   |
   PropagateRequestIdLayer (Response'a ID aktarır)
   |
   CatchPanicLayer (panic yakalamaya çalışır)
   |
   TimeoutLayer (Zaman sınırı uygular)
   |
   Handler

   Response ise bu katmanların içinden dışına doğru geri döner.

Response
    |
    Handler
    |
    Timeout
    |
    CatchPanic
    |
    PropagateRequestId
    |
    Trace
    |
    SetRequestId
    |
    Client
*/

pub fn apply_middleware(router: Router, request_timeout: Duration) -> Router {
    router
        // handler süresini sınırla, aşılırsa 408 döndür
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        // handler panicleri 500 json yanıtına çevirir (sunucu çökmez). İç servislerde meydana gelebilecek panic'leri yakalamaya çalışır.
        .layer(CatchPanicLayer::custom(handle_panic))
        // Request ID'nin response'a aktarılmasını sağlar.
        .layer(PropagateRequestIdLayer::x_request_id())
        // her istek için bir span açar ve yanıtı loglar
        .layer(
            TraceLayer::new_for_http()
                // Span sayesinde bir request sırasında üretilen logların hangi request'e ve hangi işlem bağlamına ait olduğunu ilişkilendirmek kolaylaşır.
                .make_span_with(|req: &Request<Body>| {
                    let request_id = req
                        .headers()
                        .get(REQUEST_ID_HEADER)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");

                    // query'i loglamıyoruz, sadece path
                    // Burada http_request adında bir span oluşturuluyor. Span'in içine üç bilgi koyuyorsun:
                    tracing::info_span!(
                        "http_request",
                        method = %req.method(),
                        path = %req.uri().path(),
                        request_id = %request_id
                    )
                })
                // microsaniye
                //.on_response(DefaultOnResponse::new().level(Level::INFO).latency_unit(LatencyUnit::Micros)),
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        // en dışta başlık yok ise UUID üretir, varsa korur
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

// panic payload'i loglar, istemciye genel bir 500 Json döner
// Box<dyn Any + Send + 'static>: Panic sırasında ortaya çıkan hata bilgisini taşıyan, türü önceden kesin olarak bilinmeyen bir değer.
// Send: Değerin thread'ler arasında taşınabilmesi için gereken trait sınırı.
// 'static: Bu trait sınırı açısından değer, kısa ömürlü bir referansa bağlı olmamalıdır.
// Box: Değer heap üzerinde tutulur ve trait object olarak taşınır.
fn handle_panic(err: Box<dyn Any + Send + 'static>) -> Response<Body> {
    let details = if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unkown panic payload".to_string()
    };

    // Burada %details, tracing'e değeri Display formatında yazdırmasını söyler.
    tracing::error!(panic = %details, "handler panicked");

    let body = serde_json::json!({
       "error": {"code": "internal_error", "message": "Internal server error"}
    });

    // Builder Pattern: Bir nesneyi, özelliklerini adım adım belirleyerek oluşturma yaklaşımı.
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("static response must be valid")
}
