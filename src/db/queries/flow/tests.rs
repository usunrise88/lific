use super::*;
use crate::db::models::{CreateIssue, CreateProject};
use crate::db::queries;

fn now() -> NaiveDateTime {
    chrono::Utc::now().naive_utc()
}

fn ts(at: NaiveDateTime) -> String {
    at.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn input(now: NaiveDateTime) -> FlowInput<'static> {
    FlowInput {
        visible: None,
        projects: None,
        days: 30,
        tz_minutes: 0,
        now,
    }
}

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

fn user(conn: &Connection, name: &str, bot: bool) -> i64 {
    conn.execute(
        "INSERT INTO users (username, email, password_hash, display_name, is_admin, is_bot, owner_id)
         VALUES (?1, ?1 || '@test.local', 'x', ?1, 0, ?2, NULL)",
        params![name, bot],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// An issue created at `created` whose status history is exactly `moves`
/// (`from`, `to`, actor, when).
fn issue(
    conn: &Connection,
    pid: i64,
    title: &str,
    created: NaiveDateTime,
    moves: &[(&str, &str, Option<i64>, NaiveDateTime)],
) -> i64 {
    let id = queries::create_issue(
        conn,
        &CreateIssue {
            project_id: pid,
            title: title.into(),
            ..Default::default()
        },
    )
    .unwrap()
    .id;
    let status = moves.last().map_or("backlog", |m| m.1);
    conn.execute(
        "UPDATE issues SET created_at = ?2, status = ?3 WHERE id = ?1",
        params![id, ts(created), status],
    )
    .unwrap();
    conn.execute("DELETE FROM status_transitions WHERE issue_id = ?1", params![id])
        .unwrap();
    for (from, to, actor, at) in moves {
        conn.execute(
            "INSERT INTO status_transitions (issue_id, from_status, to_status, actor_user_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, from, to, actor, ts(*at)],
        )
        .unwrap();
    }
    id
}

fn audit(conn: &Connection, entity: &str, id: i64, action: &str, field: Option<&str>, value: &str, actor: i64, at: NaiveDateTime) {
    let (old, new) = if action.starts_with("un") { (Some(value), None) } else { (None, Some(value)) };
    conn.execute(
        "INSERT INTO audit_log (ts, actor_user_id, transport, entity_type, entity_id, issue_id, action, field, old_value, new_value)
         VALUES (?1, ?2, 'mcp', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![ts(at), actor, entity, id, if entity == "comment" { Some(id) } else { None }, action, field, old, new],
    )
    .unwrap();
}

fn h(n: i64) -> Duration {
    Duration::hours(n)
}

#[test]
fn closes_count_per_day_and_actor_with_evidence_and_rework() {
    let pool = crate::db::open_memory().unwrap();
    let conn = pool.write().unwrap();
    let pid = project(&conn, "FLO");
    let bot = user(&conn, "claude", true);
    let person = user(&conn, "blake", false);
    let now = now();
    let day = now - Duration::days(2);

    let a = issue(&conn, pid, "A", day - h(5), &[("todo", "done", Some(bot), day)]);
    // Closed by the bot, reopened by a person, closed again by the bot.
    issue(
        &conn,
        pid,
        "B",
        day - h(5),
        &[
            ("todo", "done", Some(bot), day),
            ("done", "todo", Some(person), day + h(1)),
            ("todo", "done", Some(bot), day + h(2)),
        ],
    );
    issue(&conn, pid, "C", day - h(5), &[("backlog", "cancelled", Some(person), day)]);
    // The bot's evidence comment on A, a few minutes before the close.
    audit(&conn, "comment", a, "create", None, "done: see PR", bot, day - Duration::minutes(5));

    let r = flow(&conn, &input(now)).unwrap();
    assert_eq!(r.daily.len(), 30);
    assert_eq!(r.current.done, 3);
    assert_eq!(r.current.cancelled, 1);
    assert_eq!(r.current.reopened, 1);
    assert_eq!(r.current.with_evidence, 1);
    let bot_row = r.per_actor.iter().find(|x| x.actor_id == Some(bot)).unwrap();
    assert_eq!((bot_row.done, bot_row.reopened, bot_row.with_evidence), (3, 1, 1));
    let person_row = r.per_actor.iter().find(|x| x.actor_id == Some(person)).unwrap();
    assert_eq!((person_row.done, person_row.cancelled), (0, 1));
    let closes: i64 = r.closures.iter().filter(|c| c.actor_id == Some(bot)).map(|c| c.done).sum();
    assert_eq!(closes, 3);
    assert_eq!(r.daily.iter().map(|d| d.done).sum::<i64>(), 3);
    assert!(r.actors.iter().any(|x| x.id == bot && x.is_bot));
    // A and B were timed once each, at their latest close.
    assert_eq!(r.timed, 2);
}

#[test]
fn stage_times_follow_the_status_history() {
    let pool = crate::db::open_memory().unwrap();
    let conn = pool.write().unwrap();
    let pid = project(&conn, "FLO");
    let bot = user(&conn, "claude", true);
    let t0 = now() - Duration::days(3);

    issue(
        &conn,
        pid,
        "Walked",
        t0,
        &[
            ("backlog", "todo", Some(bot), t0 + h(1)),
            ("todo", "active", Some(bot), t0 + h(3)),
            ("active", "done", Some(bot), t0 + h(5)),
        ],
    );
    issue(&conn, pid, "Jumped", t0, &[("todo", "done", Some(bot), t0 + h(2))]);

    let r = flow(&conn, &input(now())).unwrap();
    let stage = |name: &str| r.stages.iter().find(|s| s.stage == name).unwrap();
    assert_eq!(stage("backlog").total, 3_600);
    assert_eq!(stage("todo").total, 2 * 3_600 + 2 * 3_600);
    assert_eq!(stage("active").total, 2 * 3_600);
    assert_eq!(stage("active").issues, 1);
    assert_eq!(r.cycle.count, 1);
    assert_eq!(r.cycle.p50, Some(2 * 3_600));
    assert_eq!(r.lead.count, 2);
    assert_eq!(r.lead.p85, Some(5 * 3_600));
    assert_eq!(r.skipped_active, 1);
    assert_eq!(r.slowest[0].title, "Walked");
    let hist: i64 = r.lead_histogram.iter().map(|b| b.count).sum();
    assert_eq!(hist, 2);
    let bucket = |key: &str| r.lead_histogram.iter().find(|b| b.key == key).unwrap().count;
    assert_eq!((bucket("4h"), bucket("1d")), (1, 1));
}

#[test]
fn time_with_people_comes_from_assignments_and_waits() {
    let pool = crate::db::open_memory().unwrap();
    let conn = pool.write().unwrap();
    let pid = project(&conn, "FLO");
    let bot = user(&conn, "claude", true);
    user(&conn, "blake", false);
    let t0 = now() - Duration::days(3);

    let done = issue(&conn, pid, "Asked", t0, &[("todo", "done", Some(bot), t0 + h(6))]);
    audit(&conn, "issue", done, "assign", Some("assignee"), "human", bot, t0 + h(1));
    audit(&conn, "issue", done, "unassign", Some("assignee"), "human", bot, t0 + h(3));
    // Assigning another bot is not a hand-off to a person.
    audit(&conn, "issue", done, "assign", Some("assignee"), "@claude", bot, t0 + h(4));

    let open = issue(&conn, pid, "Waiting", t0, &[("backlog", "todo", Some(bot), t0 + h(1))]);
    audit(&conn, "issue", open, "wait", Some("user"), "@blake: decide", bot, t0 + h(2));

    let r = flow(&conn, &input(now())).unwrap();
    let people = r.stages.iter().find(|s| s.stage == "people").unwrap();
    assert_eq!((people.issues, people.total), (1, 2 * 3_600));
    assert_eq!(r.waiting_total, 1);
    assert_eq!(r.waiting[0].title, "Waiting");
    assert_eq!(r.waiting[0].actor_id, Some(bot));
    assert!(r.waiting_p50.unwrap() > 60 * 3_600);
}

#[test]
fn active_work_shows_who_took_it_and_for_how_long() {
    let pool = crate::db::open_memory().unwrap();
    let conn = pool.write().unwrap();
    let pid = project(&conn, "FLO");
    let bot = user(&conn, "claude", true);
    let t0 = now() - Duration::days(4);
    issue(&conn, pid, "Claimed", t0, &[("todo", "active", Some(bot), t0 + h(1))]);

    let r = flow(&conn, &input(now())).unwrap();
    assert_eq!(r.active_total, 1);
    assert_eq!(r.active[0].actor_id, Some(bot));
    assert!(r.active[0].seconds >= 4 * 86_400 - 2 * 3_600);
    let row = r.per_actor.iter().find(|x| x.actor_id == Some(bot)).unwrap();
    assert_eq!(row.active_now, 1);
}

#[test]
fn windows_and_project_filters_bound_the_counts() {
    let pool = crate::db::open_memory().unwrap();
    let conn = pool.write().unwrap();
    let mine = project(&conn, "ONE");
    let other = project(&conn, "TWO");
    let bot = user(&conn, "claude", true);
    let now = now();
    issue(&conn, mine, "Recent", now - Duration::days(9), &[("todo", "done", Some(bot), now - Duration::days(2))]);
    issue(&conn, mine, "Earlier", now - Duration::days(20), &[("todo", "done", Some(bot), now - Duration::days(10))]);
    issue(&conn, other, "Elsewhere", now - Duration::days(9), &[("todo", "done", Some(bot), now - Duration::days(2))]);

    let only = [mine];
    let r = flow(
        &conn,
        &FlowInput {
            projects: Some(&only),
            days: 7,
            ..input(now)
        },
    )
    .unwrap();
    assert_eq!(r.daily.len(), 7);
    assert_eq!(r.current.done, 1);
    assert_eq!(r.previous.done, 1);
    assert_eq!(r.slowest.len(), 1);

    let hidden: HashSet<i64> = [other].into();
    let r = flow(
        &conn,
        &FlowInput {
            visible: Some(&hidden),
            projects: Some(&only),
            ..input(now)
        },
    )
    .unwrap();
    assert_eq!(r.current.done, 0, "a project filter never widens what the caller can see");
}

#[test]
fn percentiles_use_the_nearest_rank() {
    assert_eq!(percentile(&mut [], 50), None);
    assert_eq!(percentile(&mut [5], 85), Some(5));
    assert_eq!(percentile(&mut [4, 1, 3, 2], 50), Some(2));
    assert_eq!(percentile(&mut [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 85), Some(9));
}
