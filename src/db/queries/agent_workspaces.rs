//! Fork: one agent per place it runs.
//!
//! A tool connects as one bot per owner (`claude-code-anton`), so every
//! session of that tool, on every machine and in every repository, used to
//! be the same agent with one inbox: a comment meant for the session in
//! `forge` could be read by the one in `prism`. A connection can now say
//! where it runs with the `X-Lific-Workspace` header (`forge@chamber`); the
//! request then acts as a *workspace agent* of that bot, created on first
//! use: its own account (`Claude Code · forge@chamber`), history and inbox,
//! the same owner and so the same access. It has no credential of its own:
//! it exists only through its parent's, so disconnecting the parent stops
//! every workspace agent too.
//!
//! The table is created with the inbox's (see `agent_inbox::ensure_schema`).

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::models::AuthUser;
use crate::error::LificError;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS fork_agent_workspaces (
    agent_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    parent_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace    TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (parent_id, workspace)
);
";

/// The request header a connection names its workspace with.
pub const HEADER: &str = "x-lific-workspace";
/// Longest workspace label kept.
const MAX_LABEL: usize = 64;
/// Workspace agents one bot may have; past it, requests act as the bot.
const MAX_PER_PARENT: i64 = 100;

/// A workspace label as stored: lowercase `[a-z0-9._@-]`, anything else
/// turned into `-`, at most 64 characters. `None` when nothing is left.
pub fn normalize(raw: &str) -> Option<String> {
    let mut out = String::new();
    for c in raw.trim().chars().flat_map(char::to_lowercase) {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-') {
            c
        } else {
            '-'
        };
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
        if out.len() >= MAX_LABEL {
            break;
        }
    }
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    (!out.is_empty()).then_some(out)
}

fn auth_user(conn: &Connection, id: i64) -> Result<Option<AuthUser>, LificError> {
    Ok(conn
        .query_row(
            "SELECT id, username, display_name, is_admin FROM users
              WHERE id = ?1 AND is_active = 1",
            params![id],
            |r| {
                Ok(AuthUser {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    display_name: r.get(2)?,
                    is_admin: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// The workspace agent `parent` acts as in `workspace`, if it exists. Read
/// only: the hot path for every request that sends the header. The second
/// value says `last_seen_at` is old enough to refresh.
pub fn find(
    conn: &Connection,
    parent_id: i64,
    workspace: &str,
) -> Result<Option<(AuthUser, bool)>, LificError> {
    let row: Option<(i64, bool)> = conn
        .query_row(
            "SELECT agent_id, last_seen_at < datetime('now', '-5 minutes')
               FROM fork_agent_workspaces WHERE parent_id = ?1 AND workspace = ?2",
            params![parent_id, workspace],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((id, stale)) = row else { return Ok(None) };
    Ok(auth_user(conn, id)?.map(|u| (u, stale)))
}

pub fn touch(conn: &Connection, agent_id: i64) -> Result<(), LificError> {
    conn.execute(
        "UPDATE fork_agent_workspaces SET last_seen_at = datetime('now') WHERE agent_id = ?1",
        params![agent_id],
    )?;
    Ok(())
}

/// Find or create the workspace agent for `parent` in `workspace` (already
/// normalized). `None` when `parent` cannot have one: not a bot, no owner,
/// itself a workspace agent, or at its limit.
pub fn ensure(
    conn: &Connection,
    parent_id: i64,
    workspace: &str,
) -> Result<Option<AuthUser>, LificError> {
    if let Some((user, _)) = find(conn, parent_id, workspace)? {
        return Ok(Some(user));
    }
    let parent: Option<(String, String, Option<i64>, Option<String>, bool)> = conn
        .query_row(
            "SELECT username, COALESCE(NULLIF(display_name, ''), username), owner_id, tool_id,
                    EXISTS (SELECT 1 FROM fork_agent_workspaces WHERE agent_id = users.id)
               FROM users WHERE id = ?1 AND is_bot = 1 AND is_active = 1",
            params![parent_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((username, display, Some(owner_id), tool_id, false)) = parent else {
        return Ok(None);
    };
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM fork_agent_workspaces WHERE parent_id = ?1",
        params![parent_id],
        |r| r.get(0),
    )?;
    if count >= MAX_PER_PARENT {
        tracing::warn!(parent_id, "workspace agent limit reached; acting as the parent");
        return Ok(None);
    }
    let tool = format!("{}@{workspace}", tool_id.unwrap_or_else(|| username.clone()));
    // Usernames take letters, digits, `_` and `-` (what an @mention parses).
    let slug: String = workspace
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base: String = format!("{username}-{slug}").chars().take(60).collect();
    let display_name = format!("{display} · {workspace}");
    let mut bot = None;
    for n in 1..=9 {
        let candidate = if n == 1 { base.clone() } else { format!("{base}-{n}") };
        let taken: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM users WHERE username = ?1 COLLATE NOCASE)",
            params![candidate],
            |r| r.get(0),
        )?;
        if !taken {
            bot = Some(super::users::create_bot_user(
                conn,
                owner_id,
                &candidate,
                &display_name,
                Some(&tool),
            )?);
            break;
        }
    }
    let Some(bot) = bot else { return Ok(None) };
    conn.execute(
        "INSERT INTO fork_agent_workspaces (agent_id, parent_id, workspace) VALUES (?1, ?2, ?3)",
        params![bot.id, parent_id, workspace],
    )?;
    tracing::info!(parent_id, agent_id = bot.id, workspace, "workspace agent created");
    auth_user(conn, bot.id)
}

/// The bot a workspace agent belongs to, or `None` for any other account.
pub fn parent_of(conn: &Connection, agent_id: i64) -> Result<Option<i64>, LificError> {
    Ok(conn
        .query_row(
            "SELECT parent_id FROM fork_agent_workspaces WHERE agent_id = ?1",
            params![agent_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// The workspace agents of a bot.
pub fn children(conn: &Connection, parent_id: i64) -> Result<Vec<i64>, LificError> {
    let mut stmt =
        conn.prepare_cached("SELECT agent_id FROM fork_agent_workspaces WHERE parent_id = ?1")?;
    let ids = stmt
        .query_map(params![parent_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Workspace agent → its bot, for agents seen by this process. Lets code
/// with no database at hand (MCP's `for:` marker) treat an issue assigned
/// to the bot as the workspace agent's own.
static PARENTS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<i64, i64>>> =
    std::sync::LazyLock::new(Default::default);

pub fn cached_parent(agent_id: i64) -> Option<i64> {
    PARENTS.lock().ok()?.get(&agent_id).copied()
}

fn remember(agent_id: i64, parent_id: i64) {
    if let Ok(mut map) = PARENTS.lock() {
        map.insert(agent_id, parent_id);
    }
}

/// Swap a request's credential user for its workspace agent when the
/// connection names a workspace. Anything that goes wrong keeps the
/// credential user: the header is a routing hint, never a reason to fail a
/// request.
pub fn apply(db: &crate::db::DbPool, user: AuthUser, raw: Option<&str>) -> AuthUser {
    let Some(workspace) = raw.and_then(normalize) else {
        return user;
    };
    let found = db.read().and_then(|conn| find(&conn, user.id, &workspace));
    let result = match found {
        Ok(Some((agent, false))) => {
            remember(agent.id, user.id);
            return agent;
        }
        Ok(Some((agent, true))) => db.transaction(|tx| touch(tx, agent.id)).map(|()| Some(agent)),
        Ok(None) => db.transaction(|tx| ensure(tx, user.id, &workspace)),
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(agent)) => {
            remember(agent.id, user.id);
            agent
        }
        Ok(None) => user,
        Err(e) => {
            tracing::warn!(error = %e, "workspace agent lookup failed; acting as the credential's user");
            user
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_labels() {
        assert_eq!(normalize(" Forge@Chamber ").as_deref(), Some("forge@chamber"));
        assert_eq!(normalize("my repo/x").as_deref(), Some("my-repo-x"));
        assert_eq!(normalize("  --  ").as_deref(), None);
        assert_eq!(normalize(&"a".repeat(200)).unwrap().len(), 64);
    }

    #[test]
    fn creates_one_agent_per_workspace_under_the_same_owner() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.write().unwrap();
        let owner = super::super::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "anton".into(),
                email: "anton@x.test".into(),
                password: "password123456".into(),
                display_name: None,
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        let bot = super::super::users::ensure_bot(&conn, owner.id, "claude-code", "Claude Code")
            .unwrap();
        let a = ensure(&conn, bot.id, "forge@chamber").unwrap().unwrap();
        assert_eq!(a.username, "claude-code-anton-forge-chamber");
        assert_eq!(a.display_name, "Claude Code · forge@chamber");
        assert_eq!(ensure(&conn, bot.id, "forge@chamber").unwrap().unwrap().id, a.id);
        let b = ensure(&conn, bot.id, "prism@chamber").unwrap().unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(parent_of(&conn, a.id).unwrap(), Some(bot.id));
        // Same owner, so the same access.
        let owner_of: i64 = conn
            .query_row("SELECT owner_id FROM users WHERE id = ?1", [a.id], |r| r.get(0))
            .unwrap();
        assert_eq!(owner_of, owner.id);
        // No nesting, and people have no workspace agents.
        assert!(ensure(&conn, a.id, "x").unwrap().is_none());
        assert!(ensure(&conn, owner.id, "x").unwrap().is_none());
        // The parent's (owner, tool) identity still resolves to the parent.
        let again = super::super::users::ensure_bot(&conn, owner.id, "claude-code", "Claude Code")
            .unwrap();
        assert_eq!(again.id, bot.id);
    }
}
