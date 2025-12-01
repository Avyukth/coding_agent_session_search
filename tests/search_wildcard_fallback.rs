use coding_agent_search::connectors::{NormalizedConversation, NormalizedMessage};
use coding_agent_search::search::query::{MatchType, SearchClient, SearchFilters};
use coding_agent_search::search::tantivy::TantivyIndex;
use tempfile::TempDir;

mod util;

#[test]
fn implicit_wildcard_fallback_finds_substrings() {
    let dir = TempDir::new().unwrap();
    let mut index = TantivyIndex::open_or_create(dir.path()).unwrap();

    // Seed index with "apple"
    let conv = NormalizedConversation {
        agent_slug: "tester".into(),
        external_id: None,
        title: Some("fruit test".into()),
        workspace: Some(std::path::PathBuf::from("/tmp")),
        source_path: dir.path().join("log.jsonl"),
        started_at: Some(1000),
        ended_at: None,
        metadata: serde_json::json!({}),
        messages: vec![NormalizedMessage {
            idx: 0,
            role: "user".into(),
            author: None,
            created_at: Some(1000),
            content: "I like eating an apple everyday".into(),
            extra: serde_json::json!({}),
            snippets: vec![],
        }],
    };
    index.add_conversation(&conv).unwrap();
    index.commit().unwrap();

    let client = SearchClient::open(dir.path(), None)
        .unwrap()
        .expect("client");
    let filters = SearchFilters::default();

    // 1. Search "pple" (substring).
    // Exact match "pple" -> 0 hits.
    // Fallback to "*pple*" -> should find "apple".
    // We use sparse_threshold=1 to force fallback if < 1 result.
    let result = client.search_with_fallback("pple", filters.clone(), 10, 0, 1).unwrap();
    let hits = result.hits;

    assert_eq!(hits.len(), 1, "Should find 'apple' via fallback for 'pple'");
    assert_eq!(
        hits[0].match_type,
        MatchType::ImplicitWildcard,
        "Match type should be ImplicitWildcard"
    );
}

#[test]
fn explicit_wildcard_works_without_fallback() {
    let dir = TempDir::new().unwrap();
    let mut index = TantivyIndex::open_or_create(dir.path()).unwrap();

    let conv = NormalizedConversation {
        agent_slug: "tester".into(),
        external_id: None,
        title: Some("wild test".into()),
        workspace: Some(std::path::PathBuf::from("/tmp")),
        source_path: dir.path().join("log.jsonl"),
        started_at: Some(1000),
        ended_at: None,
        metadata: serde_json::json!({}),
        messages: vec![NormalizedMessage {
            idx: 0,
            role: "user".into(),
            author: None,
            created_at: Some(1000),
            content: "config_file_v2.json".into(),
            extra: serde_json::json!({}),
            snippets: vec![],
        }],
    };
    index.add_conversation(&conv).unwrap();
    index.commit().unwrap();

    let client = SearchClient::open(dir.path(), None)
        .unwrap()
        .expect("client");
    let filters = SearchFilters::default();

    // Search "*fig*" -> explicit wildcard
    let hits = client.search("*fig*", filters.clone(), 10, 0).unwrap();
    assert_eq!(hits.len(), 1);
    // Should be Substring because of *x*
    assert_eq!(
        hits[0].match_type,
        MatchType::Substring,
        "Explicit *term* should be Substring"
    );
}
