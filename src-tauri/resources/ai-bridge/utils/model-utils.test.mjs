import test from 'node:test';
import assert from 'node:assert/strict';

import {
    mapModelIdToSdkName,
    modelSupportsVision,
    resolveModelFromSettings,
    setModelEnvironmentVariables,
} from './model-utils.js';

// --- mapModelIdToSdkName ------------------------------------------------

test('mapModelIdToSdkName maps Claude families to short SDK names', () => {
  assert.equal(mapModelIdToSdkName('claude-opus-4-7'), 'opus');
  assert.equal(mapModelIdToSdkName('claude-haiku-4-5'), 'haiku');
  assert.equal(mapModelIdToSdkName('claude-sonnet-4-6'), 'sonnet');
  // Unknown / third-party IDs fall back to sonnet (because the SDK uses
  // ANTHROPIC_DEFAULT_SONNET_MODEL as the lookup target for arbitrary names).
  assert.equal(mapModelIdToSdkName('mimo-v2.5-pro'), 'sonnet');
  assert.equal(mapModelIdToSdkName(''), 'sonnet');
  assert.equal(mapModelIdToSdkName(null), 'sonnet');
});

// --- resolveModelFromSettings -------------------------------------------

test('resolveModelFromSettings returns original when no settings env provided', () => {
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6', null), 'claude-sonnet-4-6');
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6', {}), 'claude-sonnet-4-6');
});

test('resolveModelFromSettings keeps explicit Claude model selections over family defaults', () => {
  const env = {
    ANTHROPIC_DEFAULT_OPUS_MODEL: 'claude-opus-4-7',
    ANTHROPIC_DEFAULT_SONNET_MODEL: 'glm-4.7',
    ANTHROPIC_DEFAULT_HAIKU_MODEL: 'glm-4.7-flash',
  };
  assert.equal(resolveModelFromSettings('claude-opus-4-8', env), 'claude-opus-4-8');
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6', env), 'claude-sonnet-4-6');
  assert.equal(resolveModelFromSettings('claude-haiku-4-5', env), 'claude-haiku-4-5');
});

test('resolveModelFromSettings honors global ANTHROPIC_MODEL override', () => {
  const env = {
    ANTHROPIC_MODEL: 'override-everywhere',
    ANTHROPIC_DEFAULT_SONNET_MODEL: 'ignored',
  };
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6', env), 'override-everywhere');
  assert.equal(resolveModelFromSettings('claude-opus-4-7', env), 'override-everywhere');
});

test('resolveModelFromSettings ignores empty / whitespace mapping values', () => {
  const env = {
    ANTHROPIC_DEFAULT_SONNET_MODEL: '   ',
    ANTHROPIC_DEFAULT_OPUS_MODEL: '',
  };
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6', env), 'claude-sonnet-4-6');
  assert.equal(resolveModelFromSettings('claude-opus-4-7', env), 'claude-opus-4-7');
});

test('resolveModelFromSettings does NOT remap non-Anthropic model IDs', () => {
  // A third-party model name like 'qwen3-max' should pass through unchanged
  // even when ANTHROPIC_DEFAULT_SONNET_MODEL is configured. Otherwise we would
  // silently rewrite intentional model selections.
  const env = { ANTHROPIC_DEFAULT_SONNET_MODEL: 'glm-4.7' };
  assert.equal(resolveModelFromSettings('qwen3-max', env), 'qwen3-max');
  assert.equal(resolveModelFromSettings('deepseek-v4-pro', env), 'deepseek-v4-pro');
});

// --- [1m] suffix follows the webview request state ------------------------
//
// The Chat request modelId is the source of truth. Provider family defaults may
// have supplied model-list options, but they must not rewrite the user's
// selected id. Preserve [1m] when the toggle is on, and strip a stale suffix from
// the selected request model when the toggle is off.

test('resolveModelFromSettings preserves [1m] on explicit Claude selection despite family defaults', () => {
  const env = { ANTHROPIC_DEFAULT_SONNET_MODEL: 'glm-4.7' };
  assert.equal(
    resolveModelFromSettings('claude-sonnet-4-6[1m]', env),
    'claude-sonnet-4-6[1m]',
    'request asked for 1M, selected model must keep the [1m] suffix so the SDK enables 1M context'
  );
});

test('resolveModelFromSettings keeps explicit Opus version with [1m] despite older provider default', () => {
  const env = { ANTHROPIC_DEFAULT_OPUS_MODEL: 'claude-opus-4-7' };
  assert.equal(
    resolveModelFromSettings('claude-opus-4-8[1m]', env),
    'claude-opus-4-8[1m]'
  );
});

test('resolveModelFromSettings preserves selected custom provider model id and [1m]', () => {
  const env = { ANTHROPIC_DEFAULT_SONNET_MODEL: 'deepseek-v4-pro[1m]' };
  assert.equal(
    resolveModelFromSettings('MiniMax-M2.5[1m]', env),
    'MiniMax-M2.5[1m]'
  );
});

test('resolveModelFromSettings ignores stale family-default [1m] suffix when 1M toggle is OFF', () => {
  const env = { ANTHROPIC_DEFAULT_SONNET_MODEL: 'glm-4.7[1M]' };
  assert.equal(
    resolveModelFromSettings('MiniMax-M2.5', env),
    'MiniMax-M2.5',
    'request did not ask for 1M, stale settings family default suffix must not force it on'
  );
});

test('resolveModelFromSettings preserves [1m] across ANTHROPIC_MODEL global override', () => {
  const env = { ANTHROPIC_MODEL: 'override-model[1M]' };
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6[1m]', env), 'override-model[1m]');
  assert.equal(resolveModelFromSettings('claude-sonnet-4-6', env), 'override-model');
});

test('resolveModelFromSettings preserves explicit opus [1m] without applying opus mapping', () => {
  const env = { ANTHROPIC_DEFAULT_OPUS_MODEL: 'mimo-v2.5-pro' };
  assert.equal(resolveModelFromSettings('claude-opus-4-7[1m]', env), 'claude-opus-4-7[1m]');
});

test('resolveModelFromSettings can opt into legacy family default mapping for non-chat callers', () => {
  const env = {
    ANTHROPIC_DEFAULT_HAIKU_MODEL: 'glm-4.7-flash',
    ANTHROPIC_DEFAULT_OPUS_MODEL: 'claude-opus-4-7',
  };
  assert.equal(
    resolveModelFromSettings('claude-haiku-4-5', env, {
      allowFamilyDefaultMapping: true,
    }),
    'glm-4.7-flash'
  );
  assert.equal(
    resolveModelFromSettings('claude-opus-4-8', env, {
      allowFamilyDefaultMapping: true,
    }),
    'claude-opus-4-7'
  );
});

test('resolveModelFromSettings opt-in family mapping still follows request-owned [1m] state', () => {
  const env = {
    ANTHROPIC_DEFAULT_HAIKU_MODEL: 'glm-4.7-flash[1m]',
    ANTHROPIC_DEFAULT_SONNET_MODEL: 'glm-4.7',
  };
  assert.equal(
    resolveModelFromSettings('claude-haiku-4-5', env, {
      allowFamilyDefaultMapping: true,
    }),
    'glm-4.7-flash'
  );
  assert.equal(
    resolveModelFromSettings('claude-sonnet-4-6[1m]', env, {
      allowFamilyDefaultMapping: true,
    }),
    'glm-4.7[1m]'
  );
});

// --- setModelEnvironmentVariables ---------------------------------------

test('setModelEnvironmentVariables sets sonnet env for sonnet-family base model', () => {
  const previous = {
    ANTHROPIC_MODEL: process.env.ANTHROPIC_MODEL,
    ANTHROPIC_DEFAULT_SONNET_MODEL: process.env.ANTHROPIC_DEFAULT_SONNET_MODEL,
  };
  try {
    delete process.env.ANTHROPIC_MODEL;
    delete process.env.ANTHROPIC_DEFAULT_SONNET_MODEL;

    setModelEnvironmentVariables('glm-4.7[1m]', 'claude-sonnet-4-6[1m]');

    assert.equal(process.env.ANTHROPIC_MODEL, 'glm-4.7[1m]');
    assert.equal(process.env.ANTHROPIC_DEFAULT_SONNET_MODEL, 'glm-4.7[1m]');
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
});

test('setModelEnvironmentVariables routes haiku base to haiku env', () => {
  const previous = {
    ANTHROPIC_MODEL: process.env.ANTHROPIC_MODEL,
    ANTHROPIC_DEFAULT_HAIKU_MODEL: process.env.ANTHROPIC_DEFAULT_HAIKU_MODEL,
  };
  try {
    delete process.env.ANTHROPIC_MODEL;
    delete process.env.ANTHROPIC_DEFAULT_HAIKU_MODEL;

    setModelEnvironmentVariables('glm-4.7-flash', 'claude-haiku-4-5');

    assert.equal(process.env.ANTHROPIC_DEFAULT_HAIKU_MODEL, 'glm-4.7-flash');
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
});

// --- modelSupportsVision -------------------------------------------------

test('modelSupportsVision only matches the canonical claude- prefix', () => {
  assert.equal(modelSupportsVision('claude-sonnet-4-6'), true);
  assert.equal(modelSupportsVision('claude-opus-4-7'), true);
  // Third-party proxies that merely contain "claude" must NOT be treated as
  // native vision-capable models.
  assert.equal(modelSupportsVision('claude-compatible-proxy'), true); // starts with 'claude-'
  assert.equal(modelSupportsVision('mimo-claude-bridge'), false);
  assert.equal(modelSupportsVision('glm-4.7'), false);
  assert.equal(modelSupportsVision('deepseek-v4-pro[1m]'), false);
  assert.equal(modelSupportsVision(''), true);
  assert.equal(modelSupportsVision(null), true);
});
