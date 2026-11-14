//! LIF-507: `GET /api/home/overview`, the Home dashboard's aggregates.
//! Cross-project and filtered to what the caller can see, like
//! `/api/issues/attention`; the counting lives in `db::queries::home`.

use axum::{
    Extension,
    extract::{Json, Query, State},
};

use crate::authz;
use crate::db::DbPool;
use crate::db::queries::flow::{DEFAULT_DAYS, FlowInput, FlowReport, flow};
use crate::db::queries::home::{HomeOverview, OverviewInput, overview};
use crate::error::LificError;

use super::with_read;

#[derive(Debug, serde::Deserialize)]
pub(super) struct HomeQuery {
    /// Summarize what was opened and closed after this ISO 8601 instant.
    pub since: Option<String>,
    /// The caller's offset from UTC in minutes, east positive. Defaults to 0.
    pub tz: Option<i32>,
    /// Fork: only these projects, comma-separated ids. Never widens what
    /// the caller can see.
    pub projects: Option<String>,
}

/// `projects=3,7` as ids; empty or absent means no narrowing.
fn parse_projects(raw: &Option<String>) -> Result<Option<Vec<i64>>, LificError> {
    let Some(raw) = raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    raw.split(',')
        .map(|v| {
            v.trim()
                .parse::<i64>()
                .map_err(|_| LificError::BadRequest(format!("invalid project id '{v}'")))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// GET /api/home/overview?since=...&tz=...
pub(super) async fn home_overview(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Query(q): Query<HomeQuery>,
) -> Result<Json<HomeOverview>, LificError> {
    let user = super::require_user(&identity)?;
    let visible = authz::visible_project_ids(&db, &identity)?;
    // Fork: the Home filter narrows the caller's projects.
    let visible = match parse_projects(&q.projects)? {
        None => visible,
        Some(only) => Some(
            only.into_iter()
                .filter(|id| visible.as_ref().is_none_or(|set| set.contains(id)))
                .collect(),
        ),
    };
    with_read(&db, |conn| {
        let user_id = authz::effective_user(conn, &Some(user)).map(|u| u.id);
        overview(
            conn,
            &OverviewInput {
                visible: visible.as_ref(),
                user_id,
                since: q.since.as_deref(),
                tz_minutes: q.tz.unwrap_or(0),
                now: chrono::Utc::now().naive_utc(),
            },
        )
    })
    .map(Json)
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct FlowQuery {
    /// Window in days, ending today (7 to 365, default 30).
    pub days: Option<i64>,
    pub tz: Option<i32>,
    pub projects: Option<String>,
}

/// Fork: GET /api/home/flow?days=30&tz=180&projects=3,7. Closures per day
/// and per actor, time in each stage, and the work that is stuck now.
pub(super) async fn home_flow(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Query(q): Query<FlowQuery>,
) -> Result<Json<FlowReport>, LificError> {
    super::require_user(&identity)?;
    let visible = authz::visible_project_ids(&db, &identity)?;
    let projects = parse_projects(&q.projects)?;
    with_read(&db, |conn| {
        flow(
            conn,
            &FlowInput {
                visible: visible.as_ref(),
                projects: projects.as_deref(),
                days: q.days.unwrap_or(DEFAULT_DAYS),
                tz_minutes: q.tz.unwrap_or(0),
                now: chrono::Utc::now().naive_utc(),
            },
        )
    })
    .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::api::test_helpers::*;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn overview_returns_every_section() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;
        json_post(
            &app,
            "/api/issues",
            serde_json::json!({ "project_id": project_id, "title": "Open", "priority": "urgent" }),
        )
        .await;

        let resp = json_get(
            &app,
            "/api/home/overview?since=2000-01-01T00:00:00Z&tz=-300",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = parse_json(resp).await;
        assert_eq!(body["open_total"], 1);
        assert_eq!(body["age_buckets"].as_array().unwrap().len(), 5);
        assert_eq!(body["age_buckets"][0]["urgent"], 1);
        assert_eq!(body["age_buckets"][0]["agent_ready"], 1);
        assert_eq!(body["open_trend"].as_array().unwrap().len(), 91);
        assert_eq!(body["projects"][0]["project_id"], project_id);
        assert_eq!(body["since"]["people_opened"], 1);
        assert_eq!(body["my_done"].as_array().unwrap().len(), 84);
    }

    #[tokio::test]
    async fn overview_narrows_to_the_requested_projects() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;
        json_post(
            &app,
            "/api/issues",
            serde_json::json!({ "project_id": project_id, "title": "Open" }),
        )
        .await;
        let all = parse_json(json_get(&app, &format!("/api/home/overview?projects={project_id}")).await).await;
        assert_eq!(all["open_total"], 1);
        let none = parse_json(json_get(&app, "/api/home/overview?projects=999999").await).await;
        assert_eq!(none["open_total"], 0);
        let bad = json_get(&app, "/api/home/overview?projects=x").await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn flow_reports_closures_and_stages() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;
        let resp = json_get(&app, &format!("/api/home/flow?days=14&tz=180&projects={project_id}")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = parse_json(resp).await;
        assert_eq!(body["days"], 14);
        assert_eq!(body["daily"].as_array().unwrap().len(), 14);
        assert_eq!(body["stages"].as_array().unwrap().len(), 4);
        assert_eq!(body["lead_histogram"].as_array().unwrap().len(), 7);
    }

    #[tokio::test]
    async fn flow_is_limited_to_projects_the_caller_can_see() {
        let (db, _admin, lead, _maintainer, _viewer, non_member, project_id) =
            setup_membership_test();
        let lead_app = app_as_user(db.clone(), &lead);
        let created = parse_json(
            json_post(
                &lead_app,
                "/api/issues",
                serde_json::json!({ "project_id": project_id, "title": "Members only", "status": "active" }),
            )
            .await,
        )
        .await;
        assert!(created["id"].is_number());
        let body = parse_json(json_get(&lead_app, "/api/home/flow").await).await;
        assert_eq!(body["active_total"], 1);
        let outsider = app_as_user(db, &non_member);
        let body = parse_json(json_get(&outsider, &format!("/api/home/flow?projects={project_id}")).await).await;
        assert_eq!(body["active_total"], 0);
        assert!(body["active"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn overview_rejects_a_malformed_since() {
        let app = test_app();
        let resp = json_get(&app, "/api/home/overview?since=soon").await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn overview_counts_only_projects_the_caller_can_see() {
        let (db, _admin, lead, _maintainer, _viewer, non_member, project_id) =
            setup_membership_test();
        let lead_app = app_as_user(db.clone(), &lead);
        json_post(
            &lead_app,
            "/api/issues",
            serde_json::json!({ "project_id": project_id, "title": "Members only" }),
        )
        .await;

        let lead_body = parse_json(json_get(&lead_app, "/api/home/overview").await).await;
        assert_eq!(lead_body["open_total"], 1);

        let outsider = app_as_user(db, &non_member);
        let body = parse_json(json_get(&outsider, "/api/home/overview").await).await;
        assert_eq!(body["open_total"], 0);
        assert!(body["projects"].as_array().unwrap().is_empty());
        assert!(body["since"].is_null());
    }
}
