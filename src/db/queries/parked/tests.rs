use super::*;
use crate::db::models::{CreateIssue, CreateProject, ListIssuesQuery, Status, UpdateIssue};
use crate::db::queries;

fn setup() -> (crate::db::DbPool, i64, i64, i64) {
    let pool = crate::db::open_memory().unwrap();
    let (pid, anton) = {
        let conn = pool.write().unwrap();
        let pid = queries::create_project(
            &conn,
            &CreateProject { name: "Park".into(), identifier: "PRK".into(), ..Default::default() },
        )
        .unwrap()
        .id;
        conn.execute(
            "INSERT INTO users (username, email, password_hash, display_name, is_admin, is_bot)
             VALUES ('anton', 'anton@test.local', 'x', 'Anton', 0, 0)",
            [],
        )
        .unwrap();
        let anton = conn.last_insert_rowid();
        queries::members::add_member(&conn, pid, anton, "lead").unwrap();
        (pid, anton)
    };
    let other = {
        let conn = pool.write().unwrap();
        conn.execute("UPDATE _actor_state SET user_id = ?1", [anton]).unwrap();
        queries::create_issue(&conn, &CreateIssue { project_id: pid, title: "Work".into(), ..Default::default() })
            .unwrap()
            .id
    };
    (pool, pid, anton, other)
}

fn idea(conn: &Connection, pid: i64, title: &str) -> i64 {
    queries::create_issue(conn, &CreateIssue { project_id: pid, title: title.into(), ..Default::default() })
        .unwrap()
        .id
}

fn titles(conn: &Connection, pid: i64, parked: Option<bool>) -> Vec<String> {
    let mut t: Vec<String> = queries::list_issues(
        conn,
        &ListIssuesQuery { project_id: Some(pid), parked, ..Default::default() },
    )
    .unwrap()
    .into_iter()
    .map(|i| i.title)
    .collect();
    t.sort();
    t
}

#[test]
fn a_parked_issue_leaves_lists_and_counts_but_keeps_its_data() {
    let (pool, pid, anton, _) = setup();
    let conn = pool.write().unwrap();
    let id = idea(&conn, pid, "Dark mode");
    let seq_before = queries::issue_seq(&conn, id).unwrap();
    let p = park(&conn, id, "  nice to have ", Some(anton)).unwrap();
    assert_eq!(p.reason, "nice to have");
    assert_eq!(p.by.as_deref(), Some("anton"));
    assert!(queries::issue_seq(&conn, id).unwrap() > seq_before, "seq advanced for sync");

    assert_eq!(titles(&conn, pid, Some(false)), ["Work"]);
    assert_eq!(titles(&conn, pid, Some(true)), ["Dark mode"]);
    assert_eq!(titles(&conn, pid, None), ["Dark mode", "Work"], "internal readers see both");
    assert_eq!(queries::count_issues_by_status(&conn, pid).unwrap().total, 1);
    assert_eq!(counts_by_project(&conn).unwrap()[&pid], 1);

    let issue = queries::get_issue(&conn, id).unwrap();
    assert_eq!(issue.status, Status::Backlog);
    assert_eq!(issue.parked.unwrap().reason, "nice to have");

    // Parking again only changes the reason.
    park(&conn, id, "later", None).unwrap();
    assert_eq!(get(&conn, id).unwrap().unwrap().reason, "later");

    assert!(unpark(&conn, id).unwrap());
    assert!(!unpark(&conn, id).unwrap());
    assert_eq!(titles(&conn, pid, Some(false)), ["Dark mode", "Work"]);

    let actions: Vec<(String, Option<String>, Option<String>)> = conn
        .prepare("SELECT action, new_value, old_value FROM audit_log WHERE issue_id = ?1 AND field = 'parked' ORDER BY id")
        .unwrap()
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        actions,
        [
            ("park".into(), Some("nice to have".into()), None),
            ("unpark".into(), None, Some("later".into())),
        ]
    );
}

#[test]
fn taking_an_issue_up_unparks_it() {
    let (pool, pid, anton, _) = setup();
    let conn = pool.write().unwrap();
    let a = idea(&conn, pid, "A");
    let b = idea(&conn, pid, "B");
    let c = idea(&conn, pid, "C");
    for id in [a, b, c] {
        park(&conn, id, "", None).unwrap();
    }
    // A status change.
    queries::update_issue(&conn, a, &UpdateIssue { status: Some(Status::Todo), ..Default::default() }).unwrap();
    assert!(get(&conn, a).unwrap().is_none());
    // Naming who does it.
    queries::update_issue(&conn, b, &UpdateIssue { assignees: Some(vec!["anton".into()]), ..Default::default() })
        .unwrap();
    assert!(get(&conn, b).unwrap().is_none());
    // Other edits keep it parked.
    queries::update_issue(&conn, c, &UpdateIssue { title: Some("C2".into()), ..Default::default() }).unwrap();
    assert!(get(&conn, c).unwrap().is_some());
    let _ = anton;
}

#[test]
fn closed_issues_cannot_be_parked() {
    let (pool, pid, _, _) = setup();
    let conn = pool.write().unwrap();
    let id = idea(&conn, pid, "Old");
    queries::update_issue(&conn, id, &UpdateIssue { status: Some(Status::Done), ..Default::default() }).unwrap();
    let err = park(&conn, id, "", None).unwrap_err().to_string();
    assert!(err.contains("closed"), "{err}");
}
