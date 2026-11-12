//! The "For me" page (fork): open issues a person has to act on, across every
//! project the caller can see. An issue qualifies when it is marked for a
//! person (any person, or named people; see `assignees`) or when a user wait
//! points at the caller.
//!
//! Candidates are few (open, human-assigned issues), so the query reads all
//! of them with the flags the filters need, counts them for the filter rail,
//! then filters, sorts and pages in SQL order before hydrating one page of
//! full issues.

use rusqlite::{Connection, params_from_iter};
use std::collections::HashSet;

use crate::db::models::Issue;
use crate::error::LificError;

const DEFAULT_LIMIT: i64 = 60;
const MAX_LIMIT: i64 = 200;
const COMMENT_PREVIEW_CHARS: usize = 600;

pub const STATUSES: &[&str] = &["backlog", "todo", "active", "done", "cancelled"];
const OPEN_STATUSES: &[&str] = &["backlog", "todo", "active"];
pub const PRIORITIES: &[&str] = &["urgent", "high", "medium", "low", "none"];

/// Whose issues: named for the caller (or waiting on them), marked for any
/// person, or named for someone else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Me,
    Anyone,
    Others,
}

impl Scope {
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "me" => Some(Self::Me),
            "anyone" => Some(Self::Anyone),
            "others" => Some(Self::Others),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Most recently assigned first.
    #[default]
    Newest,
    Oldest,
    Updated,
    Priority,
}

impl Sort {
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "newest" => Some(Self::Newest),
            "oldest" => Some(Self::Oldest),
            "updated" => Some(Self::Updated),
            "priority" => Some(Self::Priority),
            _ => None,
        }
    }

    fn order_by(self) -> &'static str {
        match self {
            Sort::Newest => "assigned_at DESC, i.id DESC",
            Sort::Oldest => "assigned_at ASC, i.id ASC",
            Sort::Updated => "i.updated_at DESC, i.id DESC",
            Sort::Priority => {
                "CASE i.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2
                                 WHEN 'low' THEN 3 ELSE 4 END, assigned_at DESC, i.id DESC"
            }
        }
    }
}

/// Who handed the issue to people: an agent, a person, or either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestedBy {
    #[default]
    All,
    Agents,
    People,
}

#[derive(Debug, Default)]
pub struct ForMeFilter<'a> {
    pub visible: Option<&'a HashSet<i64>>,
    /// The caller's user id; `None` for an unbound operator key.
    pub me: Option<i64>,
    pub projects: Option<Vec<i64>>,
    /// `None` = open statuses.
    pub statuses: Option<Vec<String>>,
    pub priorities: Option<Vec<String>>,
    /// `None` = every scope.
    pub scopes: Option<Vec<Scope>>,
    pub requested_by: RequestedBy,
    pub sort: Sort,
    pub mine_first: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Requester {
    pub user_id: Option<i64>,
    pub name: String,
    pub username: Option<String>,
    pub is_bot: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommentPreview {
    pub author: String,
    pub is_bot: bool,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ForMeItem {
    #[serde(flatten)]
    pub issue: Issue,
    pub project_identifier: String,
    pub project_name: String,
    /// When the issue was last handed to people (the newest assignment row),
    /// or created, for an issue that is only waiting on the caller.
    pub assigned_at: String,
    pub assigned_by: Option<Requester>,
    /// Named for the caller.
    pub mine: bool,
    /// A user wait points at the caller.
    pub waiting_on_me: bool,
    pub comment_count: i64,
    pub last_comment: Option<CommentPreview>,
}

/// Matches per scope under the other filters, for the filter rail.
#[derive(Debug, Default, serde::Serialize)]
pub struct ForMeCounts {
    pub me: i64,
    pub anyone: i64,
    pub others: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct ForMePage {
    pub items: Vec<ForMeItem>,
    pub total: i64,
    pub has_more: bool,
    pub counts: ForMeCounts,
}

struct Candidate {
    id: i64,
    mine: bool,
    waiting: bool,
    anyone: bool,
    others: bool,
    assigned_at: String,
    assigner: Option<i64>,
    assigner_is_bot: bool,
}

pub fn list_for_me(conn: &Connection, f: &ForMeFilter) -> Result<ForMePage, LificError> {
    let empty = || ForMePage {
        items: Vec::new(),
        total: 0,
        has_more: false,
        counts: ForMeCounts::default(),
    };
    let me = f.me.unwrap_or(-1);
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(me)];
    let mut clauses = vec!["i.deleted_at IS NULL".to_string()];

    let mut in_list = |values: Vec<Box<dyn rusqlite::types::ToSql>>| -> String {
        let start = params.len();
        let n = values.len();
        params.extend(values);
        (1..=n).map(|i| format!("?{}", start + i)).collect::<Vec<_>>().join(", ")
    };

    if let Some(visible) = f.visible {
        if visible.is_empty() {
            return Ok(empty());
        }
        let marks = in_list(visible.iter().map(|&id| boxed(id)).collect());
        clauses.push(format!("i.project_id IN ({marks})"));
    }
    if let Some(projects) = &f.projects {
        if projects.is_empty() {
            return Ok(empty());
        }
        let marks = in_list(projects.iter().map(|&id| boxed(id)).collect());
        clauses.push(format!("i.project_id IN ({marks})"));
    }
    let statuses: Vec<String> = f
        .statuses
        .clone()
        .unwrap_or_else(|| OPEN_STATUSES.iter().map(|s| s.to_string()).collect());
    if statuses.is_empty() {
        return Ok(empty());
    }
    let marks = in_list(statuses.into_iter().map(boxed).collect());
    clauses.push(format!("i.status IN ({marks})"));
    if let Some(priorities) = &f.priorities {
        if priorities.is_empty() {
            return Ok(empty());
        }
        let marks = in_list(priorities.iter().map(|p| boxed(p.clone())).collect());
        clauses.push(format!("i.priority IN ({marks})"));
    }

    let sql = format!(
        "SELECT i.id,
                EXISTS (SELECT 1 FROM issue_assignees a WHERE a.issue_id = i.id AND a.user_id = ?1),
                EXISTS (SELECT 1 FROM issue_waits w
                         WHERE w.issue_id = i.id AND w.kind = 'user' AND w.user_id = ?1),
                EXISTS (SELECT 1 FROM issue_assignees a WHERE a.issue_id = i.id AND a.user_id IS NULL),
                EXISTS (SELECT 1 FROM issue_assignees a
                         WHERE a.issue_id = i.id AND a.user_id IS NOT NULL AND a.user_id != ?1
                           AND {person}),
                COALESCE((SELECT MAX(a.created_at) FROM issue_assignees a WHERE a.issue_id = i.id),
                         i.created_at) AS assigned_at,
                (SELECT a.created_by FROM issue_assignees a WHERE a.issue_id = i.id
                  ORDER BY a.created_at DESC, a.id DESC LIMIT 1),
                COALESCE((SELECT u.is_bot FROM issue_assignees a JOIN users u ON u.id = a.created_by
                           WHERE a.issue_id = i.id ORDER BY a.created_at DESC, a.id DESC LIMIT 1), 0)
         FROM issues i
         WHERE {}
           AND (EXISTS (SELECT 1 FROM issue_assignees a WHERE a.issue_id = i.id AND {person})
                OR EXISTS (SELECT 1 FROM issue_waits w
                            WHERE w.issue_id = i.id AND w.kind = 'user' AND w.user_id = ?1))
         ORDER BY {}{}",
        clauses.join(" AND "),
        if f.mine_first {
            "(EXISTS (SELECT 1 FROM issue_assignees a WHERE a.issue_id = i.id AND a.user_id = ?1)
              OR EXISTS (SELECT 1 FROM issue_waits w
                          WHERE w.issue_id = i.id AND w.kind = 'user' AND w.user_id = ?1)) DESC, "
        } else {
            ""
        },
        f.sort.order_by(),
        person = super::assignees::PERSON_ROW,
    );
    let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let candidates = stmt
        .query_map(params_from_iter(refs), |row| {
            Ok(Candidate {
                id: row.get(0)?,
                mine: row.get(1)?,
                waiting: row.get(2)?,
                anyone: row.get(3)?,
                others: row.get(4)?,
                assigned_at: row.get(5)?,
                assigner: row.get(6)?,
                assigner_is_bot: row.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let requested = |c: &Candidate| match f.requested_by {
        RequestedBy::All => true,
        RequestedBy::Agents => c.assigner_is_bot,
        RequestedBy::People => !c.assigner_is_bot,
    };
    let in_scope = |c: &Candidate, s: Scope| match s {
        Scope::Me => c.mine || c.waiting,
        Scope::Anyone => c.anyone,
        Scope::Others => c.others,
    };

    let mut counts = ForMeCounts::default();
    for c in candidates.iter().filter(|c| requested(c)) {
        counts.me += in_scope(c, Scope::Me) as i64;
        counts.anyone += in_scope(c, Scope::Anyone) as i64;
        counts.others += in_scope(c, Scope::Others) as i64;
    }

    let matching: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| requested(c))
        .filter(|c| match &f.scopes {
            None => true,
            Some(scopes) => scopes.iter().any(|&s| in_scope(c, s)),
        })
        .collect();

    let (limit, offset) = super::page_with(f.limit, f.offset, DEFAULT_LIMIT, MAX_LIMIT);
    let start = offset.max(0) as usize;
    let page: Vec<&Candidate> = matching.iter().skip(start).take(limit as usize).copied().collect();
    let total = matching.len() as i64;
    let has_more = (start + page.len()) < matching.len();

    let mut items = Vec::with_capacity(page.len());
    for c in page {
        items.push(hydrate(conn, c)?);
    }
    Ok(ForMePage {
        items,
        total,
        has_more,
        counts,
    })
}

fn hydrate(conn: &Connection, c: &Candidate) -> Result<ForMeItem, LificError> {
    let issue = super::get_issue(conn, c.id)?;
    let (project_identifier, project_name): (String, String) = conn.query_row(
        "SELECT identifier, name FROM projects WHERE id = ?1",
        [issue.project_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let assigned_by = match c.assigner {
        None => None,
        Some(id) => conn
            .query_row(
                "SELECT username, COALESCE(NULLIF(display_name, ''), username), is_bot
                   FROM users WHERE id = ?1",
                [id],
                |r| {
                    Ok(Requester {
                        user_id: Some(id),
                        username: Some(r.get(0)?),
                        name: r.get(1)?,
                        is_bot: r.get::<_, i64>(2)? != 0,
                    })
                },
            )
            .ok(),
    };
    let comment_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM comments WHERE issue_id = ?1 AND deleted_at IS NULL",
        [c.id],
        |r| r.get(0),
    )?;
    let last_comment = conn
        .query_row(
            "SELECT COALESCE(c.imported_author, NULLIF(u.display_name, ''), u.username, 'System'),
                    COALESCE(u.is_bot, 0), c.content, c.created_at
               FROM comments c LEFT JOIN users u ON u.id = c.user_id
              WHERE c.issue_id = ?1 AND c.deleted_at IS NULL
              ORDER BY c.created_at DESC, c.id DESC LIMIT 1",
            [c.id],
            |r| {
                let content: String = r.get(2)?;
                Ok(CommentPreview {
                    author: r.get(0)?,
                    is_bot: r.get::<_, i64>(1)? != 0,
                    content: content.chars().take(COMMENT_PREVIEW_CHARS).collect(),
                    created_at: r.get(3)?,
                })
            },
        )
        .ok();
    Ok(ForMeItem {
        issue,
        project_identifier,
        project_name,
        assigned_at: c.assigned_at.clone(),
        assigned_by,
        mine: c.mine,
        waiting_on_me: c.waiting,
        comment_count,
        last_comment,
    })
}

fn boxed<T: rusqlite::types::ToSql + 'static>(value: T) -> Box<dyn rusqlite::types::ToSql> {
    Box::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::*;
    use crate::db::queries;

    fn setup() -> (crate::db::DbPool, i64, i64, Vec<i64>) {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.write().unwrap();
        let pid = queries::create_project(
            &conn,
            &CreateProject {
                name: "Me".into(),
                identifier: "ME".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        let user = |name: &str, bot: bool| {
            conn.execute(
                "INSERT INTO users (username, email, password_hash, display_name, is_admin, is_bot)
                 VALUES (?1, ?1 || '@t', 'x', ?1, 0, ?2)",
                rusqlite::params![name, bot],
            )
            .unwrap();
            conn.last_insert_rowid()
        };
        let me = user("ada", false);
        let other = user("bob", false);
        let agent = user("claude", true);
        let mut ids = Vec::new();
        for (n, title) in ["mine", "anyone", "others", "agents only", "done mine"].iter().enumerate() {
            let issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: pid,
                    title: (*title).into(),
                    ..Default::default()
                },
            )
            .unwrap();
            ids.push(issue.id);
            let ts = format!("2026-11-0{} 10:00:00", n + 1);
            let assign = |user_id: Option<i64>| {
                conn.execute(
                    "INSERT INTO issue_assignees (issue_id, user_id, created_by, created_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![issue.id, user_id, agent, ts],
                )
                .unwrap();
            };
            match n {
                0 | 4 => assign(Some(me)),
                1 => assign(None),
                2 => assign(Some(other)),
                _ => {}
            }
        }
        conn.execute("UPDATE issues SET status = 'done' WHERE id = ?1", [ids[4]]).unwrap();
        drop(conn);
        (pool, me, agent, ids)
    }

    fn titles(p: &ForMePage) -> Vec<String> {
        p.items.iter().map(|i| i.issue.title.clone()).collect()
    }

    #[test]
    fn lists_open_human_work_with_scopes_counts_and_sorts() {
        let (pool, me, agent, _) = setup();
        let conn = pool.read().unwrap();
        let base = ForMeFilter {
            me: Some(me),
            ..Default::default()
        };

        let all = list_for_me(&conn, &base).unwrap();
        assert_eq!(titles(&all), vec!["others", "anyone", "mine"], "newest assigned first, open only");
        assert_eq!((all.counts.me, all.counts.anyone, all.counts.others), (1, 1, 1));
        let mine = &all.items[2];
        assert!(mine.mine);
        assert_eq!(mine.assigned_by.as_ref().unwrap().user_id, Some(agent));
        assert!(mine.assigned_by.as_ref().unwrap().is_bot);

        let oldest = list_for_me(&conn, &ForMeFilter { sort: Sort::Oldest, ..base_clone(&base) }).unwrap();
        assert_eq!(titles(&oldest), vec!["mine", "anyone", "others"]);

        let mine_first = list_for_me(&conn, &ForMeFilter { mine_first: true, ..base_clone(&base) }).unwrap();
        assert_eq!(titles(&mine_first)[0], "mine");

        let only_me = list_for_me(&conn, &ForMeFilter { scopes: Some(vec![Scope::Me]), ..base_clone(&base) }).unwrap();
        assert_eq!(titles(&only_me), vec!["mine"]);
        assert_eq!(only_me.counts.anyone, 1, "counts ignore the scope filter");

        let by_people = list_for_me(&conn, &ForMeFilter { requested_by: RequestedBy::People, ..base_clone(&base) }).unwrap();
        assert!(by_people.items.is_empty(), "every assignment here came from the agent");

        let done = list_for_me(&conn, &ForMeFilter { statuses: Some(vec!["done".into()]), ..base_clone(&base) }).unwrap();
        assert_eq!(titles(&done), vec!["done mine"]);

        let hidden = HashSet::new();
        let none = list_for_me(&conn, &ForMeFilter { visible: Some(&hidden), ..base_clone(&base) }).unwrap();
        assert!(none.items.is_empty());
    }

    fn base_clone<'a>(f: &ForMeFilter<'a>) -> ForMeFilter<'a> {
        ForMeFilter {
            visible: f.visible,
            me: f.me,
            ..Default::default()
        }
    }
}
