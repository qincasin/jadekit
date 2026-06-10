#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

function usage() {
  console.error('Usage: node scripts/merge-legacy-providers.cjs <target-db> <source-db-or-json>');
  process.exit(1);
}

const [, , targetDb, sourcePath] = process.argv;
if (!targetDb || !sourcePath) usage();

if (!fs.existsSync(targetDb)) {
  throw new Error(`Target database not found: ${targetDb}`);
}
if (!fs.existsSync(sourcePath)) {
  throw new Error(`Source file not found: ${sourcePath}`);
}

const PROVIDER_COLUMNS = [
  'id',
  'name',
  'app_type',
  'api_key',
  'url',
  'default_sonnet_model',
  'default_opus_model',
  'default_haiku_model',
  'default_reasoning_model',
  'custom_params',
  'settings_config',
  'meta',
  'icon',
  'in_failover_queue',
  'description',
  'tags',
  'is_active',
  'created_at',
  'last_used',
  'proxy_config',
];

function sqliteQuote(value) {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number') return Number.isFinite(value) ? String(value) : 'NULL';
  if (typeof value === 'boolean') return value ? '1' : '0';
  return `'${String(value).replace(/'/g, "''")}'`;
}

function toTimestamp(value, fallback) {
  if (value === null || value === undefined || value === '') return fallback;
  if (typeof value === 'number' && Number.isFinite(value)) return Math.trunc(value);
  const parsed = Date.parse(String(value));
  if (Number.isNaN(parsed)) return fallback;
  return Math.trunc(parsed / 1000);
}

function jsonOrNull(value) {
  if (value === null || value === undefined) return null;
  return JSON.stringify(value);
}

function normalizeProvider(provider) {
  const now = Math.trunc(Date.now() / 1000);
  return {
    id: provider.id,
    name: provider.name,
    app_type: provider.appType ?? provider.app_type,
    api_key: provider.apiKey ?? provider.api_key,
    url: provider.url ?? null,
    default_sonnet_model: provider.defaultSonnetModel ?? provider.default_sonnet_model ?? null,
    default_opus_model: provider.defaultOpusModel ?? provider.default_opus_model ?? null,
    default_haiku_model: provider.defaultHaikuModel ?? provider.default_haiku_model ?? null,
    default_reasoning_model: provider.defaultReasoningModel ?? provider.default_reasoning_model ?? null,
    custom_params: jsonOrNull(provider.customParams ?? provider.custom_params),
    settings_config: jsonOrNull(provider.settingsConfig ?? provider.settings_config),
    meta: jsonOrNull(provider.meta),
    icon: provider.icon ?? null,
    in_failover_queue: provider.inFailoverQueue ?? provider.in_failover_queue ?? false,
    description: provider.description ?? null,
    tags: jsonOrNull(provider.tags ?? null),
    is_active: provider.isActive ?? provider.is_active ?? false,
    created_at: toTimestamp(provider.createdAt ?? provider.created_at, now),
    last_used: toTimestamp(provider.lastUsed ?? provider.last_used, 0),
    proxy_config: jsonOrNull(provider.proxyConfig ?? provider.proxy_config),
  };
}

function mergeFromDb() {
  const sql = `
ATTACH DATABASE ${sqliteQuote(path.resolve(sourcePath))} AS legacy;
INSERT OR REPLACE INTO providers (${PROVIDER_COLUMNS.join(', ')})
SELECT ${PROVIDER_COLUMNS.join(', ')}
FROM legacy.providers;
DETACH DATABASE legacy;
`;
  execFileSync('sqlite3', [targetDb], { input: sql });
  return 'db';
}

function mergeFromJson() {
  const raw = JSON.parse(fs.readFileSync(sourcePath, 'utf8'));
  const providers = Array.isArray(raw) ? raw : Array.isArray(raw.providers) ? raw.providers : [];
  if (providers.length === 0) {
    console.log('Merged 0 providers from JSON.');
    return 'json';
  }

  const statements = ['BEGIN IMMEDIATE;'];
  for (const provider of providers) {
    const row = normalizeProvider(provider);
    if (!row.id || !row.name || !row.app_type || !row.api_key) {
      continue;
    }
    const values = PROVIDER_COLUMNS.map((column) => sqliteQuote(row[column]));
    statements.push(
      `INSERT OR REPLACE INTO providers (${PROVIDER_COLUMNS.join(', ')}) VALUES (${values.join(', ')});`
    );
  }
  statements.push('COMMIT;');
  execFileSync('sqlite3', [targetDb], { input: statements.join('\n') });
  return 'json';
}

const sourceExt = path.extname(sourcePath).toLowerCase();
const sourceType = sourceExt === '.db' ? mergeFromDb() : mergeFromJson();
console.log(`Merged providers from ${sourceType}: ${sourcePath}`);
