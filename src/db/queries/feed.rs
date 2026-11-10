//! Cross-project activity feed: the audit log of every project the caller
//! can see, newest first, filtered on the server so a busy instance does not
//! ship hundreds of label attaches to a browser that only wants comments.
//!
//! Each row carries a `kind`, a coarse event class the Feed page filters and
//! styles on. The classification lives in SQL (`KIND_SQL`) so filtering and
//! the value the client sees can never disagree.

use rusqlite::Connection;
use std::collections::HashSet;

use crate::db::models::{Activity, ActorStat};
use crate::error::LificError;

pub const MAX_LIMIT: i64 = 200;
const DEFAULT_LIMIT: i64 = 50;

/// Every value `kind` can take, in the order the Feed page lists them.
pub const KINDS: &[&str] = &[
    "created",
    "status",
    "comment",
    "edit",
    "assignee",
    "label",
    "link",
    "page",
    "plan",
    "structure",
];

const KIND_SQL: &str = "CASE
    WHEN a.entity_type = 'comment' THEN 'comment'
    WHEN a.entity_type = 'issue' AND a.action = 'create' THEN 'created'
    WHEN a.entity_type = 'issue' AND a.action = 'update' AND a.field = 'status' THEN 'status'
    WHEN a.entity_type = 'issue' AND a.action IN ('attach', 'detach') THEN 'label'
    WHEN a.entity_type = 'issue' AND a.action IN ('link', 'unlink', 'wait', 'unwait') THEN 'link'
    WHEN a.entity_type = 'issue' AND a.action IN ('assign', 'unassign') THEN 'assignee'
    WHEN a.entity_type = 'issue' THEN 'edit'
    WHEN a.entity_type = 'page' THEN 'page'
    WHEN a.entity_type IN ('plan', 'plan_step') THEN 'plan'
    ELSE 'structure'
END";

/// Who did it: a user (person or agent), or the unattributed system bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorRef {
    User(i64),
    System,
}

/// Narrow the feed to agents, to people, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentFilter {
    #[default]
    All,
    Only,
    Exclude,
}

#[derive(Debug, Default)]
pub struct FeedFilter<'a> {
    /// `None` = every project (admin, or authz off); `Some` = these only.
    pub visible: Option<&'a HashSet<i64>>,
    pub projects: Option<Vec<i64>>,
    pub kinds: Option<Vec<String>>,
    pub actors: Option<Vec<ActorRef>>,
    pub agents: AgentFilter,
    /// Rows strictly older than this id (paging down).
    pub before: Option<i64>,
    /// Rows strictly newer than this id (live updates).
    pub after: Option<i64>,
    pub limit: Option<i64>,
}

/// One audit row plus the context a cross-project feed needs to render it
/// without a request per row.
#[derive(Debug, serde::Serialize)]
pub struct FeedItem {
    #[serde(flatten)]
    pub activity: Activity,
    pub kind: String,
    pub project_identifier: Option<String>,
    pub project_name: Option<String>,
    /// Current title and status of the issue the row belongs to, when it
    /// still exists (a deleted issue keeps its label but loses these).
    pub issue_title: Option<String>,
    pub issue_status: Option<String>,
    pub page_title: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct FeedPage {
    pub items: Vec<FeedItem>,
    pub has_more: bool,
}

pub fn list_feed(conn: &Connection, filter: &FeedFilter) -> Result<FeedPage, LificError> {
    let (limit, _) = super::page_with(filter.limit, None, DEFAULT_LIMIT, MAX_LIMIT);

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut clauses: Vec<String> = vec!["a.project_id IS NOT NULL".into()];

    if let Some(visible) = filter.visible {
        if visible.is_empty() {
            return Ok(FeedPage { items: Vec::new(), has_more: false });
        }
        let marks = in_list(visible.iter().map(|&id| boxed(id)).collect(), &mut params);
        clauses.push(format!("a.project_id IN ({marks})"));
    }
    if let Some(projects) = &filter.projects {
        if projects.is_empty() {
            return Ok(FeedPage { items: Vec::new(), has_more: false });
        }
        let marks = in_list(projects.iter().map(|&id| boxed(id)).collect(), &mut params);
        clauses.push(format!("a.project_id IN ({marks})"));
    }
    if let Some(kinds) = &filter.kinds {
        if kinds.is_empty() {
            return Ok(FeedPage { items: Vec::new(), has_more: false });
        }
        let marks = in_list(kinds.iter().map(|k| boxed(k.clone())).collect(), &mut params);
        clauses.push(format!("({KIND_SQL}) IN ({marks})"));
    }
    if let Some(actors) = &filter.actors {
        if actors.is_empty() {
            return Ok(FeedPage { items: Vec::new(), has_more: false });
        }
        let users: Vec<i64> = actors
            .iter()
            .filter_map(|a| match a {
                ActorRef::User(id) => Some(*id),
                ActorRef::System => None,
            })
            .collect();
        let mut alternatives = Vec::new();
        if !users.is_empty() {
            let marks = in_list(users.into_iter().map(boxed).collect(), &mut params);
            alternatives.push(format!("a.actor_user_id IN ({marks})"));
        }
        if actors.contains(&ActorRef::System) {
            alternatives.push("a.actor_user_id IS NULL".into());
        }
        clauses.push(format!("({})", alternatives.join(" OR ")));
    }
    match filter.agents {
        AgentFilter::All => {}
        AgentFilter::Only => clauses.push("COALESCE(u.is_bot, 0) = 1".into()),
        AgentFilter::Exclude => clauses.push("COALESCE(u.is_bot, 0) = 0".into()),
    }
    if let Some(before) = filter.before {
        params.push(boxed(before));
        clauses.push(format!("a.id < ?{}", params.len()));
    }
    if let Some(after) = filter.after {
        params.push(boxed(after));
        clauses.push(format!("a.id > ?{}", params.len()));
    }

    params.push(boxed(super::over_fetch(limit)));
    let sql = format!(
        "SELECT a.id, a.ts, a.actor_user_id,
                COALESCE(a.imported_author, u.username), COALESCE(a.imported_author, u.display_name),
                COALESCE(u.is_bot, 0), a.transport, a.entity_type, a.entity_id,
                a.entity_label, a.project_id, a.issue_id, a.page_id,
                a.action, a.field, a.old_value, a.new_value,
                {KIND_SQL}, p.identifier, p.name,
                i.title, i.status, pg.title
         FROM audit_log a
         LEFT JOIN users u ON u.id = a.actor_user_id
         LEFT JOIN projects p ON p.id = a.project_id
         LEFT JOIN issues i ON i.id = a.issue_id AND i.deleted_at IS NULL
         LEFT JOIN pages pg ON pg.id = a.page_id AND pg.deleted_at IS NULL
         WHERE {}
         ORDER BY a.id DESC
         LIMIT ?{}",
        clauses.join(" AND "),
        params.len(),
    );

    let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(FeedItem {
            activity: Activity {
                id: row.get(0)?,
                ts: row.get(1)?,
                actor_user_id: row.get(2)?,
                actor_username: row.get(3)?,
                actor_display_name: row.get(4)?,
                actor_is_bot: row.get::<_, i64>(5)? != 0,
                transport: row.get(6)?,
                entity_type: row.get(7)?,
                entity_id: row.get(8)?,
                entity_label: row.get(9)?,
                project_id: row.get(10)?,
                issue_id: row.get(11)?,
                page_id: row.get(12)?,
                action: row.get(13)?,
                field: row.get(14)?,
                old_value: row.get(15)?,
                new_value: row.get(16)?,
            },
            kind: row.get(17)?,
            project_identifier: row.get(18)?,
            project_name: row.get(19)?,
            issue_title: row.get(20)?,
            issue_status: row.get(21)?,
            page_title: row.get(22)?,
        })
    })?;
    let items = rows.collect::<Result<Vec<_>, _>>()?;
    let page = super::Page::from_over_fetch(items, limit);
    Ok(FeedPage { items: page.items, has_more: page.has_more })
}

/// Everyone who has acted in the visible projects, most active first. The
/// Feed's actor filter lists these.
pub fn feed_actors(
    conn: &Connection,
    visible: Option<&HashSet<i64>>,
) -> Result<Vec<ActorStat>, LificError> {
    let (project_filter, ids): (String, Vec<i64>) = match visible {
        None => (String::new(), Vec::new()),
        Some(ids) if ids.is_empty() => return Ok(Vec::new()),
        Some(ids) => (
            format!(
                " AND a.project_id IN ({})",
                (1..=ids.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ")
            ),
            ids.iter().copied().collect(),
        ),
    };
    let sql = format!(
        "SELECT a.actor_user_id, u.username, u.display_name, COALESCE(u.is_bot, 0),
                COUNT(*) AS actions, MAX(a.ts),
                (SELECT t.transport FROM audit_log t
                 WHERE t.actor_user_id IS a.actor_user_id
                 GROUP BY t.transport ORDER BY COUNT(*) DESC LIMIT 1)
         FROM audit_log a
         LEFT JOIN users u ON u.id = a.actor_user_id
         WHERE a.project_id IS NOT NULL{project_filter}
         GROUP BY a.actor_user_id
         ORDER BY actions DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids), |row| {
        Ok(ActorStat {
            actor_user_id: row.get(0)?,
            username: row.get(1)?,
            display_name: row.get(2)?,
            is_bot: row.get::<_, i64>(3)? != 0,
            actions: row.get(4)?,
            last_ts: row.get(5)?,
            top_transport: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(LificError::Database)
}

/// Append `values` as positional parameters and return their `?N` marks.
fn in_list(
    values: Vec<Box<dyn rusqlite::types::ToSql>>,
    params: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
) -> String {
    let start = params.len();
    let marks = (0..values.len())
        .map(|i| format!("?{}", start + i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    params.extend(values);
    marks
}

fn boxed<T: rusqlite::types::ToSql + 'static>(value: T) -> Box<dyn rusqlite::types::ToSql> {
    Box::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::*;
    use crate::db::queries;

    fn project(conn: &Connection, ident: &str) -> i64 {
        queries::create_project(
            conn,
            &CreateProject {
                name: ident.into(),
                identifier: ident.into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id
    }

    fn issue(conn: &Connection, project_id: i64, title: &str) -> Issue {
        queries::create_issue(
            conn,
            &CreateIssue {
                project_id,
                title: title.into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn kinds(page: &FeedPage) -> Vec<&str> {
        page.items.iter().map(|i| i.kind.as_str()).collect()
    }

    #[test]
    fn classifies_filters_and_scopes_rows() {
        let pool = crate::db::open_memory().unwrap();
        let (first, second, created) = {
            let conn = pool.write().unwrap();
            let first = project(&conn, "ONE");
            let second = project(&conn, "TWO");
            let created = issue(&conn, first, "Fix login");
            queries::update_issue(
                &conn,
                created.id,
                &UpdateIssue {
                    status: Some(Status::Active),
                    ..Default::default()
                },
            )
            .unwrap();
            issue(&conn, second, "Elsewhere");
            (first, second, created)
        };
        let conn = pool.read().unwrap();

        let all = list_feed(&conn, &FeedFilter::default()).unwrap();
        let status = all.items.iter().find(|i| i.kind == "status").expect("status row");
        assert_eq!(status.activity.new_value.as_deref(), Some("active"));
        assert_eq!(status.project_identifier.as_deref(), Some("ONE"));
        assert_eq!(status.issue_title.as_deref(), Some("Fix login"));
        assert_eq!(status.issue_status.as_deref(), Some("active"));

        let only_status = list_feed(
            &conn,
            &FeedFilter {
                kinds: Some(vec!["status".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(kinds(&only_status), vec!["status"]);

        let visible = HashSet::from([second]);
        let scoped = list_feed(
            &conn,
            &FeedFilter {
                visible: Some(&visible),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(scoped.items.iter().all(|i| i.activity.project_id == Some(second)));
        assert!(!scoped.items.is_empty());

        // An explicit project filter cannot widen past what is visible.
        let widened = list_feed(
            &conn,
            &FeedFilter {
                visible: Some(&visible),
                projects: Some(vec![first]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(widened.items.is_empty());

        let created_rows = list_feed(
            &conn,
            &FeedFilter {
                projects: Some(vec![first]),
                kinds: Some(vec!["created".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(created_rows.items.len(), 1);
        assert_eq!(created_rows.items[0].activity.issue_id, Some(created.id));
    }

    #[test]
    fn pages_by_cursor_and_filters_actors() {
        let pool = crate::db::open_memory().unwrap();
        {
            let conn = pool.write().unwrap();
            let pid = project(&conn, "CUR");
            for n in 0..5 {
                issue(&conn, pid, &format!("i{n}"));
            }
        }
        let conn = pool.read().unwrap();

        let first = list_feed(&conn, &FeedFilter { limit: Some(3), ..Default::default() }).unwrap();
        assert_eq!(first.items.len(), 3);
        assert!(first.has_more);
        let oldest = first.items.last().unwrap().activity.id;
        let newest = first.items[0].activity.id;

        let rest = list_feed(
            &conn,
            &FeedFilter {
                before: Some(oldest),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(rest.items.iter().all(|i| i.activity.id < oldest));
        assert!(!rest.has_more);

        let newer = list_feed(
            &conn,
            &FeedFilter {
                after: Some(newest),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(newer.items.is_empty());

        // Test writes are unattributed: the system bucket has them, agents none.
        let system = list_feed(
            &conn,
            &FeedFilter {
                actors: Some(vec![ActorRef::System]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!system.items.is_empty());
        let agents = list_feed(
            &conn,
            &FeedFilter {
                agents: AgentFilter::Only,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(agents.items.is_empty());

        let actors = feed_actors(&conn, None).unwrap();
        assert_eq!(actors.len(), 1);
        assert_eq!(actors[0].actor_user_id, None);
    }
}
