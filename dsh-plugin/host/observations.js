// Explicit quota observation for the providers tmax manages.
//
// A GET may read the host state; only an explicit POST observes. Every
// observation is one `tmax provider refresh <id> --scope quota`, and the
// result is tmax's own typed envelope. The plugin holds no vendor endpoint, no
// credential resolution, and no quota reading of its own: which window means
// what belongs to tmax.
import { readEnvelope, REFRESH_TIMEOUT_MS } from './cli.js'

const BLOCKED_REASON_CODES = new Set([
  'unsupported_source',
  'credential_unavailable',
  'target_incomplete',
  'noninteractive_unavailable',
  'interactive_required',
  'login_source_required',
  'native_not_installed',
  'native_version_unknown',
  'native_version_unqualified',
  'native_schema_incompatible',
  'endpoint_conflict',
  'restart_required',
  'daemon_incompatible',
])

/** Human text for a deterministic observation blocker. */
function blockedMessage(code) {
  if (code === 'unsupported_source') return 'this provider has no supported observation source'
  if (code === 'credential_unavailable') return 'the credential for this provider is unavailable'
  if (code === 'target_incomplete') return 'the provider configuration is incomplete'
  return `this provider cannot be observed now (${code})`
}

/** Classify one tmax enrollment refresh envelope into one item result. */
function classifyTmaxItem(id, result, dryRun) {
  const item = { source: 'tmax', id }
  if (result.envelope === undefined) {
    item.status = 'failed'
    item.code = 'tmax_unavailable'
    item.message = result.message || 'tmax did not answer'
    return item
  }
  const envelope = result.envelope
  const data = envelope.data !== null && typeof envelope.data === 'object' ? envelope.data : {}
  const checks = Array.isArray(data.checks)
    ? data.checks.map(check => ({
      scope: typeof check?.scope === 'string' ? check.scope : null,
      outcome: typeof check?.outcome === 'string' ? check.outcome : null,
      reasonCode: typeof check?.reason_code === 'string' ? check.reason_code : null,
      published: check?.published ?? (check?.outcome === 'completed' ? true : false),
    }))
    : undefined
  if (checks !== undefined) item.checks = checks
  const reasonCodes = (checks ?? []).map(check => check.reasonCode).filter(code => code !== null)
  const blockedCode = reasonCodes.find(code => BLOCKED_REASON_CODES.has(code))
  if (dryRun && envelope.code === 'dry_run_ready') {
    if (blockedCode !== undefined) {
      item.status = 'blocked'
      item.code = blockedCode
      item.message = blockedMessage(blockedCode)
    } else {
      item.status = 'unchanged'
      item.code = 'dry_run'
      item.message = 'dry run: no check was executed'
    }
    return item
  }
  if (result.ok) {
    item.status = 'refreshed'
    return item
  }
  const message = result.message || `tmax provider refresh failed: ${envelope.code || 'unknown'}`
  if (blockedCode !== undefined) {
    item.status = 'blocked'
    item.code = blockedCode
    item.message = blockedMessage(blockedCode)
  } else if (BLOCKED_REASON_CODES.has(envelope.code)) {
    item.status = 'blocked'
    item.code = envelope.code
    item.message = blockedMessage(envelope.code)
  } else {
    item.status = 'failed'
    item.code = typeof envelope.code === 'string' ? envelope.code : 'observation_failed'
    item.message = message
  }
  return item
}


/**
 * Explicitly refresh quota observations for every provider tmax manages.
 * One `provider refresh <id> --scope quota` per provider; `--dry-run` when the
 * caller asked for a preview. Results are per item, never a generic success.
 * @param {object} config
 * @param {{dryRun?: boolean, readEnvelopeImpl?: Function}} [options]
 */
export async function observeTmax(config, { dryRun = false } = {}) {
  const listed = await readEnvelope(config, ['provider', 'list'])
  if (!listed.ok) {
    return {
      items: [{
        source: 'tmax',
        id: 'tmax:provider-list',
        status: 'failed',
        code: listed.code,
        message: listed.message || 'tmax provider list failed',
      }],
    }
  }
  const ids = (listed.data?.providers ?? [])
    .map(provider => (typeof provider?.id === 'string' && provider.id !== '' ? provider.id : undefined))
    .filter(id => id !== undefined)
  const items = await Promise.all(ids.map(async id => {
    const args = ['provider', 'refresh', id, '--scope', 'quota']
    if (dryRun) args.push('--dry-run')
    const result = await readEnvelope(config, args, { timeoutMs: REFRESH_TIMEOUT_MS })
    return classifyTmaxItem(id, result, dryRun)
  }))
  return { items }
}
