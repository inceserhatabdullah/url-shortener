use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

// 4xx hatalar istemciye gösterilir
// 5xx hatalar loglanır, istemciye sızdırılmaz
#[derive(Debug, Error)]
pub enum AppError {
    // bozuk json, hatalı path/query parametresi gibi istek biçim hataları
    #[error("{0}")]
    BadRequest(String),

    // içerik geçersiz (örn: geçersiz url)
    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    // dosya okuma/yazma hatası
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // json serileştirme/ayrıştırma hatası
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    // hatanın http karşılığı, eşleşme sadece burada yapılır
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Io(_) | Self::Serialization(_) | Self::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    // istemcinin okuyabileceği sabit hata kodu
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Io(_) | Self::Serialization(_) | Self::Internal(_) => "internal_error",
        }
    }
}

// yanıt gövdesinin dış katmanı (örn: {"error": {...}})
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Debug, Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        let message = if status.is_server_error() {
            tracing::error!(error = %self, "request failed with a server error");
            "an internal error occurred".to_string()
        } else {
            self.to_string()
        };

        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message,
            },
        };

        (status, Json(body)).into_response()
    }
}

// axum extractor hatalarını kendi hata tipimize çeviriyoruz, böylece bozuk json gibi durumlarda tutarlı json response dönüyoruz
impl From<JsonRejection> for AppError {
    fn from(value: JsonRejection) -> Self {
        Self::BadRequest(value.body_text())
    }
}

impl From<PathRejection> for AppError {
    fn from(value: PathRejection) -> Self {
        Self::BadRequest(value.body_text())
    }
}

impl From<QueryRejection> for AppError {
    fn from(value: QueryRejection) -> Self {
        Self::BadRequest(value.body_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Yanıt gövdesini JSON değerine çevirir.
    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn maps_variants_to_expected_status_codes() {
        assert_eq!(
            AppError::BadRequest("x".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::Validation("x".into()).status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            AppError::NotFound("x".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::Conflict("x".into()).status_code(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            AppError::Internal("x".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn client_errors_expose_their_message() {
        let response = AppError::NotFound("link 'abc' not found".into()).into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = body_json(response).await;
        assert_eq!(json["error"]["code"], "not_found");
        assert_eq!(json["error"]["message"], "link 'abc' not found");
    }

    #[tokio::test]
    async fn server_errors_hide_internal_details() {
        let response =
            AppError::Internal("secret path /var/data/links.json".into()).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = body_json(response).await;
        assert_eq!(json["error"]["code"], "internal_error");
        assert_eq!(json["error"]["message"], "an internal error occurred");
    }

    #[tokio::test]
    async fn io_errors_are_converted_with_question_mark() {
        // `?` operatörünün io::Error'ı otomatik olarak AppError'a çevirdiğini doğrular.
        fn failing() -> AppResult<()> {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "boom"))?;
            Ok(())
        }

        let err = failing().unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
