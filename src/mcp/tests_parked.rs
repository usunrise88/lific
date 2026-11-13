//! Fork: parked issues through update_issue, bulk_update, list_issues,
//! get_issue and get_board.

use rmcp::handler::server::wrapper::Parameters;

use super::*;

fn as_user(user: &models::AuthUser, f: impl FnOnce() -> String) -> String {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(crate::mcp::with_request_user(Some(user.clone()), || async { f() }))
}

#[test]
fn parked_issues_stay_out_of_what_agents_read_until_asked() {
    let (db, _admin, _lead, maintainer, _viewer, _outsider, _pid) =
        crate::api::test_helpers::setup_membership_test();
    let m = LificMcp::new(db);
    let _guard = acquire_test_guard();
    let me = models::AuthUser {
        id: maintainer.id,
        username: maintainer.username,
        display_name: maintainer.display_name,
        is_admin: false,
    };
    let run = |f: &dyn Fn() -> String| as_user(&me, f);
    for title in ["Ship it", "Dark mode", "Emoji picker"] {
        run(&|| {
            m.create_issue(Parameters(CreateIssueInput {
                project: Some("MEM".into()),
                title: title.into(),
                ..Default::default()
            }))
        });
    }
    let out = run(&|| {
        m.update_issue(Parameters(UpdateIssueInput {
            identifier: "MEM-2".into(),
            parked: Some(true),
            park_reason: Some("nice to have".into()),
            ..Default::default()
        }))
    });
    assert!(out.contains("MEM-2") && out.contains(" parked"), "{out}");
    let out = run(&|| {
        m.bulk_update(Parameters(BulkUpdateInput {
            project: "MEM".into(),
            filter_label: None,
            set_parked: Some(true),
            ..Default::default()
        }))
    });
    // Only the two unparked issues match a bulk filter by default.
    assert_eq!(out, "Updated 2 issue(s)");
    let out = run(&|| {
        m.update_issue(Parameters(UpdateIssueInput {
            identifier: "MEM-1".into(),
            status: Some("active".into()),
            ..Default::default()
        }))
    });
    assert!(!out.contains(" parked"), "taking it up unparks it: {out}");

    let list = |parked: Option<bool>| {
        run(&|| {
            m.list_issues(Parameters(ListIssuesInput {
                project: Some("MEM".into()),
                parked,
                ..Default::default()
            }))
        })
    };
    let open = list(None);
    assert!(open.contains("MEM-1") && !open.contains("MEM-2") && !open.contains("MEM-3"), "{open}");
    let shelf = list(Some(true));
    assert!(shelf.contains("MEM-2") && shelf.contains("MEM-3") && !shelf.contains("MEM-1"), "{shelf}");

    let issue = run(&|| {
        m.get_issue(Parameters(GetIssueInput {
            identifier: "MEM-2".into(),
            include_comments: Some("none".into()),
        }))
    });
    assert!(issue.contains("by @maintainer: nice to have (set aside"), "{issue}");

    let board = run(&|| {
        m.get_board(Parameters(GetBoardInput { project: Some("MEM".into()), ..Default::default() }))
    });
    assert!(!board.contains("Dark mode"), "{board}");

    let err = run(&|| {
        m.update_issue(Parameters(UpdateIssueInput {
            identifier: "MEM-1".into(),
            park_reason: Some("why".into()),
            ..Default::default()
        }))
    });
    assert!(err.contains("park_reason needs parked=true"), "{err}");
}
