//! Fork: REST for the agent manifest (`queries::agent_manifest`).
//!
//! - `GET /api/agent/manifest?project=&format=` — the block an agent pastes
//!   into AGENTS.md / CLAUDE.md, rendered for a project. Markdown by default,
//!   `format=json` adds the version. Any signed-in caller: it is the same
//!   text for everyone and carries no project data.
//! - `GET /api/instance/agent-manifest` — the stored template and the
//!   built-in default, for the admin editor.
//! - `PUT /api/instance/agent-manifest` — save it; `DELETE` resets it. Admin
//!   only, from a recent browser session, like instance settings: the text
//!   lands in every agent's instructions.

use axum::{
    Extension,
    extract::{Json, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::db::DbPool;
use crate::db::queries::agent_manifest as manifest;
use crate::error::LificError;
use crate::resolve_caller::ResolvedIdentity;

use super::{require_admin, require_user, with_read};

#[derive(Debug, Deserialize)]
pub(super) struct ManifestQuery {
    project: Option<String>,
    format: Option<String>,
}

pub(super) async fn get_rendered(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<ResolvedIdentity>>,
    Query(q): Query<ManifestQuery>,
) -> Result<Response, LificError> {
    require_user(&identity)?;
    let m = with_read(&db, manifest::get)?;
    let block = manifest::render(&m, q.project.as_deref());
    if q.format.as_deref() == Some("json") {
        return Ok(Json(serde_json::json!({
            "version": m.version,
            "updated_at": m.updated_at,
            "is_default": m.is_default,
            "markdown": block,
        }))
        .into_response());
    }
    let mut res = block.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    Ok(res)
}

fn admin_json(m: &manifest::Manifest) -> serde_json::Value {
    serde_json::json!({
        "content": m.content,
        "is_default": m.is_default,
        "updated_at": m.updated_at,
        "version": m.version,
        "default": manifest::DEFAULT.trim(),
        "max_chars": manifest::MAX_CHARS,
        "preview": manifest::render(m, None),
    })
}

pub(super) async fn admin_get(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<ResolvedIdentity>>,
) -> Result<Json<serde_json::Value>, LificError> {
    require_admin(&identity)?;
    let m = with_read(&db, manifest::get)?;
    Ok(Json(admin_json(&m)))
}

#[derive(Debug, Deserialize)]
pub(super) struct ManifestPut {
    content: String,
}

/// Admin, re-checked inside the write against a recent browser session.
fn write_as_admin(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    headers: &HeaderMap,
    op: impl FnOnce(&rusqlite::Connection) -> Result<manifest::Manifest, LificError>,
) -> Result<manifest::Manifest, LificError> {
    require_admin(identity)?;
    let admin = require_user(identity)?;
    let token = crate::auth::recent_session_token(headers)?;
    db.transaction(move |tx| {
        let fresh = crate::auth::revalidate_recent_session(tx, &token, admin.id)?;
        crate::auth::require_fresh_admin(&fresh)?;
        op(tx)
    })
}

pub(super) async fn admin_put(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<ResolvedIdentity>>,
    headers: HeaderMap,
    Json(input): Json<ManifestPut>,
) -> Result<Json<serde_json::Value>, LificError> {
    let m = write_as_admin(&db, &identity, &headers, |tx| manifest::set(tx, &input.content))?;
    tracing::info!(version = %m.version, "agent manifest saved");
    Ok(Json(admin_json(&m)))
}

pub(super) async fn admin_reset(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<ResolvedIdentity>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, LificError> {
    let m = write_as_admin(&db, &identity, &headers, manifest::reset)?;
    tracing::info!("agent manifest reset to the default");
    Ok(Json(admin_json(&m)))
}

#[cfg(test)]
mod tests {
    use crate::api::test_helpers::*;
    use axum::http::StatusCode;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn admin_edits_and_agents_read_the_rendered_block() {
        let (db, _admin, lead, _maintainer, _viewer, _non_member, _pid) = setup_membership_test();
        let admin = test_app();
        let data = parse_json(json_get(&admin, "/api/instance/agent-manifest").await).await;
        assert_eq!(data["is_default"], true);
        assert_eq!(data["content"], data["default"]);

        let resp = json_put(
            &admin,
            "/api/instance/agent-manifest",
            serde_json::json!({ "content": "Work on {{project}} only." }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let saved = parse_json(resp).await;
        assert_eq!(saved["is_default"], false);
        assert!(saved["preview"].as_str().unwrap().contains("Work on APP only."));

        // Markers are refused: they are added around the text.
        let resp = json_put(
            &admin,
            "/api/instance/agent-manifest",
            serde_json::json!({ "content": "<!-- lific:begin --> x" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // The rendered block, as an agent fetches it.
        let md = json_get(&admin, "/api/agent/manifest?project=LIF").await;
        assert_eq!(md.status(), StatusCode::OK);
        assert!(md.headers()["content-type"].to_str().unwrap().starts_with("text/markdown"));
        let body = String::from_utf8(md.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(body.starts_with("<!-- lific:begin -->"), "{body}");
        assert!(body.contains("Work on LIF only."), "{body}");

        let reset = parse_json(json_delete(&admin, "/api/instance/agent-manifest").await).await;
        assert_eq!(reset["is_default"], true);

        // Anyone signed in (here on another, untouched instance) reads the
        // block; only an admin edits it.
        let person = app_as_user(db.clone(), &lead);
        let json = parse_json(json_get(&person, "/api/agent/manifest?format=json").await).await;
        assert_eq!(json["is_default"], true);
        assert!(json["markdown"].as_str().unwrap().contains("lific:end"));
        assert_eq!(
            json_get(&person, "/api/instance/agent-manifest").await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            json_put(&person, "/api/instance/agent-manifest", serde_json::json!({ "content": "x" }))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
}
