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

pub const SERVICE: &str = "srvcs-range";
pub const CONCERN: &str = "comparison: range (max - min) of a list";
pub const DEPENDS_ON: &[&str] = &["srvcs-sortascending", "srvcs-subtract"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub sortascending_url: String,
    pub subtract_url: String,
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
    /// The list of integers to take the range of. An empty list is rejected.
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct RangeResponse {
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
    pub result: i64,
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

/// Forward a dependency's response verbatim (used to propagate `422` for invalid
/// input, so range reports the same rejection a leaf dependency did).
fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// Ask `srvcs-sortascending` to sort `values`, returning the sorted JSON array
/// from its `result`. Maps the dependency's failures to the response this
/// service should return.
async fn ask_sort(url: &str, values: &[Value]) -> Result<Vec<Value>, Response> {
    let payload = json!({ "values": values });
    match client::call(url, &payload).await {
        Err(DepError::Unreachable) => Err(degraded("srvcs-sortascending")),
        Ok((200, body)) => match body.get("result").and_then(Value::as_array) {
            Some(arr) => Ok(arr.clone()),
            None => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "srvcs-sortascending returned no array result" })),
            )
                .into_response()),
        },
        // Invalid input propagates from the leaf dependency; forward it.
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded("srvcs-sortascending")),
    }
}

/// Ask `srvcs-subtract` for `a - b`, returning its integer `result`. Maps the
/// dependency's failures to the response this service should return.
async fn ask_subtract(url: &str, a: &Value, b: &Value) -> Result<i64, Response> {
    let payload = json!({ "a": a, "b": b });
    match client::call(url, &payload).await {
        Err(DepError::Unreachable) => Err(degraded("srvcs-subtract")),
        Ok((200, body)) => match body.get("result").and_then(Value::as_i64) {
            Some(n) => Ok(n),
            None => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "srvcs-subtract returned no integer result" })),
            )
                .into_response()),
        },
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded("srvcs-subtract")),
    }
}

/// `POST /` — compute the range (max - min) of a list of integers.
///
/// This service does no arithmetic of its own. It asks `srvcs-sortascending`
/// for the sorted list, then asks `srvcs-subtract` for
/// `sorted[last] - sorted[0]`. The empty list is rejected with `422`. Invalid
/// elements are rejected by the leaf dependencies and the resulting `422` is
/// forwarded unchanged.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = RangeResponse),
        (status = 422, description = "empty list, or an element is not a valid integer (forwarded)"),
        (status = 500, description = "a dependency returned an unusable response"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    if req.values.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "values must be a non-empty list" })),
        )
            .into_response();
    }

    // sorted = sortascending(values).result
    let sorted = match ask_sort(&deps.sortascending_url, &req.values).await {
        Ok(arr) => arr,
        Err(resp) => return resp,
    };

    if sorted.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "srvcs-sortascending returned an empty array" })),
        )
            .into_response();
    }

    // result = subtract(sorted[last], sorted[0]).result
    let last = &sorted[sorted.len() - 1];
    let first = &sorted[0];
    let result = match ask_subtract(&deps.subtract_url, last, first).await {
        Ok(n) => n,
        Err(resp) => return resp,
    };

    (
        StatusCode::OK,
        Json(json!({ "values": req.values, "result": result })),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, RangeResponse))
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
    async fn index_reports_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-range");
        assert_eq!(info.concern, "comparison: range (max - min) of a list");
        assert_eq!(
            info.depends_on,
            vec!["srvcs-sortascending", "srvcs-subtract"]
        );
    }
}
