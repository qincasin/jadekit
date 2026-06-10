#!/usr/bin/env bash
set -euo pipefail

TARGET_DIR="${JADEKIT_DATA_DIR:-$HOME/.jadekit}"
BACKUP_ROOT="${JADEKIT_MIGRATION_BACKUP_DIR:-$HOME/.jadekit-migration-backups}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
BACKUP_DIR="$BACKUP_ROOT/$TIMESTAMP"
OLD_DIRS=("$HOME/.ccg-switch" "$HOME/.claude-switch" "$HOME/.ccswitch")
MIGRATE_FILES=(
  "config.json"
  "providers.json"
  "tokens.json"
  "proxy_config.json"
  "global-proxy.json"
  "webdav.json"
  "skill-apps.json"
  "tool_versions_cache.json"
)
MIGRATE_DIRS=(
  "skills"
  "prompts"
)

if ! command -v rsync >/dev/null 2>&1; then
  echo "rsync is required for non-overwriting migration." >&2
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "node is required for provider merge migration." >&2
  exit 1
fi

if ! command -v sqlite3 >/dev/null 2>&1; then
  echo "sqlite3 is required for provider merge migration." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_MERGE_SCRIPT="$SCRIPT_DIR/merge-legacy-providers.cjs"

merge_providers_into_target_db() {
  local source_path="$1"
  local source_label="$2"
  local target_db="$TARGET_DIR/jadekit.db"

  if [[ ! -f "$target_db" ]]; then
    return
  fi

  if [[ ! -f "$source_path" ]]; then
    return
  fi

  node "$PROVIDER_MERGE_SCRIPT" "$target_db" "$source_path"
  echo "Merged providers from $source_label into $target_db"
}

mkdir -p "$TARGET_DIR" "$BACKUP_DIR"

copied_any=false

copy_file_if_present() {
  local old_dir="$1"
  local rel_path="$2"
  local src="$old_dir/$rel_path"
  local dst="$TARGET_DIR/$rel_path"
  local backup_dst="$BACKUP_DIR/$(basename "$old_dir")/$rel_path"

  if [[ ! -f "$src" ]]; then
    return
  fi

  copied_any=true
  mkdir -p "$(dirname "$dst")" "$(dirname "$backup_dst")"
  cp -p "$src" "$backup_dst"

  if [[ -e "$dst" ]]; then
    echo "Skipped existing file: $dst"
  else
    cp -p "$src" "$dst"
    echo "Copied file: $src -> $dst"
  fi
}

copy_dir_if_present() {
  local old_dir="$1"
  local rel_path="$2"
  local src="$old_dir/$rel_path"
  local dst="$TARGET_DIR/$rel_path"
  local backup_dst="$BACKUP_DIR/$(basename "$old_dir")/$rel_path"

  if [[ ! -d "$src" ]]; then
    return
  fi

  copied_any=true
  mkdir -p "$dst" "$backup_dst"
  rsync -a "$src/" "$backup_dst/"
  rsync -a --ignore-existing "$src/" "$dst/"
  echo "Merged directory: $src -> $dst"
}

for old_dir in "${OLD_DIRS[@]}"; do
  if [[ ! -d "$old_dir" ]]; then
    continue
  fi

  echo "Scanning legacy app data: $old_dir"

  for file in "${MIGRATE_FILES[@]}"; do
    copy_file_if_present "$old_dir" "$file"
  done

  merge_providers_into_target_db "$old_dir/providers.json" "$old_dir/providers.json"

  copy_file_if_present "$old_dir" "jadekit.db"

  if [[ -f "$old_dir/ccg-switch.db" ]]; then
    copied_any=true
    mkdir -p "$BACKUP_DIR/$(basename "$old_dir")"
    cp -p "$old_dir/ccg-switch.db" "$BACKUP_DIR/$(basename "$old_dir")/ccg-switch.db"
    if [[ -e "$TARGET_DIR/jadekit.db" ]]; then
      echo "Skipped legacy database because $TARGET_DIR/jadekit.db already exists."
      merge_providers_into_target_db "$old_dir/ccg-switch.db" "$old_dir/ccg-switch.db"
    else
      cp -p "$old_dir/ccg-switch.db" "$TARGET_DIR/jadekit.db"
      echo "Copied database: $old_dir/ccg-switch.db -> $TARGET_DIR/jadekit.db"
    fi
  fi

  for dir in "${MIGRATE_DIRS[@]}"; do
    copy_dir_if_present "$old_dir" "$dir"
  done
done

if [[ "$copied_any" == false ]]; then
  echo "No migratable legacy app data found."
  exit 0
fi

echo "Migration finished."
echo "Backup: $BACKUP_DIR"
echo "Target: $TARGET_DIR"
echo "Note: chat/session/history/log directories are intentionally not migrated."
