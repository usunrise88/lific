//! Fork: the agent manifest. The working rules this instance wants every
//! agent to keep in its own instructions file (AGENTS.md, CLAUDE.md, ...).
//! An admin edits it; agents fetch it rendered for a project and paste it
//! between the same markers `lific agents-md` writes, so either one replaces
//! the other's block in place.
//!
//! Stored as one `fork_settings` row; absent means the built-in default.

use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::fork_settings;
use crate::cli::agents_md::{BEGIN_MARKER, END_MARKER};
use crate::error::LificError;

const KEY: &str = "agent_manifest";
/// What a manifest says when nobody has edited it.
pub const DEFAULT: &str = include_str!("agent_manifest/default.md");
/// Agents pay for every byte of this in every session.
pub const MAX_CHARS: usize = 20_000;
/// Stands in for a project identifier the caller did not give.
const PROJECT_PLACEHOLDER: &str = "APP";

#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    /// The template as stored, placeholders unexpanded.
    pub content: String,
    pub is_default: bool,
    /// `None` until an admin first saves one.
    pub updated_at: Option<String>,
    /// Short hash of `content`; changes whenever the text does.
    pub version: String,
}

pub fn version_of(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

pub fn get(conn: &Connection) -> Result<Manifest, LificError> {
    Ok(match fork_settings::read_entry(conn, KEY)? {
        Some((content, updated_at)) => Manifest {
            version: version_of(&content),
            content,
            is_default: false,
            updated_at: Some(updated_at),
        },
        None => Manifest {
            content: DEFAULT.trim().to_string(),
            is_default: true,
            updated_at: None,
            version: version_of(DEFAULT.trim()),
        },
    })
}

/// Save `content`. Saving the default text verbatim resets instead, so the
/// instance keeps following future changes to the built-in manifest.
pub fn set(conn: &Connection, content: &str) -> Result<Manifest, LificError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(LificError::BadRequest(
            "the manifest cannot be empty; reset it to go back to the default".into(),
        ));
    }
    if content.chars().count() > MAX_CHARS {
        return Err(LificError::BadRequest(format!(
            "the manifest is limited to {MAX_CHARS} characters"
        )));
    }
    if content.contains(BEGIN_MARKER) || content.contains(END_MARKER) {
        return Err(LificError::BadRequest(
            "leave out the lific:begin and lific:end markers; they are added around the manifest"
                .into(),
        ));
    }
    if content == DEFAULT.trim() {
        return reset(conn);
    }
    fork_settings::write_value(conn, KEY, content)?;
    get(conn)
}

pub fn reset(conn: &Connection) -> Result<Manifest, LificError> {
    fork_settings::remove(conn, KEY)?;
    get(conn)
}

/// When the manifest changed after `since` (an activity timestamp in UTC,
/// `YYYY-MM-DDTHH:MM:SSZ` or `YYYY-MM-DD HH:MM:SS`), the manifest; `None`
/// when it did not or was never edited.
pub fn changed_since(conn: &Connection, since: &str) -> Result<Option<Manifest>, LificError> {
    let m = get(conn)?;
    let Some(updated) = m.updated_at.as_deref() else {
        return Ok(None);
    };
    let norm = |s: &str| s.replace('T', " ").trim_end_matches('Z').to_string();
    Ok((norm(updated) > norm(since)).then_some(m))
}

/// The block an agent pastes into its instructions file: markers, a version
/// line, and the manifest with `{{project}}` filled in.
pub fn render(m: &Manifest, project: Option<&str>) -> String {
    let project = project
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(PROJECT_PLACEHOLDER);
    let body = m.content.trim().replace("{{project}}", project);
    format!(
        "{BEGIN_MARKER}\n<!-- lific manifest v={} — edited in Lific (instance settings); refresh with get_manifest -->\n{body}\n{END_MARKER}\n",
        m.version
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_until_saved_and_resets() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.write().unwrap();
        let m = get(&conn).unwrap();
        assert!(m.is_default);
        assert_eq!(m.version, version_of(DEFAULT.trim()));

        let saved = set(&conn, "  Always run the tests.  ").unwrap();
        assert!(!saved.is_default);
        assert_eq!(saved.content, "Always run the tests.");
        assert_ne!(saved.version, m.version);
        assert!(saved.updated_at.is_some());

        assert!(reset(&conn).unwrap().is_default);
        // Saving the default text verbatim is a reset, not a pinned copy.
        set(&conn, "x").unwrap();
        assert!(set(&conn, DEFAULT).unwrap().is_default);
    }

    #[test]
    fn rejects_empty_oversized_and_marker_text() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.write().unwrap();
        assert!(set(&conn, "   ").is_err());
        assert!(set(&conn, &"a".repeat(MAX_CHARS + 1)).is_err());
        assert!(set(&conn, &format!("x {END_MARKER}")).is_err());
        assert!(get(&conn).unwrap().is_default);
    }

    #[test]
    fn renders_between_the_agents_md_markers() {
        let m = Manifest {
            content: "Project {{project}}: use {{project}}-1.".into(),
            is_default: false,
            updated_at: None,
            version: "abcd1234".into(),
        };
        let out = render(&m, Some("LIF"));
        assert!(out.starts_with(BEGIN_MARKER));
        assert!(out.trim_end().ends_with(END_MARKER));
        assert!(out.contains("v=abcd1234"));
        assert!(out.contains("Project LIF: use LIF-1."));
        assert!(render(&m, None).contains("Project APP"));
    }

    #[test]
    fn changed_since_compares_both_timestamp_shapes() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.write().unwrap();
        assert!(changed_since(&conn, "2000-01-01T00:00:00Z").unwrap().is_none());
        set(&conn, "rules").unwrap();
        assert!(changed_since(&conn, "2000-01-01T00:00:00Z").unwrap().is_some());
        assert!(changed_since(&conn, "2000-01-01 00:00:00").unwrap().is_some());
        assert!(changed_since(&conn, "2999-01-01T00:00:00Z").unwrap().is_none());
    }
}
