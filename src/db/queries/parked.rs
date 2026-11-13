//! Fork: parked issues ("Отложено").
//!
//! A parked issue is an ordinary issue — same status, comments, links and
//! history — that nobody means to work on now: a nice-to-have, an idea. It
//! stays out of the way until someone brings it back: off the board and the
//! issue list (they get their own "Parked" mode), out of Home, For me and the
//! flow statistics, out of project counts, and out of what agents read by
//! default (`list_issues`, `workable`, the briefing). Search, direct links
//! and `get_issue` still find it, marked.
//!
//! Parking is a row in `fork_issue_parked`, not a status, so it needs no
//! rebuild of `issues` and keeps clear of upstream's numbered migrations.
//! Taking the issue up again unparks it on its own: any status change, or
//! assigning someone. Triggers do that, so every path (web, MCP, CLI, git
//! hook, bulk) agrees.
//!
//! Created with the other fork tables (see `agent_inbox::ensure_schema`).

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::models::IssueParked;
use crate::error::LificError;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS fork_issue_parked (
    issue_id   INTEGER PRIMARY KEY REFERENCES issues(id) ON DELETE CASCADE,
    parked_by  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    reason     TEXT    NOT NULL DEFAULT '',
    parked_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Sync: parking is activity on the issue, so /changes re-delivers it.
CREATE TRIGGER IF NOT EXISTS fork_issue_parked_bump_ai AFTER INSERT ON fork_issue_parked BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues SET updated_at = datetime('now'), seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = NEW.issue_id;
END;
CREATE TRIGGER IF NOT EXISTS fork_issue_parked_bump_au AFTER UPDATE ON fork_issue_parked BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues SET updated_at = datetime('now'), seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = NEW.issue_id;
END;
CREATE TRIGGER IF NOT EXISTS fork_issue_parked_bump_ad AFTER DELETE ON fork_issue_parked BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues SET updated_at = datetime('now'), seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = OLD.issue_id;
END;

-- History: 'park' with the reason, 'unpark' with the reason it had.
CREATE TRIGGER IF NOT EXISTS fork_audit_issue_park AFTER INSERT ON fork_issue_parked BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, new_value)
    VALUES (
        COALESCE((SELECT user_id FROM _actor_state WHERE id = 1), NEW.parked_by),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', NEW.issue_id,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = NEW.issue_id),
        (SELECT project_id FROM issues WHERE id = NEW.issue_id),
        NEW.issue_id, 'park', 'parked', NEW.reason
    );
END;
CREATE TRIGGER IF NOT EXISTS fork_audit_issue_unpark AFTER DELETE ON fork_issue_parked
WHEN EXISTS (SELECT 1 FROM issues WHERE id = OLD.issue_id AND deleted_at IS NULL)
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', OLD.issue_id,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = OLD.issue_id),
        (SELECT project_id FROM issues WHERE id = OLD.issue_id),
        OLD.issue_id, 'unpark', 'parked', OLD.reason
    );
END;

-- Taking the issue up unparks it: a status change, or naming who does it.
CREATE TRIGGER IF NOT EXISTS fork_issue_parked_status AFTER UPDATE OF status ON issues
WHEN NEW.status <> OLD.status
 AND EXISTS (SELECT 1 FROM fork_issue_parked WHERE issue_id = NEW.id)
BEGIN
    DELETE FROM fork_issue_parked WHERE issue_id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS fork_issue_parked_assign AFTER INSERT ON issue_assignees
WHEN EXISTS (SELECT 1 FROM fork_issue_parked WHERE issue_id = NEW.issue_id)
BEGIN
    DELETE FROM fork_issue_parked WHERE issue_id = NEW.issue_id;
END;
";

/// Longest reason kept.
pub const MAX_REASON: usize = 500;

/// SQL condition: the issue aliased `alias` is not parked.
pub fn not_parked(alias: &str) -> String {
    format!("NOT EXISTS (SELECT 1 FROM fork_issue_parked fp WHERE fp.issue_id = {alias}.id)")
}

/// SQL condition: the issue aliased `alias` is parked.
pub fn is_parked(alias: &str) -> String {
    format!("EXISTS (SELECT 1 FROM fork_issue_parked fp WHERE fp.issue_id = {alias}.id)")
}

const SELECT: &str = "SELECT f.issue_id, f.reason, f.parked_at, u.username,
                             COALESCE(NULLIF(u.display_name, ''), u.username)
                        FROM fork_issue_parked f LEFT JOIN users u ON u.id = f.parked_by";

fn read_row(r: &rusqlite::Row) -> rusqlite::Result<(i64, IssueParked)> {
    Ok((
        r.get(0)?,
        IssueParked {
            reason: r.get(1)?,
            at: r.get(2)?,
            by: r.get(3)?,
            by_name: r.get(4)?,
        },
    ))
}

pub fn get(conn: &Connection, issue_id: i64) -> Result<Option<IssueParked>, LificError> {
    Ok(conn
        .query_row(&format!("{SELECT} WHERE f.issue_id = ?1"), params![issue_id], read_row)
        .optional()?
        .map(|(_, p)| p))
}

pub fn by_issue(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, IssueParked>, LificError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE f.issue_id IN ({placeholders})"))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids), read_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Parked issues per project.
#[cfg(test)]
pub fn counts_by_project(conn: &Connection) -> Result<HashMap<i64, i64>, LificError> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.project_id, COUNT(*) FROM fork_issue_parked f
           JOIN issues i ON i.id = f.issue_id AND i.deleted_at IS NULL
          GROUP BY i.project_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Park an open issue, or change the reason of a parked one.
pub fn park(
    conn: &Connection,
    issue_id: i64,
    reason: &str,
    by: Option<i64>,
) -> Result<IssueParked, LificError> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM issues WHERE id = ?1 AND deleted_at IS NULL",
            params![issue_id],
            |r| r.get(0),
        )
        .optional()?;
    match status.as_deref() {
        None => return Err(LificError::NotFound(format!("issue {issue_id} not found"))),
        Some("done" | "cancelled") => {
            return Err(LificError::BadRequest(
                "a closed issue cannot be parked; reopen it first".into(),
            ));
        }
        Some(_) => {}
    }
    let reason: String = reason.trim().chars().take(MAX_REASON).collect();
    conn.execute(
        "INSERT INTO fork_issue_parked (issue_id, parked_by, reason) VALUES (?1, COALESCE(?2, (SELECT user_id FROM _actor_state WHERE id = 1)), ?3)
         ON CONFLICT (issue_id) DO UPDATE SET reason = excluded.reason
          WHERE fork_issue_parked.reason <> excluded.reason",
        params![issue_id, by, reason],
    )?;
    get(conn, issue_id)?.ok_or_else(|| LificError::NotFound(format!("issue {issue_id} not found")))
}

/// Bring a parked issue back. `false` when it was not parked.
pub fn unpark(conn: &Connection, issue_id: i64) -> Result<bool, LificError> {
    Ok(conn.execute("DELETE FROM fork_issue_parked WHERE issue_id = ?1", params![issue_id])? > 0)
}

#[cfg(test)]
mod tests;
