use axum::{
    extract::{Path, State, rejection::PathRejection},
    http::Method,
    response::Redirect,
};

use crate::{error::AppResult, state::AppState};

// Get /{code} tıklamayı sayar ve hedef URL 307 ile yönlendirir
// 301 yerine 307 kullanıyoruz: 301 tarayıcıda önbelleğe alınır ve sonraki tıklamalar sunucuya hiç uğramaz, sayaç bozulur.
pub async fn redirect(
    State(state): State<AppState>,
    code: Result<Path<String>, PathRejection>,
    method: Method,
) -> AppResult<Redirect> {
    let Path(code) = code?;

    // kod yoksa not-found var ise sayacı artırır hedef url'i döner
    let target = if method == Method::HEAD {
        // sayaç artmaz; kod yok ise yine not_found
        state.service.get(&code).await?.original_url
    } else {
        state.service.resolve(&code).await?
    };

    Ok(Redirect::temporary(&target))
}
