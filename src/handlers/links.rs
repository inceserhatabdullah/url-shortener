use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::{StatusCode, header},
};

use crate::{
    error::AppResult,
    models::{CreateLinkRequest, LinkResponse},
    state::AppState,
};

// POST /api/links
pub async fn create_link(
    State(state): State<AppState>,
    // rejection kendimiz alıyoruz ki '?' ile AppError (json gövdeli 400) olsun.
    payload: Result<Json<CreateLinkRequest>, JsonRejection>,
) -> AppResult<(
    StatusCode,
    [(header::HeaderName, String); 1],
    Json<LinkResponse>,
)> {
    let Json(request) = payload?;

    let link = state.service.create(request).await?;
    let location = format!("/api/links/{}", link.code);
    let body = state.service.to_response(&link);

    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(body),
    ))
}
