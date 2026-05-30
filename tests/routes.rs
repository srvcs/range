use axum::body::Body;
use axum::extract::Json as JsonExtract;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_range::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

// --- COMPUTING dependency mocks ----------------------------------------------
//
// Each mock ACTUALLY COMPUTES from the request body, so the composition is
// genuinely tested rather than faked. The range orchestrator calls
// sortascending then subtract; both mocks below do the real work.

/// Mock `srvcs-sortascending`: reads `{values: [int, ...]}`, sorts ascending,
/// returns `{"values", "result": [sorted ints]}`. Non-integer elements are
/// rejected with `422`, mirroring the real leaf service (this is how validation
/// propagates to the orchestrator).
async fn spawn_computing_sortascending() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let values = req["values"].as_array().cloned().unwrap_or_default();
            let mut nums: Vec<i64> = Vec::with_capacity(values.len());
            for v in &values {
                match v.as_i64() {
                    Some(n) => nums.push(n),
                    None => {
                        return (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            Json(json!({ "error": "values must be integers" })),
                        )
                    }
                }
            }
            nums.sort();
            (
                StatusCode::OK,
                Json(json!({ "values": values, "result": nums })),
            )
        }),
    );
    serve(app).await
}

/// Mock `srvcs-subtract`: reads `{a, b}` and returns
/// `{"a", "b", "result": a - b}`. This makes `range == max - min` real.
async fn spawn_computing_subtract() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let a = req["a"].as_i64().unwrap_or(0);
            let b = req["b"].as_i64().unwrap_or(0);
            Json(json!({ "a": a, "b": b, "result": a - b }))
        }),
    );
    serve(app).await
}

/// Mock that always answers with a fixed status + body (used to simulate a
/// dependency's `422` rejection).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn app(sortascending_url: &str, subtract_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            sortascending_url: sortascending_url.to_string(),
            subtract_url: subtract_url.to_string(),
        },
    )
}

async fn eval(sortascending_url: &str, subtract_url: &str, values: Value) -> (StatusCode, Value) {
    let res = app(sortascending_url, subtract_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "values": values }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

// --- Standard endpoints ------------------------------------------------------

#[tokio::test]
async fn index_ok() {
    assert_eq!(status_of("/").await, StatusCode::OK);
}

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

// --- Correctness cases, exercised against REAL computing dependencies --------

#[tokio::test]
async fn range_of_list() {
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_computing_subtract().await;
    let (status, body) = eval(&sort, &sub, json!([1, 5, 3])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], 4);
    assert_eq!(body["values"], json!([1, 5, 3]));
}

#[tokio::test]
async fn range_of_singleton_is_zero() {
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_computing_subtract().await;
    let (status, body) = eval(&sort, &sub, json!([7])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], 0);
}

#[tokio::test]
async fn range_handles_negatives() {
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_computing_subtract().await;
    // min = -7, max = 10 => range = 17
    let (status, body) = eval(&sort, &sub, json!([10, -3, -7, 4])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], 17);
}

#[tokio::test]
async fn range_of_already_sorted() {
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_computing_subtract().await;
    let (status, body) = eval(&sort, &sub, json!([-2, 0, 7])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], 9);
}

#[tokio::test]
async fn range_with_duplicate_extremes() {
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_computing_subtract().await;
    let (status, body) = eval(&sort, &sub, json!([5, 5, 5])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], 0);
}

// --- Error / edge cases ------------------------------------------------------

#[tokio::test]
async fn empty_list_is_rejected() {
    // DEAD_URL: an empty list must short-circuit to 422 with no dependency call.
    let (status, body) = eval(DEAD_URL, DEAD_URL, json!([])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn forwards_422_from_sortascending_for_bad_element() {
    // A real computing sortascending rejects non-integers with 422; the
    // orchestrator forwards it unchanged.
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_computing_subtract().await;
    let (status, body) = eval(&sort, &sub, json!([1, "nope", 3])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "values must be integers");
}

#[tokio::test]
async fn forwards_422_from_subtract() {
    let sort = spawn_computing_sortascending().await;
    let sub = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "operand is not an integer" }),
    )
    .await;
    let (status, body) = eval(&sort, &sub, json!([1, 5, 3])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "operand is not an integer");
}

#[tokio::test]
async fn degrades_when_sortascending_is_unreachable() {
    let sub = spawn_computing_subtract().await;
    let (status, body) = eval(DEAD_URL, &sub, json!([1, 5, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-sortascending");
}

#[tokio::test]
async fn degrades_when_subtract_is_unreachable() {
    let sort = spawn_computing_sortascending().await;
    let (status, body) = eval(&sort, DEAD_URL, json!([1, 5, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-subtract");
}

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.headers().contains_key("x-request-id"),
        "response must carry a generated x-request-id"
    );
}
