//! `GET /api/feed` and `GET /api/feed/actors`: the cross-project activity
//! feed behind the Feed page. Filtered to the projects the caller can see,
//! like `/api/home/overview`; the query lives in `db::queries::feed`.
//!
//! List filters are comma-separated: `projects=3,7`, `kinds=status,comment`,
//! `actors=12,system`. `agents=only|exclude` keeps or drops agent rows.
//! `before` pages down from an id, `after` fetches what is newer than one.

use axum::{
    Extension,
    extract::{Json, Query, State},
};

use crate::authz;
use crate::db::DbPool;
use crate::db::models::ActorStat;
use crate::db::queries::feed::{
    ActorRef, AgentFilter, FeedFilter, FeedPage, KINDS, feed_actors, list_feed,
};
use crate::error::LificError;

use super::with_read;

#[derive(Debug, serde::Deserialize)]
pub(super) struct FeedQuery {
    pub projects: Option<String>,
    pub kinds: Option<String>,
    pub actors: Option<String>,
    pub agents: Option<String>,
    pub before: Option<i64>,
    pub after: Option<i64>,
    pub limit: Option<i64>,
}

fn split(raw: &Option<String>) -> Option<Vec<&str>> {
    raw.as_deref()
        .map(|s| s.split(',').map(str::trim).filter(|v| !v.is_empty()).collect())
}

fn parse_ids(raw: &Option<String>, what: &str) -> Result<Option<Vec<i64>>, LificError> {
    split(raw)
        .map(|values| {
            values
                .into_iter()
                .map(|v| {
                    v.parse::<i64>()
                        .map_err(|_| LificError::BadRequest(format!("invalid {what} id '{v}'")))
                })
                .collect()
        })
        .transpose()
}

impl FeedQuery {
    fn kinds(&self) -> Result<Option<Vec<String>>, LificError> {
        split(&self.kinds)
            .map(|values| {
                values
                    .into_iter()
                    .map(|k| {
                        if KINDS.contains(&k) {
                            Ok(k.to_string())
                        } else {
                            Err(LificError::BadRequest(format!(
                                "unknown kind '{k}'. Use: {}",
                                KINDS.join(", ")
                            )))
                        }
                    })
                    .collect()
            })
            .transpose()
    }

    fn actors(&self) -> Result<Option<Vec<ActorRef>>, LificError> {
        split(&self.actors)
            .map(|values| {
                values
                    .into_iter()
                    .map(|v| match v {
                        "system" => Ok(ActorRef::System),
                        id => id.parse::<i64>().map(ActorRef::User).map_err(|_| {
                            LificError::BadRequest(format!(
                                "invalid actor '{id}'. Use user ids or 'system'"
                            ))
                        }),
                    })
                    .collect()
            })
            .transpose()
    }

    fn agents(&self) -> Result<AgentFilter, LificError> {
        match self.agents.as_deref() {
            None | Some("") | Some("all") => Ok(AgentFilter::All),
            Some("only") => Ok(AgentFilter::Only),
            Some("exclude") => Ok(AgentFilter::Exclude),
            Some(other) => Err(LificError::BadRequest(format!(
                "invalid agents '{other}'. Use all, only or exclude"
            ))),
        }
    }
}

/// GET /api/feed
pub(super) async fn feed(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Query(q): Query<FeedQuery>,
) -> Result<Json<FeedPage>, LificError> {
    super::require_user(&identity)?;
    let visible = authz::visible_project_ids(&db, &identity)?;
    let filter = FeedFilter {
        visible: visible.as_ref(),
        projects: parse_ids(&q.projects, "project")?,
        kinds: q.kinds()?,
        actors: q.actors()?,
        agents: q.agents()?,
        before: q.before,
        after: q.after,
        limit: q.limit,
    };
    with_read(&db, |conn| list_feed(conn, &filter)).map(Json)
}

/// GET /api/feed/actors
pub(super) async fn actors(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
) -> Result<Json<Vec<ActorStat>>, LificError> {
    super::require_user(&identity)?;
    let visible = authz::visible_project_ids(&db, &identity)?;
    with_read(&db, |conn| feed_actors(conn, visible.as_ref())).map(Json)
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
    async fn feed_lists_filters_and_rejects_bad_input() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;
        json_post(
            &app,
            "/api/issues",
            serde_json::json!({ "project_id": project_id, "title": "Feed me" }),
        )
        .await;

        let (status, body) = get(&app, "/api/feed").await;
        assert_eq!(status, StatusCode::OK);
        let items = body["items"].as_array().unwrap();
        let created = items.iter().find(|i| i["kind"] == "created").expect("created row");
        assert_eq!(created["issue_title"], "Feed me");
        assert_eq!(created["project_identifier"], "TST");

        let (_, only) = get(&app, &format!("/api/feed?kinds=created&projects={project_id}")).await;
        assert!(only["items"].as_array().unwrap().iter().all(|i| i["kind"] == "created"));

        let (status, _) = get(&app, "/api/feed?kinds=bogus").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = get(&app, "/api/feed?actors=nobody").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = get(&app, "/api/feed?agents=maybe").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, actors) = get(&app, "/api/feed/actors").await;
        assert_eq!(status, StatusCode::OK);
        assert!(!actors.as_array().unwrap().is_empty());
    }
}
