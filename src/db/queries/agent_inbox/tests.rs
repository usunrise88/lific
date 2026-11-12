use super::*;
use crate::db::models::{AttachmentActor, CommentActor, CreateIssue, CreateProject};
use crate::db::queries;
use crate::db::queries::comments::{CommentParent, create_comment_for_agents, create_comment_with_mentions};

struct F {
    pool: crate::db::DbPool,
    pid: i64,
    other_pid: i64,
    anton: i64,
    yana: i64,
    claude: i64,
    desktop: i64,
    yana_bot: i64,
}

fn user(conn: &Connection, name: &str, owner: Option<i64>, admin: bool) -> i64 {
    conn.execute(
        "INSERT INTO users (username, email, password_hash, display_name, is_admin, is_bot, owner_id)
         VALUES (?1, ?1 || '@test.local', 'x', ?1, ?2, ?3, ?4)",
        params![name, admin, owner.is_some(), owner],
    )
    .unwrap();
    conn.last_insert_rowid()
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

fn fixture() -> F {
    let pool = crate::db::open_memory().unwrap();
    let (pid, other_pid, anton, yana, claude, desktop, yana_bot) = {
        let conn = pool.write().unwrap();
        let pid = project(&conn, "AGT");
        let other_pid = project(&conn, "OTH");
        let anton = user(&conn, "anton", None, false);
        let yana = user(&conn, "yana", None, false);
        let claude = user(&conn, "claude-anton", Some(anton), false);
        let desktop = user(&conn, "desktop-anton", Some(anton), false);
        let yana_bot = user(&conn, "claude-yana", Some(yana), false);
        queries::members::add_member(&conn, pid, anton, "lead").unwrap();
        queries::members::add_member(&conn, pid, yana, "maintainer").unwrap();
        queries::members::add_member(&conn, other_pid, yana, "maintainer").unwrap();
        queries::settings::update(
            &conn,
            queries::settings::InstanceSettingsPatch {
                authz_enforced: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        (pid, other_pid, anton, yana, claude, desktop, yana_bot)
    };
    F { pool, pid, other_pid, anton, yana, claude, desktop, yana_bot }
}

fn act_as(conn: &Connection, user_id: i64) {
    conn.execute("UPDATE _actor_state SET user_id = ?1", params![user_id]).unwrap();
}

fn issue(conn: &Connection, pid: i64, as_user: i64) -> i64 {
    act_as(conn, as_user);
    queries::create_issue(
        conn,
        &CreateIssue {
            project_id: pid,
            title: "Work".into(),
            ..Default::default()
        },
    )
    .unwrap()
    .id
}

fn comment(conn: &Connection, f: &F, issue_id: i64, author: i64, text: &str, picks: &[i64]) -> i64 {
    act_as(conn, author);
    create_comment_for_agents(
        conn,
        CommentParent::Issue(issue_id),
        Some(f.pid),
        CommentActor { user_id: author, is_admin: false },
        AttachmentActor::Unattributed,
        text,
        false,
        picks,
    )
    .unwrap()
    .id
}

fn inbox(conn: &Connection, agent: i64) -> Vec<String> {
    take(conn, agent, 50, false).unwrap().into_iter().map(|i| format!("{}:{}", i.reason, i.content)).collect()
}

#[test]
fn a_persons_reply_reaches_the_agent_that_created_the_issue() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.pid, f.claude);
    comment(&conn, &f, i, f.anton, "yes, go ahead", &[]);
    assert_eq!(inbox(&conn, f.claude), ["attached:yes, go ahead"]);
    assert!(inbox(&conn, f.claude).is_empty(), "taken once");
    assert!(inbox(&conn, f.desktop).is_empty(), "the owner's other agent was not on it");
}

#[test]
fn explicit_addressees_win_over_attached_agents() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.pid, f.claude);
    comment(&conn, &f, i, f.anton, "@desktop-anton take this one", &[]);
    assert!(inbox(&conn, f.claude).is_empty());
    assert_eq!(inbox(&conn, f.desktop), ["mention:@desktop-anton take this one"]);
    comment(&conn, &f, i, f.anton, "picked", &[f.claude]);
    assert_eq!(inbox(&conn, f.claude), ["picked:picked"]);
}

#[test]
fn project_routes_catch_what_no_agent_is_attached_to() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.pid, f.anton);
    comment(&conn, &f, i, f.anton, "anyone?", &[]);
    assert!(inbox(&conn, f.claude).is_empty());
    set_route(&conn, f.claude, f.pid, None, true, f.anton).unwrap();
    comment(&conn, &f, i, f.yana, "now?", &[]);
    assert_eq!(inbox(&conn, f.claude), ["project_route:now?"]);
    // An issue route outranks the project route.
    set_route(&conn, f.desktop, f.pid, Some(i), true, f.anton).unwrap();
    comment(&conn, &f, i, f.anton, "issue level", &[]);
    assert!(inbox(&conn, f.claude).is_empty());
    assert_eq!(inbox(&conn, f.desktop), ["issue_route:issue level"]);
}

#[test]
fn agents_hear_only_what_is_addressed_and_never_themselves() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.pid, f.claude);
    // Another agent's plain comment is not routed implicitly.
    comment(&conn, &f, i, f.yana_bot, "status update", &[]);
    assert!(inbox(&conn, f.claude).is_empty());
    // But it can address an agent by name, and not itself.
    comment(&conn, &f, i, f.yana_bot, "@claude-anton @claude-yana ping", &[]);
    assert_eq!(inbox(&conn, f.claude), ["mention:@claude-anton @claude-yana ping"]);
    assert!(inbox(&conn, f.yana_bot).is_empty());
}

#[test]
fn an_agent_whose_owner_cannot_see_the_project_gets_nothing() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.other_pid, f.yana);
    act_as(&conn, f.yana);
    create_comment_with_mentions(
        &conn,
        CommentParent::Issue(i),
        Some(f.other_pid),
        CommentActor { user_id: f.yana, is_admin: false },
        AttachmentActor::Unattributed,
        "@claude-anton look",
        false,
    )
    .unwrap();
    assert!(inbox(&conn, f.claude).is_empty());
    let seen = agents_for_project(&conn, f.other_pid).unwrap();
    assert!(seen.iter().all(|a| a.owner_id == Some(f.yana)), "{seen:?}");
}

#[test]
fn peeking_leaves_messages_waiting_and_deliveries_show_on_the_issue() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.pid, f.claude);
    let c = comment(&conn, &f, i, f.anton, "first", &[]);
    assert_eq!(take(&conn, f.claude, 10, true).unwrap().len(), 1);
    assert_eq!(pending_count(&conn, f.claude).unwrap(), 1);
    let items = take(&conn, f.claude, 10, false).unwrap();
    assert!(items[0].author_is_owner);
    assert_eq!(items[0].identifier, "AGT-1");
    let d = deliveries_for_issue(&conn, i).unwrap();
    assert_eq!(d[&c].len(), 1);
    assert!(d[&c][0].delivered_at.is_some());
    assert_eq!(d[&c][0].agent.owner_name.as_deref(), Some("anton"));
}

#[test]
fn an_edit_that_adds_a_mention_delivers_to_the_new_agent() {
    let f = fixture();
    let conn = f.pool.write().unwrap();
    let i = issue(&conn, f.pid, f.anton);
    let c = comment(&conn, &f, i, f.anton, "draft", &[]);
    act_as(&conn, f.anton);
    queries::comments::update_comment_with_mentions(
        &conn,
        c,
        Some(f.pid),
        AttachmentActor::Unattributed,
        "draft @claude-anton",
        false,
    )
    .unwrap();
    assert_eq!(inbox(&conn, f.claude), ["mention:draft @claude-anton"]);
    let _ = f.yana;
}
