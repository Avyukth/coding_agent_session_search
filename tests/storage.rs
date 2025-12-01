use std::path::PathBuf;

use coding_agent_search::model::types::{Agent, AgentKind, Conversation, Message, MessageRole};
use coding_agent_search::storage::sqlite::SqliteStorage;

fn sample_agent() -> Agent {
    Agent {
        id: None,
        slug: "tester".into(),
        name: "Tester".into(),
        version: Some("1.0".into()),
        kind: AgentKind::Cli,
    }
}

fn sample_conv(external_id: Option<&str>, messages: Vec<Message>) -> Conversation {
    Conversation {
        id: None,
        agent_slug: "tester".into(),
        workspace: Some(PathBuf::from("/workspace/demo")),
        external_id: external_id.map(|s| s.to_owned()),
        title: Some("Demo conversation".into()),
        source_path: PathBuf::from("/logs/demo.jsonl"),
        started_at: Some(1),
        ended_at: Some(2),
        approx_tokens: Some(42),
        metadata_json: serde_json::json!({"k": "v"}),
        messages,
    }
}

fn msg(idx: i64, created_at: i64) -> Message {
    Message {
        id: None,
        idx,
        role: MessageRole::User,
        author: Some("user".into()),
        created_at: Some(created_at),
        content: format!("msg-{idx}"),
        extra_json: serde_json::json!({}),
        snippets: vec![],
    }
}

#[test]
fn schema_version_created_on_open() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("store.db");
    let storage = SqliteStorage::open(&db_path).expect("open");

    assert_eq!(storage.schema_version().unwrap(), 3);

    // If meta row is removed, the getter surfaces an error.
    storage.raw().execute("DELETE FROM meta", []).unwrap();
    assert!(storage.schema_version().is_err());
}

#[test]
fn rebuild_fts_repopulates_rows() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("fts.db");
    let mut storage = SqliteStorage::open(&db_path).expect("open");

    let agent_id = storage.ensure_agent(&sample_agent()).unwrap();
    let ws_id = storage
        .ensure_workspace(PathBuf::from("/workspace/demo").as_path(), Some("Demo"))
        .unwrap();

    let conv = sample_conv(Some("ext-1"), vec![msg(0, 10), msg(1, 20)]);
    storage
        .insert_conversation_tree(agent_id, Some(ws_id), &conv)
        .unwrap();

    let count_messages: i64 = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    let mut fts_count: i64 = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM fts_messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fts_count, count_messages);

    storage
        .raw()
        .execute("DELETE FROM fts_messages", [])
        .unwrap();
    fts_count = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM fts_messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fts_count, 0);

    storage.rebuild_fts().unwrap();
    fts_count = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM fts_messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fts_count, count_messages);
}

#[test]
fn transaction_rolls_back_on_duplicate_idx() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("rollback.db");
    let mut storage = SqliteStorage::open(&db_path).expect("open");

    let agent_id = storage.ensure_agent(&sample_agent()).unwrap();

    // Duplicate idx inside the same conversation should trigger UNIQUE constraint
    // and leave the database unchanged after rollback.
    let conv = sample_conv(None, vec![msg(0, 1), msg(0, 2)]);
    let result = storage.insert_conversation_tree(agent_id, None, &conv);
    assert!(result.is_err());

    let conv_count: i64 = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM conversations", [], |c| c.get(0))
        .unwrap();
    let msg_count: i64 = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM messages", [], |c| c.get(0))
        .unwrap();

    assert_eq!(conv_count, 0);
    assert_eq!(msg_count, 0);
}

#[test]
fn append_only_updates_existing_conversation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("append.db");
    let mut storage = SqliteStorage::open(&db_path).expect("open");

    let agent_id = storage.ensure_agent(&sample_agent()).unwrap();

    let first = sample_conv(Some("ext-2"), vec![msg(0, 100), msg(1, 200)]);
    let outcome1 = storage
        .insert_conversation_tree(agent_id, None, &first)
        .unwrap();
    assert_eq!(outcome1.inserted_indices, vec![0, 1]);

    let second = sample_conv(Some("ext-2"), vec![msg(0, 100), msg(1, 200), msg(2, 300)]);
    let outcome2 = storage
        .insert_conversation_tree(agent_id, None, &second)
        .unwrap();
    assert_eq!(outcome2.conversation_id, outcome1.conversation_id);
    assert_eq!(outcome2.inserted_indices, vec![2]);

    let rows: Vec<(i64, i64)> = storage
        .raw()
        .prepare("SELECT idx, created_at FROM messages ORDER BY idx")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap()))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(rows, vec![(0, 100), (1, 200), (2, 300)]);

    let ended_at: i64 = storage
        .raw()
        .query_row(
            "SELECT ended_at FROM conversations WHERE id = ?",
            [outcome1.conversation_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ended_at, 300);
}

#[test]
fn large_batch_insert_keeps_fts_in_sync() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("batch.db");
    let mut storage = SqliteStorage::open(&db_path).expect("open");

    let agent_id = storage.ensure_agent(&sample_agent()).unwrap();

    // Build a conversation with 200 messages
    let mut msgs = Vec::new();
    for idx in 0..200 {
        msgs.push(msg(idx, 1_000 + idx));
    }
    let conv = sample_conv(Some("batch-1"), msgs);

    let outcome = storage
        .insert_conversation_tree(agent_id, None, &conv)
        .expect("batch insert");
    assert_eq!(outcome.inserted_indices.len(), 200);

    let msg_count: i64 = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    let fts_count: i64 = storage
        .raw()
        .query_row("SELECT COUNT(*) FROM fts_messages", [], |r| r.get(0))
        .unwrap();

    assert_eq!(msg_count, 200);
    assert_eq!(fts_count, 200);

    // Spot check a few message rows for correct ordering and timestamps
    let rows: Vec<(i64, i64)> = storage
        .raw()
        .prepare("SELECT idx, created_at FROM messages ORDER BY idx LIMIT 3 OFFSET 197")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap()))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![(197, 1_197), (198, 1_198), (199, 1_199)],
        "tail rows should preserve order and timestamps"
    );
}

#[test]
fn last_scan_ts_roundtrip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("scan.db");
    let mut storage = SqliteStorage::open(&db_path).expect("open");

    // Initially None
    assert_eq!(storage.get_last_scan_ts().unwrap(), None);

    storage.set_last_scan_ts(1234).expect("set ts");
    assert_eq!(storage.get_last_scan_ts().unwrap(), Some(1234));

    // Reopen and ensure persisted
    drop(storage);
    let storage2 = SqliteStorage::open(&db_path).expect("reopen");
    assert_eq!(storage2.get_last_scan_ts().unwrap(), Some(1234));
}

#[test]
fn last_scan_ts_overwrite() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("scan_over.db");
    let mut storage = SqliteStorage::open(&db_path).expect("open");

    storage.set_last_scan_ts(10).expect("set ts 10");
    storage.set_last_scan_ts(20).expect("set ts 20");
    assert_eq!(storage.get_last_scan_ts().unwrap(), Some(20));
}

#[test]
fn unsupported_schema_version_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("schema.db");

    // First open initializes schema to current version
    let storage = SqliteStorage::open(&db_path).expect("initial open");
    // Poison the schema_version to an unsupported future value
    storage
        .raw()
        .execute(
            "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
    drop(storage); // Close connection before reopening

    let reopen = SqliteStorage::open(&db_path);
    assert!(
        reopen.is_err(),
        "opening with unsupported schema_version should error"
    );
}
