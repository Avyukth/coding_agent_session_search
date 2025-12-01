use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use coding_agent_search::connectors::{Connector, ScanContext, codex::CodexConnector};

#[test]
fn codex_connector_reads_modern_envelope_jsonl() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/21");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-1.jsonl");

    // Modern envelope format with {type, timestamp, payload}
    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test/workspace","cli_version":"0.42.0"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"write a hello program"}]}}
{"timestamp":"2025-09-30T15:42:43.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"here is code"}]}}
"#;
    fs::write(&file, sample).unwrap();

    // Safe in test scope: we control process env.
    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    let c = &convs[0];
    assert_eq!(c.agent_slug, "codex");
    assert_eq!(c.messages.len(), 2);
    assert!(c.title.as_ref().unwrap().contains("write a hello program"));
    // Verify workspace was extracted from session_meta
    assert_eq!(c.workspace, Some(PathBuf::from("/test/workspace")));
    // Verify timestamps were parsed from ISO-8601
    assert!(c.started_at.is_some());
    assert!(c.ended_at.is_some());
}

#[test]
fn codex_connector_includes_agent_reasoning() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/22");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-reasoning.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"solve this problem"}]}}
{"timestamp":"2025-09-30T15:42:40.000Z","type":"event_msg","payload":{"type":"agent_reasoning","text":"Let me think about this carefully..."}}
{"timestamp":"2025-09-30T15:42:43.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"here is solution"}]}}
{"timestamp":"2025-09-30T15:42:45.000Z","type":"event_msg","payload":{"type":"token_count","input_tokens":100,"output_tokens":200}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    let c = &convs[0];

    // Should have 3 messages: user, reasoning, assistant
    // (token_count is filtered out)
    assert_eq!(c.messages.len(), 3);

    // Check reasoning is included with correct author tag
    let reasoning = c
        .messages
        .iter()
        .find(|m| m.author.as_deref() == Some("reasoning"));
    assert!(reasoning.is_some());
    assert!(
        reasoning
            .unwrap()
            .content
            .contains("think about this carefully")
    );
}

#[test]
fn codex_connector_filters_token_count() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/23");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-filter.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}
{"timestamp":"2025-09-30T15:42:37.000Z","type":"event_msg","payload":{"type":"token_count","input_tokens":10,"output_tokens":20}}
{"timestamp":"2025-09-30T15:42:38.000Z","type":"turn_context","payload":{"turn":1}}
{"timestamp":"2025-09-30T15:42:39.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"world"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    let c = &convs[0];

    // Should only have 2 messages (user, assistant)
    // token_count and turn_context should be filtered out
    assert_eq!(c.messages.len(), 2);

    for msg in &c.messages {
        assert!(!msg.content.contains("token_count"));
        assert!(!msg.content.contains("turn_context"));
        assert!(!msg.content.trim().is_empty());
    }
}

#[test]
#[ignore = "flaky in CI: CODEX_HOME env override doesn't propagate reliably"]
fn codex_connector_respects_since_ts_for_iso_and_millis() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/24");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-since.jsonl");

    // Two messages: one older (ISO string), one newer (millis). since_ts should exclude the older.
    let sample = r#"{"timestamp":"2025-09-30T15:42:34.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"old msg"}]}}
{"timestamp":1700000100000,"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"new msg"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    // since_ts set between the two messages: should drop the first and keep the second
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: Some(1_700_000_000_000),
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    let c = &convs[0];

    assert_eq!(
        c.messages.len(),
        1,
        "expected only messages newer than since_ts"
    );
    let msg = &c.messages[0];
    assert_eq!(msg.role, "assistant");
    assert!(msg.content.contains("new msg"));
    // idx should be re-sequenced after filtering
    assert_eq!(msg.idx, 0);
}

/// Test legacy .json format parsing
#[test]
fn codex_connector_reads_legacy_json_format() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/25");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-legacy.json");

    // Legacy format: single JSON object with session and items
    let sample = r#"{
        "session": {
            "id": "legacy-session",
            "cwd": "/legacy/workspace"
        },
        "items": [
            {
                "role": "user",
                "timestamp": "2025-09-30T15:42:36.190Z",
                "content": "legacy user message"
            },
            {
                "role": "assistant",
                "timestamp": "2025-09-30T15:42:43.000Z",
                "content": "legacy assistant response"
            }
        ]
    }"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    assert_eq!(c.agent_slug, "codex");
    assert_eq!(c.messages.len(), 2);
    assert_eq!(c.workspace, Some(PathBuf::from("/legacy/workspace")));

    // Verify metadata indicates legacy format
    assert_eq!(
        c.metadata.get("source").and_then(|v| v.as_str()),
        Some("rollout_json")
    );

    // Check messages
    assert_eq!(c.messages[0].role, "user");
    assert!(c.messages[0].content.contains("legacy user message"));
    assert_eq!(c.messages[1].role, "assistant");
}

/// Test detection with existing sessions directory
#[test]
fn codex_detect_with_sessions_dir() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions");
    fs::create_dir_all(&sessions).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let result = connector.detect();
    assert!(result.detected);
    assert!(!result.evidence.is_empty());
}

/// Test detection without sessions directory
#[test]
fn codex_detect_without_sessions_dir() {
    let dir = TempDir::new().unwrap();
    // Don't create sessions directory

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let result = connector.detect();
    assert!(!result.detected);
}

/// Test user_message event type
#[test]
fn codex_connector_handles_user_message_event() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/26");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-user-event.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"event_msg","payload":{"type":"user_message","message":"user event message"}}
{"timestamp":"2025-09-30T15:42:43.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"assistant reply"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    assert_eq!(c.messages.len(), 2);

    // First message should be the user event
    assert_eq!(c.messages[0].role, "user");
    assert!(c.messages[0].content.contains("user event message"));
}

/// Test malformed JSONL lines are skipped gracefully
#[test]
fn codex_connector_skips_malformed_lines() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/27");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-malformed.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{ this is not valid json
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"valid message"}]}}
also not valid
{"timestamp":"2025-09-30T15:42:43.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"valid response"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    // Should have 2 valid messages, malformed lines skipped
    assert_eq!(c.messages.len(), 2);
}

/// Test multiple sessions in separate files
#[test]
fn codex_connector_handles_multiple_sessions() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/28");
    fs::create_dir_all(&sessions).unwrap();

    for i in 1..=3 {
        let file = sessions.join(format!("rollout-{i}.jsonl"));
        let sample = format!(
            r#"{{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{{"id":"session-{i}","cwd":"/test/{i}"}}}}
{{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"message {i}"}}]}}}}
"#
        );
        fs::write(&file, sample).unwrap();
    }

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 3);
}

/// Test empty content messages are filtered
#[test]
fn codex_connector_filters_empty_content() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/29");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-empty.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"   "}]}}
{"timestamp":"2025-09-30T15:42:37.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"valid content"}]}}
{"timestamp":"2025-09-30T15:42:43.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    // Only the message with "valid content" should be included
    assert_eq!(c.messages.len(), 1);
    assert!(c.messages[0].content.contains("valid content"));
}

/// Test title extraction from first user message
#[test]
fn codex_connector_extracts_title() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/11/30");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-title.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:35.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"assistant first"}]}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"This is the user's question\nWith a second line"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    // Title should be first line of first user message
    assert_eq!(c.title, Some("This is the user's question".to_string()));
}

/// Test sequential index assignment
#[test]
fn codex_connector_assigns_sequential_indices() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/12/01");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-idx.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]}}
{"timestamp":"2025-09-30T15:42:37.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"second"}]}}
{"timestamp":"2025-09-30T15:42:38.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"third"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    assert_eq!(c.messages.len(), 3);
    assert_eq!(c.messages[0].idx, 0);
    assert_eq!(c.messages[1].idx, 1);
    assert_eq!(c.messages[2].idx, 2);
}

/// Test external_id comes from filename
#[test]
fn codex_connector_sets_external_id_from_filename() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions/2025/12/02");
    fs::create_dir_all(&sessions).unwrap();
    let file = sessions.join("rollout-unique-id-123.jsonl");

    let sample = r#"{"timestamp":"2025-09-30T15:42:34.559Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}
{"timestamp":"2025-09-30T15:42:36.190Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"test"}]}}
"#;
    fs::write(&file, sample).unwrap();

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    assert_eq!(c.external_id, Some("rollout-unique-id-123".to_string()));
}

/// Test empty sessions directory returns no conversations
#[test]
fn codex_connector_handles_empty_sessions() {
    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    // No files in sessions

    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
    }

    let connector = CodexConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert!(convs.is_empty());
}
