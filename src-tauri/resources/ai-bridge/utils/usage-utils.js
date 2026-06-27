/**
 * Shared usage accumulation utilities for streaming token tracking.
 * Used by message-service.js and persistent-query-service.js.
 */

export const DEFAULT_USAGE = {
  input_tokens: 0,
  output_tokens: 0,
  cache_creation_input_tokens: 0,
  cache_read_input_tokens: 0
};

/**
 * Merge usage data following CLI's nz6() logic.
 * - input_tokens, cache_*: only update if new value > 0 (preserve accumulated)
 * - output_tokens: use new value directly (incremental updates)
 */
export function mergeUsage(accumulated, newUsage) {
  if (!newUsage) return accumulated || { ...DEFAULT_USAGE };
  if (!accumulated) return { ...DEFAULT_USAGE, ...newUsage };
  return {
    input_tokens: newUsage.input_tokens > 0 ? newUsage.input_tokens : accumulated.input_tokens,
    cache_creation_input_tokens: newUsage.cache_creation_input_tokens > 0
      ? newUsage.cache_creation_input_tokens : accumulated.cache_creation_input_tokens,
    cache_read_input_tokens: newUsage.cache_read_input_tokens > 0
      ? newUsage.cache_read_input_tokens : accumulated.cache_read_input_tokens,
    output_tokens: newUsage.output_tokens ?? accumulated.output_tokens
  };
}

/**
 * Derive the effective context-window upper bound from a Claude model id.
 * The webview-controlled settings layer toggles 1M/200K via a `[1m]` suffix on
 * the requested model id; mirror that single source of truth here so every
 * sidecar emit reflects the true window the SDK is using.
 */
export function deriveContextWindow(modelId) {
  if (typeof modelId !== 'string' || modelId.length === 0) return 200_000;
  return modelId.includes('[1m]') ? 1_000_000 : 200_000;
}

/**
 * Build the JSON payload for a [USAGE] IPC line.
 * @param {object} usage  Token usage from accumulated stream state or an assistant message.
 * @param {number} [maxTokens]  Optional context-window upper bound.
 * @returns {object} JSON-serializable [USAGE] payload.
 */
export function buildUsagePayload(usage, maxTokens) {
  const payload = {
    input_tokens: usage?.input_tokens || 0,
    output_tokens: usage?.output_tokens || 0,
    cache_creation_input_tokens: usage?.cache_creation_input_tokens || 0,
    cache_read_input_tokens: usage?.cache_read_input_tokens || 0
  };
  if (typeof maxTokens === 'number' && Number.isFinite(maxTokens) && maxTokens > 0) {
    payload.max_tokens = maxTokens;
  }
  return payload;
}

/**
 * Emit [USAGE] tag from accumulated usage data during streaming.
 * NOTE: Uses process.stdout.write for consistent buffering with other IPC messages.
 * @param {object} accumulated  Accumulated token usage.
 * @param {number} [maxTokens]  Optional context-window upper bound (e.g. 200000 / 1000000).
 *   Forwarded as `max_tokens` so the frontend ring can show the real window
 *   (1M when the requested model has a `[1m]` suffix, 200K otherwise).
 */
export function emitAccumulatedUsage(accumulated, maxTokens) {
  if (!accumulated) return;
  const payload = buildUsagePayload(accumulated, maxTokens);
  process.stdout.write('[USAGE] ' + JSON.stringify(payload) + '\n');
}
