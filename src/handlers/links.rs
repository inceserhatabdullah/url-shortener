use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{StatusCode, header},
};

use crate::{
    error::AppResult,
    models::{CreateLinkRequest, LinkResponse, ListResponse},
    state::AppState,
};

use serde::Deserialize;
const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

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

pub async fn list_links(
    State(state): State<AppState>,
    params: Result<Query<ListParams>, QueryRejection>,
) -> AppResult<Json<ListResponse>> {
    let Query(params) = params?;
    // kaç kayıt atlanarak başlanacak. (a,b,c, c en yeni; offset=1 => b,c)
    let offset = params.offset.unwrap_or(0);
    // 1 ile max_limit arasında değer alıyor. 1'den küçük ise 1 max_limit'den büyük ise max_limit alınır.
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let (links, total) = state.service.list(offset, limit).await?;
    let items = links.iter().map(|l| state.service.to_response(l)).collect();

    Ok(Json(ListResponse {
        items,
        total,
        offset,
        limit,
    }))
}

pub async fn get_link(
    State(state): State<AppState>,
    code: Result<Path<String>, PathRejection>,
) -> AppResult<Json<LinkResponse>> {
    let Path(code) = code?;

    let link = state.service.get(&code).await?;

    Ok(Json(state.service.to_response(&link)))
}

pub async fn delete_link(
    State(state): State<AppState>,
    code: Result<Path<String>, PathRejection>,
) -> AppResult<StatusCode> {
    let Path(code) = code?;

    state.service.delete(&code).await?;

    Ok(StatusCode::NO_CONTENT)
}
