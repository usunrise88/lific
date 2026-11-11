//! Instance settings that exist only in this fork, kept as key/value rows in
//! a table of their own (`fork_settings`) instead of new columns on
//! `instance_settings`. A numbered migration would collide with upstream's
//! next one; this table is created on first write and is invisible to the
//! migration runner, and a read before that falls back to the default.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LificError;

const CREATE: &str = "CREATE TABLE IF NOT EXISTS fork_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
)";

/// Where the brand logo in the sidebar leads.
pub const LOGO_TARGET: &str = "logo_target";
/// `repo` (upstream behavior: the project on GitHub) or an in-app page.
pub const LOGO_TARGETS: &[&str] = &["repo", "home", "feed", "for-me"];
const LOGO_DEFAULT: &str = "repo";

fn read(conn: &Connection, key: &str) -> Result<Option<String>, LificError> {
    let result = conn
        .query_row(
            "SELECT value FROM fork_settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional();
    match result {
        Ok(v) => Ok(v),
        // Never written yet: the table does not exist.
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => {
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

fn write(conn: &Connection, key: &str, value: &str) -> Result<(), LificError> {
    conn.execute(CREATE, [])?;
    conn.execute(
        "INSERT INTO fork_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        params![key, value],
    )?;
    Ok(())
}

pub fn logo_target(conn: &Connection) -> Result<String, LificError> {
    Ok(read(conn, LOGO_TARGET)?
        .filter(|v| LOGO_TARGETS.contains(&v.as_str()))
        .unwrap_or_else(|| LOGO_DEFAULT.to_string()))
}

pub fn set_logo_target(conn: &Connection, value: &str) -> Result<(), LificError> {
    if !LOGO_TARGETS.contains(&value) {
        return Err(LificError::BadRequest(format!(
            "logo_target must be one of: {}",
            LOGO_TARGETS.join(", ")
        )));
    }
    write(conn, LOGO_TARGET, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_until_written_and_validates() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.write().unwrap();
        assert_eq!(logo_target(&conn).unwrap(), "repo");
        set_logo_target(&conn, "feed").unwrap();
        assert_eq!(logo_target(&conn).unwrap(), "feed");
        set_logo_target(&conn, "for-me").unwrap();
        assert_eq!(logo_target(&conn).unwrap(), "for-me");
        assert!(set_logo_target(&conn, "elsewhere").is_err());
        assert_eq!(logo_target(&conn).unwrap(), "for-me");
    }
}
