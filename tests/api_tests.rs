use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use tower::ServiceExt;

use url_shortener::{
    build_router, config::Config, repository::memory::MemoryRepo, service::LinkService,
    state::AppState,
};

fn test_app() -> (Router, AppState) {
    let config = Config::from_lookup(|key| match key {
        "BASE_URL" => Some("http://localhost:3000".to_string()),
        _ => None,
    })
    .expect("valid test config");

    let service = Arc::new(LinkService::new(
        Arc::new(MemoryRepo::new()),
        config.base_url.clone(),
        config.short_code_length,
    ));

    let state = AppState::new(service, Arc::new(config));
    (build_router(state.clone()), state)
}

fn post_json(body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/links")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create(app: &Router, body: &str) {
    let res = app.clone().oneshot(post_json(body)).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

fn get_req(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn create_link_returns_201_with_short_url() {
    let (app, _) = test_app();
    let res = app
        .oneshot(post_json(r#"{"url":"https://example.com/page"}"#))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    assert!(res.headers().contains_key(header::LOCATION));
    let body = json_body(res).await;
    assert_eq!(body["original_url"], "https://example.com/page");
    assert_eq!(body["click_count"], 0);
    assert!(
        body["short_url"]
            .as_str()
            .unwrap()
            .starts_with("http://localhost:3000/")
    );
}

#[tokio::test]
async fn malformed_json_returns_400_json_error() {
    let (app, _) = test_app();
    let res = app.oneshot(post_json("{not json")).await.unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(json_body(res).await["error"]["code"].is_string());
}

#[tokio::test]
async fn invalid_url_returns_422() {
    let (app, _) = test_app();
    let res = app
        .oneshot(post_json(r#"{"url":"ftp://example.com"}"#))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn duplicate_alias_returns_409() {
    let (app, _) = test_app();
    let body = r#"{"url":"https://example.com","custom_alias":"mylink"}"#;

    let first = app.clone().oneshot(post_json(body)).await.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = app.oneshot(post_json(body)).await.unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn redirect_returns_307_and_counts_click() {
    let (app, state) = test_app();
    let body = r#"{"url":"https://example.com/target","custom_alias":"go-1"}"#;
    app.clone().oneshot(post_json(body)).await.unwrap();

    let res = app
        .oneshot(Request::get("/go-1").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        res.headers()[header::LOCATION],
        "https://example.com/target"
    );
    assert_eq!(state.service.get("go-1").await.unwrap().click_count, 1);
}

#[tokio::test]
async fn unknown_code_returns_404() {
    let (app, _) = test_app();
    let res = app
        .oneshot(Request::get("/nope123").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn health_is_not_shadowed_by_code_route() {
    let (app, _) = test_app();
    let res = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_link_returns_detail() {
    let (app, _) = test_app();
    create(
        &app,
        r#"{"url":"https://example.com","custom_alias":"abc"}"#,
    )
    .await;

    let res = app.oneshot(get_req("/api/links/abc")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["code"], "abc");
    assert_eq!(body["click_count"], 0);
}

#[tokio::test]
async fn get_unknown_link_returns_404() {
    let (app, _) = test_app();
    let res = app.oneshot(get_req("/api/links/nope")).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_returns_newest_first_with_pagination() {
    let (app, _) = test_app();
    for alias in ["one", "two", "three"] {
        create(
            &app,
            &format!(r#"{{"url":"https://example.com/{alias}","custom_alias":"{alias}"}}"#),
        )
        .await;
    }

    let res = app
        .clone()
        .oneshot(get_req("/api/links?offset=0&limit=2"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["limit"], 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["items"][0]["code"], "three");

    let res = app
        .oneshot(get_req("/api/links?offset=2&limit=2"))
        .await
        .unwrap();
    let body = json_body(res).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["code"], "one");
}

#[tokio::test]
async fn list_clamps_limit_and_rejects_garbage() {
    let (app, _) = test_app();

    let res = app
        .clone()
        .oneshot(get_req("/api/links?limit=1000"))
        .await
        .unwrap();
    assert_eq!(json_body(res).await["limit"], 100);

    let res = app.oneshot(get_req("/api/links?limit=abc")).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_removes_link_then_404() {
    let (app, _) = test_app();
    create(
        &app,
        r#"{"url":"https://example.com","custom_alias":"bye"}"#,
    )
    .await;

    let del = Request::delete("/api/links/bye")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(del).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(get_req("/api/links/bye"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let del = Request::delete("/api/links/bye")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(del).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn head_redirect_does_not_count_click() {
    let (app, state) = test_app();
    create(
        &app,
        r#"{"url":"https://example.com/t","custom_alias":"hd1"}"#,
    )
    .await;

    let head = Request::head("/hd1").body(Body::empty()).unwrap();
    let res = app.oneshot(head).await.unwrap();

    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(state.service.get("hd1").await.unwrap().click_count, 0);
}

#[tokio::test]
async fn health_returns_ok_json() {
    let (app, _) = test_app();
    let res = app.oneshot(get_req("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json_body(res).await["status"], "ok");
}
