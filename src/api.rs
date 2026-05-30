use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-averageweighted";
pub const CONCERN: &str = "arithmetic: weighted average";
pub const DEPENDS_ON: &[&str] = &["srvcs-floatmultiply", "srvcs-floatadd", "srvcs-floatdivide"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub floatmultiply_url: String,
    pub floatadd_url: String,
    pub floatdivide_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    /// The values being averaged.
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
    /// The weight applied to each corresponding value.
    #[schema(value_type = Object)]
    pub weights: Vec<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct WeightedAverageResponse {
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
    #[schema(value_type = Object)]
    pub weights: Vec<Value>,
    pub result: f64,
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// A reachable dependency answered `200` but its body lacked a numeric
/// `result`. That is a contract violation we cannot recover from, so surface a
/// `500` rather than guessing.
fn malformed(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            json!({ "error": "dependency returned a malformed result", "dependency": dependency }),
        ),
    )
        .into_response()
}

/// Call one float dependency at `url` with `body`, returning its numeric
/// `result` or an early-return `Response` the caller should surface verbatim:
///
/// - unreachable / non-`200`/`422` -> `503` degraded
/// - `422` -> forwarded `422` (the dependency rejected the input)
/// - `200` without an `f64` `result` -> `500` malformed
async fn ask(url: &str, body: &Value, dependency: &str) -> Result<f64, Response> {
    match client::call(url, body).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => match body.get("result").and_then(Value::as_f64) {
            Some(result) => Ok(result),
            None => Err(malformed(dependency)),
        },
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute the weighted average of `values` with `weights`.
///
/// This service owns the *control flow* but delegates every arithmetic step to
/// its float dependencies:
///
/// 1. `sumwv = fold floatadd over floatmultiply(values[i], weights[i])` (start `0`);
/// 2. `sumw  = fold floatadd over weights` (start `0`);
/// 3. `result = floatdivide(sumwv, sumw)`.
///
/// `values` and `weights` must be non-empty and of equal length (`422`
/// otherwise). If a dependency is unreachable it reports itself degraded
/// (`503`); if a dependency rejects an operand it forwards the `422`.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = WeightedAverageResponse),
        (status = 422, description = "invalid input, or a dependency rejected an operand (forwarded)"),
        (status = 500, description = "a dependency returned a malformed result"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    if req.values.is_empty() || req.values.len() != req.weights.len() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "values and weights must be non-empty and equal length" })),
        )
            .into_response();
    }

    // 1. sumwv = sum of values[i] * weights[i].
    let mut sumwv: f64 = 0.0;
    for (v, w) in req.values.iter().zip(req.weights.iter()) {
        let product = match ask(
            &deps.floatmultiply_url,
            &json!({ "a": v, "b": w }),
            "srvcs-floatmultiply",
        )
        .await
        {
            Ok(p) => p,
            Err(resp) => return resp,
        };
        sumwv = match ask(
            &deps.floatadd_url,
            &json!({ "a": sumwv, "b": product }),
            "srvcs-floatadd",
        )
        .await
        {
            Ok(s) => s,
            Err(resp) => return resp,
        };
    }

    // 2. sumw = sum of weights.
    let mut sumw: f64 = 0.0;
    for w in &req.weights {
        sumw = match ask(
            &deps.floatadd_url,
            &json!({ "a": sumw, "b": w }),
            "srvcs-floatadd",
        )
        .await
        {
            Ok(s) => s,
            Err(resp) => return resp,
        };
    }

    // 3. result = sumwv / sumw.
    let result = match ask(
        &deps.floatdivide_url,
        &json!({ "a": sumwv, "b": sumw }),
        "srvcs-floatdivide",
    )
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    (
        StatusCode::OK,
        Json(json!({
            "values": req.values,
            "weights": req.weights,
            "result": result,
        })),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, WeightedAverageResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_all_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-averageweighted");
        assert_eq!(info.concern, "arithmetic: weighted average");
        assert_eq!(
            info.depends_on,
            vec!["srvcs-floatmultiply", "srvcs-floatadd", "srvcs-floatdivide"]
        );
    }
}
