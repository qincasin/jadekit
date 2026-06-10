# Update Source Selection Design

## Goal

Use `qincasin/jadekit` as the single GitHub release source for manual and automatic update checks.

## Sources

- `qincasin/jadekit`

## Data Layer

### Config

Keep `update_source: String` field in Config (default: `"qincasin/jadekit"`, serde rename: `"updateSource"`), and normalize legacy values to the default source.

### Rust: updater_service.rs

- `check_update(current_version, repo: &str)` checks the configured source.
- `check_update_all_sources(current_version)` returns one entry for `qincasin/jadekit` for compatibility with the existing UI flow.
- `check_update_and_emit` (auto-check) uses config's normalized `update_source` field.

### New Tauri Commands

- `check_for_updates_all_sources` → calls `check_update_all_sources`, returns the configured source's update info.

### Frontend Types

Add `SourceUpdateInfo`:
```typescript
interface SourceUpdateInfo {
  repo: string;
  updateInfo: UpdateInfo;
}
```

## UI Layer

### UpdateBanner

- Manual check invokes `check_for_updates_all_sources`.
- When updates are found, display the configured source with repo name, version, and publish date.

### Settings Page

- Show "Update Source" below auto-check toggle as `qincasin/jadekit`.

## Data Flow

```
Manual check → check_for_updates_all_sources → returns qincasin/jadekit
  → UI shows source → download_update(source URL)

Auto check → check_update_and_emit → uses config.update_source → emit event
```
