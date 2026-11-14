//! Fork: REST for parked issues (see `db::queries::parked`).
//!
//! `PUT /api/issues/{id}/park` parks an issue or changes its reason,
//! `DELETE` brings it back, and `POST /api/issues/park` does either for many
//! issues at once (the web's bulk bar). Maintainer, like any issue edit. Each
//! write advances the issue's seq through the table's triggers and publishes
//! `issue.updated`, so read models pull the row.

use axum::{
    Extension,
    extract::{Json, Path, State},
};
use serde::Deserialize;

use crate::authz;
use crate::db::queries::{self, parked};
use crate::db::{DbPool, models::*};
use crate::error::LificError;
use crate::realtime::{RealtimeEvent, RealtimeHub};

use super::with_read;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ParkInput {
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct BulkParkInput {
    pub ids: Vec<i64>,
    pub parked: bool,
    #[serde(default)]
    pub reason: String,
}

/// Most issues one bulk call may touch.
const BULK_CAP: usize = 500;

pub(super) async fn park_issue(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(id): Path<i64>,
    body: Option<Json<ParkInput>>,
) -> Result<Json<IssueParked>, LificError> {
    let input = body.map(|Json(b)| b).unwrap_or_default();
    let project_id = with_read(&db, |conn| queries::issue_project_id(conn, id))?;
    authz::require_role(&db, &identity, project_id, Role::Maintainer)?;
    let user = super::require_user(&identity)?;
    let (result, seq) = db.transaction(|conn| {
        authz::require_role_conn(conn, &identity, project_id, Role::Maintainer)?;
        let result = parked::park(conn, id, &input.reason, Some(user.id))?;
        Ok((result, queries::issue_seq(conn, id)?))
    })?;
    realtime.send_with_seq(RealtimeEvent::IssueUpdated { project_id, issue_id: id }, seq);
    Ok(Json(result))
}

pub(super) async fn unpark_issue(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, LificError> {
    let project_id = with_read(&db, |conn| queries::issue_project_id(conn, id))?;
    authz::require_role(&db, &identity, project_id, Role::Maintainer)?;
    let (changed, seq) = db.transaction(|conn| {
        authz::require_role_conn(conn, &identity, project_id, Role::Maintainer)?;
        let changed = parked::unpark(conn, id)?;
        Ok((changed, queries::issue_seq(conn, id)?))
    })?;
    if changed {
        realtime.send_with_seq(RealtimeEvent::IssueUpdated { project_id, issue_id: id }, seq);
    }
    Ok(Json(serde_json::json!({ "unparked": changed })))
}

/// All or nothing: one closed issue or one project the caller cannot edit
/// refuses the whole call.
pub(super) async fn bulk_park(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Json(input): Json<BulkParkInput>,
) -> Result<Json<serde_json::Value>, LificError> {
    let user = super::require_user(&identity)?;
    let mut ids = input.ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() > BULK_CAP {
        return Err(LificError::BadRequest(format!("at most {BULK_CAP} issues per call")));
    }
    let events = db.transaction(|conn| {
        let mut events = Vec::with_capacity(ids.len());
        for &id in &ids {
            let project_id = queries::issue_project_id(conn, id)?;
            authz::require_role_conn(conn, &identity, project_id, Role::Maintainer)?;
            let changed = if input.parked {
                parked::park(conn, id, &input.reason, Some(user.id))?;
                true
            } else {
                parked::unpark(conn, id)?
            };
            if changed {
                events.push((project_id, id, queries::issue_seq(conn, id)?));
            }
        }
        Ok(events)
    })?;
    for &(project_id, issue_id, seq) in &events {
        realtime.send_with_seq(RealtimeEvent::IssueUpdated { project_id, issue_id }, seq);
    }
    Ok(Json(serde_json::json!({ "changed": events.len() })))
}
