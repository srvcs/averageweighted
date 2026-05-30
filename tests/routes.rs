use axum::body::Body;
use axum::extract::Json as AxumJson;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_averageweighted::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

/// Read an `f64` operand named `key`, defaulting to `default` when absent.
fn num(body: &Value, key: &str, default: f64) -> f64 {
    body.get(key).and_then(Value::as_f64).unwrap_or(default)
}

/// Spawn a *computing* mock `srvcs-floatadd`: `{"a", "b"}` -> `{"result": a + b}`.
async fn spawn_floatadd() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": num(&body, "a", 0.0) + num(&body, "b", 0.0) }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-floatmultiply`: `{"a", "b"}` -> `{"result": a * b}`.
async fn spawn_floatmultiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": num(&body, "a", 0.0) * num(&body, "b", 0.0) }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-floatdivide`: `{"a", "b"}` -> `{"result": a / b}`,
/// or `422` on divide-by-zero.
async fn spawn_floatdivide() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let b = num(&body, "b", 1.0);
            if b == 0.0 {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": "divide by zero" })),
                );
            }
            (
                StatusCode::OK,
                Json(json!({ "result": num(&body, "a", 0.0) / b })),
            )
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-floatsubtract`: `{"a", "b"}` -> `{"result": a - b}`.
#[allow(dead_code)]
async fn spawn_floatsubtract() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": num(&body, "a", 0.0) - num(&body, "b", 0.0) }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-floatpower`: `{"base", "exp"}` -> `{"result": base^exp}`.
#[allow(dead_code)]
async fn spawn_floatpower() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": num(&body, "base", 0.0).powf(num(&body, "exp", 0.0)) }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-ln`: `{"value"}` -> `{"result": ln(value)}`.
#[allow(dead_code)]
async fn spawn_ln() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": num(&body, "value", 1.0).ln() }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-multiply` (integer): `{"a", "b"}` -> `{"result": a * b}`.
#[allow(dead_code)]
async fn spawn_multiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_i64).unwrap_or(0);
            let b = body.get("b").and_then(Value::as_i64).unwrap_or(0);
            Json(json!({ "result": a * b }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-reciprocal`: `{"value"}` -> `{"result": 1 / value}`.
#[allow(dead_code)]
async fn spawn_reciprocal() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": 1.0 / num(&body, "value", 1.0) }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-root`: `{"value", "n"}` -> `{"result": value^(1/n)}`.
#[allow(dead_code)]
async fn spawn_root() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            Json(json!({ "result": num(&body, "value", 0.0).powf(1.0 / num(&body, "n", 1.0)) }))
        }),
    );
    serve(app).await
}

/// Spawn a mock returning a fixed status + body (used for error-path tests).
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

fn app(floatmultiply_url: &str, floatadd_url: &str, floatdivide_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            floatmultiply_url: floatmultiply_url.to_string(),
            floatadd_url: floatadd_url.to_string(),
            floatdivide_url: floatdivide_url.to_string(),
        },
    )
}

async fn averageweighted(
    floatmultiply_url: &str,
    floatadd_url: &str,
    floatdivide_url: &str,
    values: Value,
    weights: Value,
) -> (StatusCode, Value) {
    let res = app(floatmultiply_url, floatadd_url, floatdivide_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "values": values, "weights": weights }).to_string(),
                ))
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
    app(DEAD_URL, DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

/// Approximate float comparison: floats must never be compared for exact
/// equality.
fn approx(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "got {got}, expected {expected}"
    );
}

// --- Standard endpoints. ---

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

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = app(DEAD_URL, DEAD_URL, DEAD_URL)
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

#[tokio::test]
async fn index_reports_identity() {
    let res = app(DEAD_URL, DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["service"], "srvcs-averageweighted");
    assert_eq!(body["concern"], "arithmetic: weighted average");
    assert_eq!(
        body["depends_on"],
        json!(["srvcs-floatmultiply", "srvcs-floatadd", "srvcs-floatdivide"])
    );
}

// --- Correctness cases, against the computing mocks. ---

#[tokio::test]
async fn weighted_average_1_2_3_with_weights_1_2_3() {
    let (mul, add, div) = (
        spawn_floatmultiply().await,
        spawn_floatadd().await,
        spawn_floatdivide().await,
    );
    let (status, body) =
        averageweighted(&mul, &add, &div, json!([1, 2, 3]), json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["values"], json!([1, 2, 3]));
    assert_eq!(body["weights"], json!([1, 2, 3]));
    // (1*1 + 2*2 + 3*3) / (1 + 2 + 3) = 14 / 6
    approx(body["result"].as_f64().unwrap(), 14.0 / 6.0);
}

#[tokio::test]
async fn weighted_average_equal_weights_is_plain_mean() {
    let (mul, add, div) = (
        spawn_floatmultiply().await,
        spawn_floatadd().await,
        spawn_floatdivide().await,
    );
    let (status, body) =
        averageweighted(&mul, &add, &div, json!([2, 4, 6]), json!([1, 1, 1])).await;
    assert_eq!(status, StatusCode::OK);
    // (2 + 4 + 6) / 3 = 4
    approx(body["result"].as_f64().unwrap(), 4.0);
}

#[tokio::test]
async fn weighted_average_fractional_inputs() {
    let (mul, add, div) = (
        spawn_floatmultiply().await,
        spawn_floatadd().await,
        spawn_floatdivide().await,
    );
    let (status, body) =
        averageweighted(&mul, &add, &div, json!([1.5, 2.5]), json!([0.5, 1.5])).await;
    assert_eq!(status, StatusCode::OK);
    // (1.5*0.5 + 2.5*1.5) / (0.5 + 1.5) = (0.75 + 3.75) / 2.0 = 2.25
    approx(body["result"].as_f64().unwrap(), 2.25);
}

#[tokio::test]
async fn weighted_average_single_element() {
    let (mul, add, div) = (
        spawn_floatmultiply().await,
        spawn_floatadd().await,
        spawn_floatdivide().await,
    );
    let (status, body) = averageweighted(&mul, &add, &div, json!([7]), json!([3])).await;
    assert_eq!(status, StatusCode::OK);
    // (7*3) / 3 = 7
    approx(body["result"].as_f64().unwrap(), 7.0);
}

// --- Validation. ---

#[tokio::test]
async fn rejects_empty_lists() {
    let (status, body) = averageweighted(DEAD_URL, DEAD_URL, DEAD_URL, json!([]), json!([])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        "values and weights must be non-empty and equal length"
    );
}

#[tokio::test]
async fn rejects_mismatched_lengths() {
    let (status, body) =
        averageweighted(DEAD_URL, DEAD_URL, DEAD_URL, json!([1, 2]), json!([1])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        "values and weights must be non-empty and equal length"
    );
}

// --- Error / degraded paths. ---

#[tokio::test]
async fn degrades_when_floatmultiply_unreachable() {
    let (add, div) = (spawn_floatadd().await, spawn_floatdivide().await);
    let (status, body) =
        averageweighted(DEAD_URL, &add, &div, json!([1, 2, 3]), json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatmultiply");
}

#[tokio::test]
async fn degrades_when_floatadd_unreachable() {
    let (mul, div) = (spawn_floatmultiply().await, spawn_floatdivide().await);
    let (status, body) =
        averageweighted(&mul, DEAD_URL, &div, json!([1, 2, 3]), json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatadd");
}

#[tokio::test]
async fn degrades_when_floatdivide_unreachable() {
    // floatmultiply + floatadd reachable, so the pipeline reaches the divide.
    let (mul, add) = (spawn_floatmultiply().await, spawn_floatadd().await);
    let (status, body) =
        averageweighted(&mul, &add, DEAD_URL, json!([1, 2, 3]), json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}

#[tokio::test]
async fn forwards_422_from_floatmultiply() {
    let (add, div) = (spawn_floatadd().await, spawn_floatdivide().await);
    let mul = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "value is not a number" }),
    )
    .await;
    let (status, _) = averageweighted(&mul, &add, &div, json!([1, 2, 3]), json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn forwards_422_from_floatdivide_on_zero_weights() {
    // weights sum to zero -> floatdivide rejects with 422 -> forwarded.
    let (mul, add, div) = (
        spawn_floatmultiply().await,
        spawn_floatadd().await,
        spawn_floatdivide().await,
    );
    let (status, _) = averageweighted(&mul, &add, &div, json!([1, 2]), json!([0, 0])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn malformed_floatmultiply_result_is_500() {
    let (add, div) = (spawn_floatadd().await, spawn_floatdivide().await);
    let mul = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-number" })).await;
    let (status, body) =
        averageweighted(&mul, &add, &div, json!([1, 2, 3]), json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-floatmultiply");
}
