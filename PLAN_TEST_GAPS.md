# Test Coverage Gap Report (bd-tests-foundation)

## Current coverage snapshot (Nov 23, 2025)
- Unit / connector fixtures: `tests/connector_{codex,cline,gemini,claude,opencode,amp}.rs` (basic parse/normalize); minimal since_ts/dedupe coverage still missing.
- UI snapshots / CLI smoke: `tests/ui_{footer,help,hotkeys,snap}.rs`.
- Search: `search::query` unit covers filters+pagination basics.
- Integration/E2E: none.
- Install scripts: none.
- Watch/incremental: none.
- Logging assertions: none.
- Benchmarks: present but only runtime checks (no assertions in CI).

## High-priority gaps (mapped to beads)
1) Connectors (bd-unit-connectors-complete)
   - Add regression fixtures for since_ts routing, external_id dedupe, idx resequencing, snippet mapping, created_at handling.
   - Assert per-connector agent/workspace tagging and source_path normalization.
2) Storage (bd-unit-storage)
   - `schema_version` getter error surfaces; migration happy-path check.
   - `rebuild_fts` repopulates after manual delete.
   - Transaction rollback on insert failure leaves DB consistent.
   - Append-only path in `insert_conversation_tree` respects `external_id` uniqueness.
3) Indexer (bd-unit-indexer)
   - Full run with `--full` truncates tables/index; append-only `add_messages` preserves prior messages; since_ts routing per connector; watch_state persistence.
4) Search (bd-unit-search)
   - Filters interaction (agent/workspace/time) and pagination boundaries; snippet highlight ordering.
5) TUI (bd-unit-tui-components)
   - Snapshot tests for search bar tips, filter pill clear hotkeys, detail tabs visibility and state when no selection.
6) Watch / incremental (bd-e2e-watch-incremental)
   - Touch fixture → targeted reindex only for affected connector; watch_state.json high-water mark bump.
7) Installers (bd-e2e-install-scripts)
   - install.sh / install.ps1 checksum enforcement (good vs bad), DEST honored, local `file://` artifacts.
8) Logging (bd-logging-coverage)
   - Structured tracing spans for connectors/indexer/search with assertions on key events.
9) E2E smoke (bd-e2e-index-tui-smoke)
   - Seed fixtures, run `index --full`, launch `tui --once`, assert doc count and UI renders without panic.
10) CI wiring (bd-ci-e2e-job)
   - Add CI job that runs install + e2e smokes (watch optional), with timeouts and artifact caching.
11) Docs/help (bd-docs-testing)
   - README testing matrix, env knobs, help text alignment with added tests.

## Proposed test tasks (beads)
- bd-unit-connectors: fixtures + per-connector tests (see below).
- bd-unit-storage: Sqlite schema/version/transaction tests.
- bd-unit-indexer: full vs incremental vs append-only coverage.
- bd-unit-search: filter/highlight/pagination tests.
- bd-unit-tui-components: snapshot tests for bar/pills/detail tabs.
- bd-e2e-index-tui-smoke: seed fixtures, run index --full, launch tui --once, assert logs.
- bd-e2e-watch-incremental: watch run + file touch, assert targeted reindex + watch_state bump.
- bd-e2e-install-scripts: checksum pass/fail, DEST install.
- bd-logging-coverage: tracing span assertions.
- bd-ci-e2e-job: wire above into CI with timeouts.
- bd-docs-testing: README testing matrix + env knobs.

## Fixture plan
- Keep fixtures under `tests/fixtures/` (already present for all connectors) and extend as needed for since_ts and append-only scenarios.
- Add installer tar/zip + matching `.sha256` pairs for positive/negative checksum tests (local `file://`), small (<50KB) to keep CI fast.
- Provide mini watch-mode playground (temp home) with connector-specific paths to validate targeted reindexing.

## Next immediate steps
1) Land storage + indexer unit tests (unblock downstream beads).
2) Add tracing/log capture helper in `tests/util` shared by logging + watch tests.
3) Extend connector fixtures for since_ts/append-only, then add regression tests.
4) Add installer checksum fixtures and e2e harness.
