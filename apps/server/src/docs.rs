//! OpenAPI documentation — Swagger UI + Redoc.

use axum::response::{Html, Json};
use utoipa::OpenApi;

use crate::handlers;

#[derive(OpenApi)]
#[openapi(
    info(title = "coevo Agent Governance Mesh API", version = "1.0.0"),
    paths(
        handlers::compile::compile_contract,
        handlers::route::route_plan,
        handlers::propose::propose_fact,
        handlers::evaluate::evaluate_risk,
        handlers::resolve::resolve_conflict,
        handlers::health::health_check,
    ),
    components(schemas(
        coevo_core::problem::ProblemDetails,
    )),
    tags(
        (name = "MCL", description = "Mission Contract Language compiler"),
        (name = "Router", description = "PCDT routing"),
        (name = "Customs", description = "Cognitive Customs"),
        (name = "Risk", description = "Risk Gate evaluation"),
        (name = "Resolution", description = "Conflict resolution"),
    )
)]
pub struct ApiDoc;

/// Serve the OpenAPI JSON spec.
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Serve Swagger UI HTML page.
pub async fn swagger_ui() -> Html<String> {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>coevo API - Swagger UI</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
        SwaggerUIBundle({
            url: "/openapi.json",
            dom_id: '#swagger-ui',
            presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
            layout: "BaseLayout"
        });
    </script>
</body>
</html>"#
            .to_string(),
    )
}

/// Serve Redoc HTML page.
pub async fn redoc() -> Html<String> {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>coevo API - Redoc</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style> body { margin: 0; padding: 0; } </style>
</head>
<body>
    <redoc spec-url="/openapi.json" expand-responses="200,202"></redoc>
    <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
</body>
</html>"#
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_openapi_excludes_internal_demo_routes() {
        let doc = ApiDoc::openapi();
        let paths = doc.paths.paths.keys().cloned().collect::<Vec<_>>();
        assert!(
            paths.iter().all(|p| !p.starts_with("/demo/")),
            "internal demo routes must not appear in public OpenAPI paths: {:?}",
            paths
        );
        assert!(
            doc.tags
                .as_deref()
                .unwrap_or_default()
                .iter()
                .all(|tag| tag.name != "Demo"),
            "internal demo routes must not expose a public OpenAPI Demo tag"
        );
    }
}
