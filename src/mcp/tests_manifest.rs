//! Fork: the agent manifest through `get_manifest`, the `manifest` prompt
//! and the stale-copy line in `get_briefing`.

use rmcp::handler::server::wrapper::Parameters;

use super::*;

fn mcp() -> (LificMcp, crate::db::DbPool, McpTestGuard) {
    let db = crate::db::open_memory().expect("test db");
    db.write()
        .unwrap()
        .execute(
            "INSERT INTO users (username, email, password_hash, display_name, is_admin)
             VALUES ('admin', 'admin@test.local', 'x', 'Admin', 1)",
            [],
        )
        .unwrap();
    let m = LificMcp::new(db.clone());
    let guard = acquire_test_guard();
    let created = m.manage_resource(Parameters(ManageResourceInput {
        resource_type: "project".into(),
        action: "create".into(),
        name: Some("Manifest".into()),
        identifier: Some("MF".into()),
        ..Default::default()
    }));
    assert!(created.starts_with("Created project"), "{created}");
    (m, db, guard)
}

#[test]
fn get_manifest_fills_in_the_project_between_the_markers() {
    let (m, db, _guard) = mcp();
    queries::agent_manifest::set(&db.write().unwrap(), "Track {{project}} work here.").unwrap();
    let out = m.get_manifest(Parameters(GetManifestInput { project: Some("mf".into()) }));
    assert!(out.contains("<!-- lific:begin -->"), "{out}");
    assert!(out.contains("Track MF work here."), "{out}");
    assert!(out.contains("<!-- lific:end -->"), "{out}");

    let bound = LificMcp::new(db.clone()).with_bound_project(Some("MF".into()));
    let out = bound.get_manifest(Parameters(GetManifestInput::default()));
    assert!(out.contains("Track MF work here."), "{out}");
}

#[test]
fn the_manifest_prompt_carries_the_block_and_the_install_steps() {
    let (m, _db, _guard) = mcp();
    let mut req = rmcp::model::GetPromptRequestParams::new("manifest");
    let res = m.manifest_prompt_result(&req).expect("prompt");
    let text = serde_json::to_string(&res.messages).unwrap();
    assert!(text.contains("lific:begin"), "{text}");
    assert!(text.contains("placeholder project APP"), "{text}");

    let mut args = serde_json::Map::new();
    args.insert("project".into(), "MF".into());
    req = req.with_arguments(args);
    let text = serde_json::to_string(&m.manifest_prompt_result(&req).unwrap().messages).unwrap();
    assert!(text.contains("project='MF'"), "{text}");
    assert!(!text.contains("placeholder project APP"), "{text}");

    assert!(m
        .manifest_prompt_result(&rmcp::model::GetPromptRequestParams::new("nope"))
        .is_err());
}

#[test]
fn briefing_says_once_when_the_manifest_changed() {
    let (m, db, _guard) = mcp();
    let briefing = |since: &str| {
        m.get_briefing(Parameters(GetBriefingInput {
            project: Some("MF".into()),
            since: Some(since.into()),
            ..Default::default()
        }))
    };
    let out = briefing("2000-01-01T00:00:00Z");
    assert!(!out.contains("agent manifest changed"), "never edited: {out}");

    queries::agent_manifest::set(&db.write().unwrap(), "New rules.").unwrap();
    let out = briefing("2000-01-01T00:00:00Z");
    assert!(out.contains("agent manifest changed"), "{out}");
    let out = briefing("2999-01-01T00:00:00Z");
    assert!(!out.contains("agent manifest changed"), "{out}");
}
