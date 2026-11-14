//! `GET /api/for-me`: open issues a person has to act on, across the
//! projects the caller can see (fork; the For me page). The query lives in
//! `db::queries::for_me`.
//!
//! List filters are comma-separated: `projects=3,7`, `statuses=todo,active`
//! (default: open), `priorities=urgent,high`, `scopes=me,anyone,others`.
//! `requested_by=agents|people` keeps issues an agent or a person handed
//! over. `sort=newest|oldest|updated|priority`, `mine_first=true`.

use axum::{
    Extension,
    extract::{Json, Query, State},
};

use crate::authz;
use crate::db::DbPool;
use crate::db::queries::for_me::{
    ForMeFilter, ForMePage, PRIORITIES, RequestedBy, STATUSES, Scope, Sort, list_for_me,
};
use crate::error::LificError;

use super::with_read;

#[derive(Debug, serde::Deserialize)]
pub(super) struct ForMeQuery {
    pub projects: Option<String>,
    pub statuses: Option<String>,
    pub priorities: Option<String>,
    pub scopes: Option<String>,
    pub requested_by: Option<String>,
    pub sort: Option<String>,
    pub mine_first: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn split(raw: &Option<String>) -> Option<Vec<&str>> {
    raw.as_deref()
        .map(|s| s.split(',').map(str::trim).filter(|v| !v.is_empty()).collect())
}

fn one_of(raw: &Option<String>, allowed: &[&str], what: &str) -> Result<Option<Vec<String>>, LificError> {
    split(raw)
        .map(|values| {
            values
                .into_iter()
                .map(|v| {
                    if allowed.contains(&v) {
                        Ok(v.to_string())
                    } else {
                        Err(LificError::BadRequest(format!(
                            "unknown {what} '{v}'. Use: {}",
                            allowed.join(", ")
                        )))
                    }
                })
                .collect()
        })
        .transpose()
}

/// GET /api/for-me
pub(super) async fn for_me(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Query(q): Query<ForMeQuery>,
) -> Result<Json<ForMePage>, LificError> {
    let user = super::require_user(&identity)?;
    let visible = authz::visible_project_ids(&db, &identity)?;

    let projects = split(&q.projects)
        .map(|values| {
            values
                .into_iter()
                .map(|v| {
                    v.parse::<i64>()
                        .map_err(|_| LificError::BadRequest(format!("invalid project id '{v}'")))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let scopes = split(&q.scopes)
        .map(|values| {
            values
                .into_iter()
                .map(|v| {
                    Scope::parse(v).ok_or_else(|| {
                        LificError::BadRequest(format!("unknown scope '{v}'. Use: me, anyone, others"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let requested_by = match q.requested_by.as_deref() {
        None | Some("") | Some("all") => RequestedBy::All,
        Some("agents") => RequestedBy::Agents,
        Some("people") => RequestedBy::People,
        Some(other) => {
            return Err(LificError::BadRequest(format!(
                "invalid requested_by '{other}'. Use all, agents or people"
            )));
        }
    };
    let sort = match q.sort.as_deref() {
        None | Some("") => Sort::default(),
        Some(v) => Sort::parse(v).ok_or_else(|| {
            LificError::BadRequest(format!("invalid sort '{v}'. Use newest, oldest, updated or priority"))
        })?,
    };
    let statuses = one_of(&q.statuses, STATUSES, "status")?;
    let priorities = one_of(&q.priorities, PRIORITIES, "priority")?;

    with_read(&db, |conn| {
        let me = authz::effective_user(conn, &Some(user)).map(|u| u.id);
        list_for_me(
            conn,
            &ForMeFilter {
                visible: visible.as_ref(),
                me,
                projects,
                statuses,
                priorities,
                scopes,
                requested_by,
                sort,
                mine_first: q.mine_first.unwrap_or(false),
                limit: q.limit,
                offset: q.offset,
            },
        )
    })
    .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::api::test_helpers::*;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn get(app: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn for_me_lists_person_work_and_rejects_bad_input() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;
        json_post(
            &app,
            "/api/issues",
            serde_json::json!({ "project_id": project_id, "title": "Agents only" }),
        )
        .await;
        json_post(
            &app,
            "/api/issues",
            serde_json::json!({ "project_id": project_id, "title": "Sign the contract", "assignees": ["human"] }),
        )
        .await;

        let (status, body) = get(&app, "/api/for-me").await;
        assert_eq!(status, StatusCode::OK);
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "Sign the contract");
        assert_eq!(items[0]["project_identifier"], "TST");
        assert_eq!(body["counts"]["anyone"], 1);

        for bad in ["sort=sideways", "scopes=everyone", "statuses=open", "requested_by=aliens"] {
            let (status, _) = get(&app, &format!("/api/for-me?{bad}")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        }
    }
}
