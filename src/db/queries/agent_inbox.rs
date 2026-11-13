//! Fork: the agent inbox. A comment on an issue can be addressed to agents
//! (bot accounts, each owned by a person); the addressed agents find it in
//! their inbox, which they read between steps over MCP (`check_inbox`) or a
//! harness hook over REST.
//!
//! Who a comment reaches, first rule that yields anyone wins:
//! 1. **Explicit**: agents picked in the composer, plus agents @mentioned in
//!    the text (by username). Any author, agents included, can address an
//!    agent this way.
//! 2. **Issue**: for a comment written by a person, the agents attached to
//!    the issue (it created it, last took it into progress, asked a person on
//!    it, waits on a person there, or commented on it) and agents routed to
//!    the issue by hand.
//! 3. **Project**: agents routed to the whole project by hand.
//!
//! An agent never receives its own comment, and only reaches comments in
//! projects its owner can see. Routes are set by the agent's owner (or an
//! admin).
//!
//! The tables are not a numbered migration: the migration runner applies
//! versions above the highest one recorded, so a fork-only number would
//! block every later upstream migration. They are created here, idempotently,
//! every time the database opens.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::LificError;

const INBOX_TABLE: &str = "
CREATE TABLE IF NOT EXISTS fork_agent_inbox (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- A comment addressed to the agent, or (comment_id NULL) a notice about
    -- an issue, such as being assigned to it.
    comment_id   INTEGER REFERENCES comments(id) ON DELETE CASCADE,
    issue_id     INTEGER REFERENCES issues(id) ON DELETE CASCADE,
    actor_id     INTEGER REFERENCES users(id) ON DELETE SET NULL,
    reason       TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    delivered_at TEXT,
    UNIQUE (agent_id, comment_id),
    CHECK (comment_id IS NOT NULL OR issue_id IS NOT NULL)
);
";

const SCHEMA: &str = "
CREATE INDEX IF NOT EXISTS fork_agent_inbox_pending
    ON fork_agent_inbox (agent_id, id) WHERE delivered_at IS NULL;
CREATE INDEX IF NOT EXISTS fork_agent_inbox_comment ON fork_agent_inbox (comment_id);

CREATE TABLE IF NOT EXISTS fork_agent_routes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    issue_id   INTEGER REFERENCES issues(id) ON DELETE CASCADE,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS fork_agent_routes_project
    ON fork_agent_routes (agent_id, project_id) WHERE issue_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS fork_agent_routes_issue
    ON fork_agent_routes (agent_id, issue_id) WHERE issue_id IS NOT NULL;
";

pub fn ensure_schema(conn: &Connection) -> Result<(), LificError> {
    let has_inbox: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'fork_agent_inbox')",
        [],
        |r| r.get(0),
    )?;
    let has_issue_col: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info('fork_agent_inbox') WHERE name = 'issue_id')",
        [],
        |r| r.get(0),
    )?;
    if has_inbox && !has_issue_col {
        // The first version kept comments only (comment_id NOT NULL). SQLite
        // cannot relax a NOT NULL in place, so copy into the new shape.
        conn.execute_batch(&format!(
            "SAVEPOINT fork_inbox_v2;
             ALTER TABLE fork_agent_inbox RENAME TO fork_agent_inbox_v1;
             DROP INDEX IF EXISTS fork_agent_inbox_pending;
             DROP INDEX IF EXISTS fork_agent_inbox_comment;
             {INBOX_TABLE}
             INSERT INTO fork_agent_inbox (id, agent_id, comment_id, reason, created_at, delivered_at)
                  SELECT id, agent_id, comment_id, reason, created_at, delivered_at
                    FROM fork_agent_inbox_v1
                   WHERE agent_id IN (SELECT id FROM users)
                     AND comment_id IN (SELECT id FROM comments);
             DROP TABLE fork_agent_inbox_v1;
             RELEASE fork_inbox_v2;"
        ))?;
    }
    conn.execute_batch(INBOX_TABLE)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch(super::agent_workspaces::SCHEMA)?;
    conn.execute_batch(super::parked::SCHEMA)?;
    Ok(())
}

/// Why an agent got a comment. Stored as text.
pub mod reason {
    pub const PICKED: &str = "picked";
    pub const MENTION: &str = "mention";
    pub const ATTACHED: &str = "attached";
    pub const ISSUE_ROUTE: &str = "issue_route";
    pub const PROJECT_ROUTE: &str = "project_route";
    /// A notice, not a comment: a person assigned the agent to the issue.
    pub const ASSIGNED: &str = "assigned";
}

/// An agent as people see it: the bot and whose it is.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Agent {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub owner_id: Option<i64>,
    pub owner_name: Option<String>,
}

const AGENT_SELECT: &str = "SELECT b.id, b.username, COALESCE(NULLIF(b.display_name, ''), b.username),
        b.owner_id, COALESCE(NULLIF(o.display_name, ''), o.username)
   FROM users b LEFT JOIN users o ON o.id = b.owner_id
  WHERE b.is_bot = 1 AND b.is_active = 1 AND (o.id IS NULL OR o.is_active = 1)";

fn agent_row(row: &rusqlite::Row) -> rusqlite::Result<Agent> {
    Ok(Agent {
        id: row.get(0)?,
        username: row.get(1)?,
        display_name: row.get(2)?,
        owner_id: row.get(3)?,
        owner_name: row.get(4)?,
    })
}

/// Active agents whose owner can see `project_id`, by owner then name.
pub fn agents_for_project(conn: &Connection, project_id: i64) -> Result<Vec<Agent>, LificError> {
    let enforced = crate::authz::authz_enforced_conn(conn)?;
    let mut stmt = conn.prepare(&format!(
        "{AGENT_SELECT}
            AND (?2 = 0 OR o.is_admin = 1
                 OR EXISTS (SELECT 1 FROM project_members m
                             WHERE m.project_id = ?1 AND m.user_id = o.id))
          ORDER BY o.username COLLATE NOCASE, b.username COLLATE NOCASE"
    ))?;
    let rows = stmt
        .query_map(params![project_id, enforced], agent_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_agent(conn: &Connection, id: i64) -> Result<Option<Agent>, LificError> {
    Ok(conn
        .query_row(&format!("{AGENT_SELECT} AND b.id = ?1"), params![id], agent_row)
        .optional()?)
}

fn is_bot(conn: &Connection, user_id: i64) -> Result<bool, LificError> {
    Ok(conn
        .query_row("SELECT is_bot FROM users WHERE id = ?1", params![user_id], |r| r.get(0))
        .optional()?
        .unwrap_or(false))
}

fn issue_project(conn: &Connection, issue_id: i64) -> Result<Option<i64>, LificError> {
    Ok(conn
        .query_row(
            "SELECT project_id FROM issues WHERE id = ?1",
            params![issue_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Agents attached to an issue by what they did on it, with the reasons.
/// Keys of the map are agent ids; values are short reason codes
/// (`assigned`, `created`, `took`, `asked`, `waits`, `commented`, `routed`).
pub fn attached_agents(
    conn: &Connection,
    issue_id: i64,
) -> Result<BTreeMap<i64, Vec<&'static str>>, LificError> {
    let mut out: BTreeMap<i64, Vec<&'static str>> = BTreeMap::new();
    let queries: [(&'static str, &str); 7] = [
        (
            "assigned",
            "SELECT user_id FROM issue_assignees WHERE issue_id = ?1 AND user_id IS NOT NULL",
        ),
        (
            "created",
            "SELECT DISTINCT a.actor_user_id FROM audit_log a
              WHERE a.entity_type = 'issue' AND a.entity_id = ?1 AND a.action = 'create'",
        ),
        (
            "took",
            "SELECT DISTINCT actor_user_id FROM status_transitions
              WHERE issue_id = ?1 AND to_status = 'active'",
        ),
        (
            "asked",
            "SELECT DISTINCT actor_user_id FROM audit_log
              WHERE entity_type = 'issue' AND entity_id = ?1 AND action = 'assign'",
        ),
        (
            "waits",
            "SELECT DISTINCT actor_user_id FROM audit_log
              WHERE entity_type = 'issue' AND entity_id = ?1 AND action = 'wait' AND field = 'user'",
        ),
        (
            "commented",
            "SELECT DISTINCT user_id FROM comments WHERE issue_id = ?1 AND deleted_at IS NULL",
        ),
        (
            "routed",
            "SELECT agent_id FROM fork_agent_routes WHERE issue_id = ?1",
        ),
    ];
    for (why, sql) in queries {
        let ids: Vec<i64> = conn
            .prepare_cached(&format!(
                "SELECT x.id FROM users x WHERE x.is_bot = 1 AND x.is_active = 1
                    AND x.id IN ({sql})"
            ))?
            .query_map(params![issue_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for id in ids {
            out.entry(id).or_default().push(why);
        }
    }
    Ok(out)
}

/// One delivery of a comment to an agent.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Delivery {
    pub agent: Agent,
    pub reason: String,
    pub created_at: String,
    /// When the agent picked it up; `None` while it waits in the inbox.
    pub delivered_at: Option<String>,
}

/// Route a new comment and fill the inboxes. `picks` are agent ids chosen
/// in the composer. Returns the agents it reached.
pub fn route_comment(
    conn: &Connection,
    comment_id: i64,
    issue_id: Option<i64>,
    author_id: i64,
    content: &str,
    picks: &[i64],
) -> Result<Vec<i64>, LificError> {
    let Some(issue_id) = issue_id else {
        return Ok(Vec::new());
    };
    let Some(project_id) = issue_project(conn, issue_id)? else {
        return Ok(Vec::new());
    };
    let reachable: Vec<Agent> = agents_for_project(conn, project_id)?;
    let can_reach = |id: i64| id != author_id && reachable.iter().any(|a| a.id == id);

    // 1. Explicit.
    let mut targets: Vec<(i64, &'static str)> = Vec::new();
    for &id in picks {
        if can_reach(id) && !targets.iter().any(|(t, _)| *t == id) {
            targets.push((id, reason::PICKED));
        }
    }
    for name in super::comments::extract_mention_usernames(content) {
        if let Some(agent) = reachable
            .iter()
            .find(|a| a.username.eq_ignore_ascii_case(&name))
            && can_reach(agent.id)
            && !targets.iter().any(|(t, _)| *t == agent.id)
        {
            targets.push((agent.id, reason::MENTION));
        }
    }

    // 2–3. Implicit, for people's comments only: agents talking to each
    // other must say who they mean.
    if targets.is_empty() && !is_bot(conn, author_id)? {
        let attached = attached_agents(conn, issue_id)?;
        // An issue assigned to agents talks to them alone.
        let assigned = attached.values().any(|whys| whys.contains(&"assigned"));
        for (id, whys) in attached {
            if assigned && !whys.contains(&"assigned") {
                continue;
            }
            if can_reach(id) {
                let why = if whys == ["routed"] {
                    reason::ISSUE_ROUTE
                } else {
                    reason::ATTACHED
                };
                targets.push((id, why));
            }
        }
        if targets.is_empty() {
            let mut stmt = conn.prepare_cached(
                "SELECT agent_id FROM fork_agent_routes WHERE project_id = ?1 AND issue_id IS NULL",
            )?;
            let ids: Vec<i64> = stmt
                .query_map(params![project_id], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for id in ids {
                if can_reach(id) {
                    targets.push((id, reason::PROJECT_ROUTE));
                }
            }
        }
    }

    for (id, why) in &targets {
        conn.execute(
            "INSERT OR IGNORE INTO fork_agent_inbox (agent_id, comment_id, reason) VALUES (?1, ?2, ?3)",
            params![id, comment_id, why],
        )?;
    }
    Ok(targets.into_iter().map(|(id, _)| id).collect())
}

/// After an edit: agents newly @mentioned get the comment too. Nobody is
/// taken back out; what was delivered stays delivered.
pub fn route_edit(
    conn: &Connection,
    comment_id: i64,
    issue_id: Option<i64>,
    author_id: i64,
    content: &str,
) -> Result<(), LificError> {
    let Some(issue_id) = issue_id else { return Ok(()) };
    let Some(project_id) = issue_project(conn, issue_id)? else {
        return Ok(());
    };
    let reachable = agents_for_project(conn, project_id)?;
    for name in super::comments::extract_mention_usernames(content) {
        if let Some(agent) = reachable
            .iter()
            .find(|a| a.username.eq_ignore_ascii_case(&name))
            && agent.id != author_id
        {
            conn.execute(
                "INSERT OR IGNORE INTO fork_agent_inbox (agent_id, comment_id, reason) VALUES (?1, ?2, ?3)",
                params![agent.id, comment_id, reason::MENTION],
            )?;
        }
    }
    Ok(())
}

/// Who each comment of an issue went to, for the comment list.
pub fn deliveries_for_issue(
    conn: &Connection,
    issue_id: i64,
) -> Result<BTreeMap<i64, Vec<Delivery>>, LificError> {
    let mut stmt = conn.prepare(
        "SELECT f.comment_id, f.agent_id, f.reason, f.created_at, f.delivered_at
           FROM fork_agent_inbox f JOIN comments c ON c.id = f.comment_id
          WHERE c.issue_id = ?1
          ORDER BY f.comment_id, f.id",
    )?;
    let rows: Vec<(i64, i64, String, String, Option<String>)> = stmt
        .query_map(params![issue_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut out: BTreeMap<i64, Vec<Delivery>> = BTreeMap::new();
    for (comment_id, agent_id, reason, created_at, delivered_at) in rows {
        if let Some(agent) = get_agent(conn, agent_id)? {
            out.entry(comment_id).or_default().push(Delivery {
                agent,
                reason,
                created_at,
                delivered_at,
            });
        }
    }
    Ok(out)
}

/// Tell an agent it was assigned to an issue. Nothing when it assigned
/// itself or is not an agent.
pub fn notify_assigned(
    conn: &Connection,
    agent_id: i64,
    issue_id: i64,
    actor_id: Option<i64>,
) -> Result<(), LificError> {
    if actor_id == Some(agent_id) || !is_bot(conn, agent_id)? {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO fork_agent_inbox (agent_id, issue_id, actor_id, reason) VALUES (?1, ?2, ?3, ?4)",
        params![agent_id, issue_id, actor_id, reason::ASSIGNED],
    )?;
    Ok(())
}

/// The agents whose inbox `agent_id` reads: its own, and for a workspace
/// agent its parent's (messages addressed to the tool as a whole, which
/// the first of its sessions to look takes).
fn inbox_owners(conn: &Connection, agent_id: i64) -> Result<(i64, i64), LificError> {
    Ok((
        agent_id,
        super::agent_workspaces::parent_of(conn, agent_id)?.unwrap_or(agent_id),
    ))
}

/// A message waiting for an agent: a comment, or (`comment_id` `None`) a
/// notice such as being assigned to the issue.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InboxItem {
    pub id: i64,
    pub reason: String,
    pub created_at: String,
    pub comment_id: Option<i64>,
    /// The comment's text; empty for a notice.
    pub content: String,
    pub author_id: Option<i64>,
    pub author_name: String,
    pub author_is_agent: bool,
    /// The author owns this agent: their word is an instruction, anyone
    /// else's a request.
    pub author_is_owner: bool,
    pub issue_id: i64,
    pub identifier: String,
    pub title: String,
    pub status: String,
    pub project_identifier: String,
}

/// Pending messages for `agent_id`, oldest first. Unless `peek`, they are
/// marked delivered, so the next call returns only what arrived since.
/// Messages whose comment was deleted are dropped on the way.
pub fn take(
    conn: &Connection,
    agent_id: i64,
    limit: i64,
    peek: bool,
) -> Result<Vec<InboxItem>, LificError> {
    let limit = limit.clamp(1, 50);
    let (own, parent) = inbox_owners(conn, agent_id)?;
    let mut stmt = conn.prepare(
        "SELECT f.id, f.reason, f.created_at, f.comment_id, COALESCE(c.content, ''),
                COALESCE(c.user_id, f.actor_id),
                COALESCE(NULLIF(u.display_name, ''), u.username, 'someone'), COALESCE(u.is_bot, 0),
                COALESCE(u.id = (SELECT owner_id FROM users WHERE id = ?1), 0),
                i.id, p.identifier || '-' || i.sequence, i.title, i.status, p.identifier,
                (f.comment_id IS NOT NULL AND (c.id IS NULL OR c.deleted_at IS NOT NULL))
                    OR i.deleted_at IS NOT NULL
           FROM fork_agent_inbox f
           LEFT JOIN comments c ON c.id = f.comment_id
           JOIN issues i ON i.id = COALESCE(c.issue_id, f.issue_id)
           LEFT JOIN users u ON u.id = COALESCE(c.user_id, f.actor_id)
           JOIN projects p ON p.id = i.project_id
          WHERE f.agent_id IN (?1, ?3) AND f.delivered_at IS NULL
          ORDER BY f.id
          LIMIT ?2",
    )?;
    let rows: Vec<(InboxItem, bool)> = stmt
        .query_map(params![own, limit, parent], |r| {
            Ok((
                InboxItem {
                    id: r.get(0)?,
                    reason: r.get(1)?,
                    created_at: r.get(2)?,
                    comment_id: r.get(3)?,
                    content: r.get(4)?,
                    author_id: r.get(5)?,
                    author_name: r.get(6)?,
                    author_is_agent: r.get(7)?,
                    author_is_owner: r.get::<_, Option<bool>>(8)?.unwrap_or(false),
                    issue_id: r.get(9)?,
                    identifier: r.get(10)?,
                    title: r.get(11)?,
                    status: r.get(12)?,
                    project_identifier: r.get(13)?,
                },
                r.get(14)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut items = Vec::new();
    for (item, gone) in rows {
        if gone || !peek {
            conn.execute(
                "UPDATE fork_agent_inbox SET delivered_at = datetime('now') WHERE id = ?1",
                params![item.id],
            )?;
        }
        if !gone {
            items.push(item);
        }
    }
    Ok(items)
}

/// Messages still waiting for `agent_id` (with its parent's, see [`take`]).
pub fn pending_count(conn: &Connection, agent_id: i64) -> Result<i64, LificError> {
    let (own, parent) = inbox_owners(conn, agent_id)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM fork_agent_inbox WHERE agent_id IN (?1, ?2) AND delivered_at IS NULL",
        params![own, parent],
        |r| r.get(0),
    )?)
}

// ── Routes ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Route {
    pub id: i64,
    pub agent: Agent,
    pub project_id: i64,
    pub issue_id: Option<i64>,
}

/// Routes in a project: the project-wide ones, and with `issue_id` that
/// issue's as well.
pub fn routes(
    conn: &Connection,
    project_id: i64,
    issue_id: Option<i64>,
) -> Result<Vec<Route>, LificError> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, project_id, issue_id FROM fork_agent_routes
          WHERE project_id = ?1 AND (issue_id IS NULL OR issue_id = ?2)
          ORDER BY issue_id IS NOT NULL, id",
    )?;
    let rows: Vec<(i64, i64, i64, Option<i64>)> = stmt
        .query_map(params![project_id, issue_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (id, agent_id, project_id, issue_id) in rows {
        if let Some(agent) = get_agent(conn, agent_id)? {
            out.push(Route {
                id,
                agent,
                project_id,
                issue_id,
            });
        }
    }
    Ok(out)
}

/// Turn a route on or off. The caller has checked that `actor` may steer
/// this agent (its owner, or an admin) and can see the project.
pub fn set_route(
    conn: &Connection,
    agent_id: i64,
    project_id: i64,
    issue_id: Option<i64>,
    on: bool,
    actor: i64,
) -> Result<(), LificError> {
    if let Some(issue_id) = issue_id
        && issue_project(conn, issue_id)? != Some(project_id)
    {
        return Err(LificError::BadRequest(
            "the issue is not in that project".into(),
        ));
    }
    if on {
        conn.execute(
            "INSERT OR IGNORE INTO fork_agent_routes (agent_id, project_id, issue_id, created_by)
             VALUES (?1, ?2, ?3, ?4)",
            params![agent_id, project_id, issue_id, actor],
        )?;
    } else {
        conn.execute(
            "DELETE FROM fork_agent_routes
              WHERE agent_id = ?1 AND project_id = ?2 AND issue_id IS ?3",
            params![agent_id, project_id, issue_id],
        )?;
    }
    Ok(())
}

/// Every agent attached to an issue, for the issue's "Agents" panel.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AttachedAgent {
    pub agent: Agent,
    pub reasons: Vec<&'static str>,
}

pub fn attached_list(conn: &Connection, issue_id: i64) -> Result<Vec<AttachedAgent>, LificError> {
    let mut out = Vec::new();
    let reachable: HashSet<i64> = match issue_project(conn, issue_id)? {
        Some(pid) => agents_for_project(conn, pid)?.into_iter().map(|a| a.id).collect(),
        None => HashSet::new(),
    };
    for (id, reasons) in attached_agents(conn, issue_id)? {
        if !reachable.contains(&id) {
            continue;
        }
        if let Some(agent) = get_agent(conn, id)? {
            out.push(AttachedAgent { agent, reasons });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
