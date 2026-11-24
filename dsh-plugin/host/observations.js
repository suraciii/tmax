// Observation state for tmax-managed and agent-configured providers.
//
// A GET may read this cache; only an explicit POST observes. tmax quota rows
// come from `provider inspect` (already-published observations) and tmax
// refresh, while providers this DSH profile configures are read by vendor
// readers in `quota.js` after an explicit refresh. Credential resolution
// happens only inside that explicit observation, never on a read path.
import { observeProvider, vendorFor } from '../quota.js'
import { readEnvelope, REFRESH_TIMEOUT_MS } from './cli.js'

/** Reason codes that mean "cannot observe without a configuration change". */
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

/** Reason codes that mean an attempted observation failed in transport/format terms. */
const FAILED_REASON_CODES = new Set([
  'transport_error',
  'observation_deadline',
  'upstream_rejection',
  'source_format_error',
  'observation_failed',
])

/** The vendor whose quota endpoint a configured provider belongs to, if any. */
export function vendorOf(provider) {
  // Only where the models are sent identifies a vendor; a route merely named
  // after one must not receive that vendor's outbound credential or display.
  return vendorFor(provider.baseURL)
}

/** How many agent-configured providers one explicit observation reads at once. */
export const AGENT_OBSERVE_CONCURRENCY = 4

/** The whole agent scope's budget; unfinished providers report `observation_deadline`. */
export const AGENT_OBSERVE_DEADLINE_MS = 45_000

function configuredBaseURL(provider) {
  return typeof provider.baseURL === 'string' && provider.baseURL !== '' ? provider.baseURL : null
}

/** One synthetic record for a provider the scope-wide budget did not finish. */
function deadlineRecord(provider, vendor) {
  return {
    id: `dsh:${provider.provider}`,
    name: provider.provider,
    source: 'dsh',
    vendor: vendor.id,
    vendorName: vendor.name,
    baseURL: configuredBaseURL(provider),
    observedAt: new Date().toISOString(),
    reasonCode: 'observation_deadline',
    message: 'the observation budget ran out before the vendor answered',
    rows: [],
    windows: [],
  }
}

/** One synthetic record for an observation that threw instead of reporting. */
function unexpectedFailureRecord(provider, vendor) {
  return {
    id: `dsh:${provider.provider}`,
    name: provider.provider,
    source: 'dsh',
    vendor: vendor.id,
    vendorName: vendor.name,
    baseURL: configuredBaseURL(provider),
    observedAt: new Date().toISOString(),
    reasonCode: 'observation_failed',
    message: 'the observation failed unexpectedly',
    rows: [],
    windows: [],
  }
}

const SCOPE_EXPIRED = Symbol('agent observation scope expired')

/**
 * Run unique observations with bounded concurrency and one scope-wide budget.
 *
 * Tasks keep their discovery order. A task that finishes before the budget
 * keeps its record; a task still queued or in flight when the budget expires
 * becomes `observation_deadline`. The budget signal also aborts in-flight
 * vendor requests, so the function settles promptly instead of waiting for a
 * slow vendor. Never rejects: an observation that throws becomes a failed
 * record, and a late result after expiry is discarded rather than cached.
 */
async function observeWithBudget(tasks, { limit, deadlineMs, observe }) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), deadlineMs)
  const expired = new Promise(resolve => {
    controller.signal.addEventListener('abort', () => resolve(SCOPE_EXPIRED), { once: true })
  })
  let cursor = 0
  const worker = async () => {
    for (;;) {
      const index = cursor
      cursor += 1
      if (index >= tasks.length) return
      const task = tasks[index]
      if (controller.signal.aborted) {
        task.record = deadlineRecord(task.provider, task.vendor)
        continue
      }
      const observation = Promise.resolve()
        .then(() => observe(task, controller.signal))
        .then(
          record => record,
          () => unexpectedFailureRecord(task.provider, task.vendor),
        )
      const outcome = await Promise.race([observation, expired])
      task.record = outcome === SCOPE_EXPIRED
        ? deadlineRecord(task.provider, task.vendor)
        : outcome
    }
  }
  const workers = []
  for (let index = 0; index < Math.min(limit, tasks.length); index += 1) workers.push(worker())
  try {
    await Promise.all(workers)
  } finally {
    clearTimeout(timer)
  }
}

/** Map one vendor observation to the provider record the state contract exposes. */
function stateRecord(provider, observed) {
  const vendor = vendorOf(provider)
  const base = {
    id: `dsh:${provider.provider}`,
    provider: provider.provider,
    displayName: provider.displayName ?? provider.provider,
    vendor: vendor?.id ?? null,
    baseURL: configuredBaseURL(provider),
    models: Array.isArray(provider.models) ? [...provider.models] : [],
  }
  if (observed === undefined) {
    return {
      ...base,
      state: 'not_observed',
      observedAt: null,
      reasonCode: vendor === undefined ? 'unsupported_source' : null,
      message: null,
      plan: null,
      rows: [],
    }
  }
  const reasonCode = typeof observed.reasonCode === 'string' ? observed.reasonCode : null
  const rows = Array.isArray(observed.rows) ? observed.rows : []
  const blocked = reasonCode !== null && BLOCKED_REASON_CODES.has(reasonCode)
  const failed = reasonCode !== null && FAILED_REASON_CODES.has(reasonCode)
  return {
    ...base,
    state: rows.length > 0 && reasonCode === null ? 'observed' : failed ? 'failed' : 'not_observed',
    observedAt: blocked ? null : (typeof observed.observedAt === 'string' ? observed.observedAt : null),
    reasonCode,
    message: typeof observed.message === 'string' ? observed.message : null,
    plan: observed.plan ?? null,
    rows,
  }
}

/** One observation item result for the explicit POST /_tmax/observe envelope. */
function itemFor(provider, status, observed, fallbackCode = 'observation_failed') {
  const item = { source: 'agent', id: `dsh:${provider.provider}`, status }
  if (status !== 'refreshed') {
    item.code = typeof observed?.reasonCode === 'string' ? observed.reasonCode : fallbackCode
    const message = typeof observed?.message === 'string' && observed.message !== ''
      ? observed.message
      : status === 'blocked'
        ? 'this provider has no supported observation source or credential'
        : 'the observation did not complete'
    item.message = message
  }
  return item
}

/** Human text for a deterministic observation blocker. */
function blockedMessage(code) {
  if (code === 'unsupported_source') return 'this provider has no supported observation source for the requested scope'
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

/** Keep the last good reading and its time when an attempt fails afterwards. */
function mergeFailure(previous, failure) {
  if (previous === undefined) return failure
  return {
    ...previous,
    // The current attempt's evidence replaces the old outcome only; the last
    // reading, its plan, and the time it was observed survive.
    reasonCode: failure.reasonCode,
    message: failure.message,
    rows: previous.rows,
    plan: previous.plan,
    observedAt: previous.observedAt,
    windows: previous.windows,
  }
}

function agentItemFor(provider, observed) {
  if (observed === undefined) {
    return vendorOf(provider) === undefined
      ? itemFor(provider, 'blocked', undefined, 'unsupported_source')
      : itemFor(provider, 'blocked', undefined, 'credential_unavailable')
  }
  const reasonCode = typeof observed.reasonCode === 'string' ? observed.reasonCode : null
  if (reasonCode === null && Array.isArray(observed.rows) && observed.rows.length > 0) {
    return { source: 'agent', id: `dsh:${provider.provider}`, status: 'refreshed' }
  }
  if (reasonCode !== null && BLOCKED_REASON_CODES.has(reasonCode)) return itemFor(provider, 'blocked', observed)
  return itemFor(provider, 'failed', observed)
}

/**
 * Observation state owned by the host.
 * @param {{ctx: object}} input
 */
export function createObservations({ ctx }) {
  /** id (`dsh:<provider>`) -> vendor observation record. */
  const observed = new Map()

  /**
   * The providers this DSH profile configures, from the live settings and the
   * configurable-provider directory. Reads configuration only; never resolves
   * a credential, never contacts a vendor.
   */
  function discover() {
    const settings = ctx.get?.('settings')
    const llm = ctx.llm
    if (settings === undefined || typeof llm?.listConfigurableProviders !== 'function') return undefined
    let descriptors
    try {
      descriptors = settings.describe?.({ redactSecrets: true })
    } catch {
      return undefined
    }
    let directory
    try {
      directory = llm.listConfigurableProviders()
    } catch {
      // A directory read that fails is the same unavailable-settings fact: no
      // provider can be attributed, and the section reports its own error
      // instead of failing the whole state read.
      return undefined
    }
    const byNs = new Map()
    for (const descriptor of descriptors ?? []) byNs.set(String(descriptor.ns), descriptor.value)
    const providers = []
    for (const entry of directory) {
      let node = byNs.get(String(entry.settingsNs))
      for (const key of entry.settingsPath ?? []) {
        node = node !== null && typeof node === 'object' ? Reflect.get(node, key) : undefined
      }
      const profile = node !== null && typeof node === 'object' ? node : {}
      // The directory lists every route the adapter could activate, including
      // vendor presets nobody configured. Only a provider this profile names is
      // the user's business.
      const declared = entry.declared === true
        || typeof profile.apiKeyEnv === 'string'
        || typeof profile.baseURL === 'string'
        || (Array.isArray(profile.models) && profile.models.length > 0)
      if (!declared) continue
      const baseURL = typeof profile.baseURL === 'string' ? profile.baseURL : entry.defaultBaseURL
      providers.push({
        provider: entry.provider,
        displayName: entry.displayName ?? entry.provider,
        baseURL,
        apiKeyEnv: typeof profile.apiKeyEnv === 'string' ? profile.apiKeyEnv : undefined,
        models: Array.isArray(profile.models) ? profile.models.map(model => model?.id).filter(Boolean) : [],
      })
    }
    return { providers }
  }

  /** Cache-only provider records for GET /_tmax/state and /_tmax/dsh-usage. */
  function providerRecords() {
    const discovered = discover()
    if (discovered === undefined) {
      return { available: false, reason: 'agent-settings-unavailable', providers: [] }
    }
    return {
      available: true,
      reason: null,
      providers: discovered.providers.map(provider => stateRecord(provider, observed.get(`dsh:${provider.provider}`))),
    }
  }

  /** Cache-only records shaped for the legacy GET /_tmax/dsh-usage payload. */
  function usageRecords() {
    const result = providerRecords()
    return {
      ok: result.available,
      cached: true,
      generatedAt: new Date().toISOString(),
      providers: result.providers.map(record => ({ name: record.provider, ...record })),
      ...(result.available ? {} : { reason: result.reason }),
    }
  }

  /**
   * Explicitly observe the providers this DSH profile configures.
   * Resolves credential references here and nowhere else; a dry run makes no
   * vendor request and resolves nothing.
   *
   * Reads are bounded: at most `concurrency` vendor requests are in flight and
   * the whole scope shares one `deadlineMs` budget. Both are overridable for
   * tests; the route always uses the shipped defaults.
   * @returns {Promise<{items: object[]}>}
   */
  async function observeAgent({
    dryRun = false,
    deadlineMs = AGENT_OBSERVE_DEADLINE_MS,
    concurrency = AGENT_OBSERVE_CONCURRENCY,
  } = {}) {
    const discovered = discover()
    if (discovered === undefined) {
      // An unavailable settings read is a failed observation, not an empty
      // successful one: nothing could be attributed, so the page must not
      // report "no providers".
      return {
        items: [{
          source: 'agent',
          id: 'dsh:settings',
          status: 'failed',
          code: 'agent_settings_unavailable',
          message: 'the DSH settings service is unavailable, so agent-configured providers cannot be observed',
        }],
      }
    }
    if (dryRun) {
      return {
        items: discovered.providers.map(provider => ({
          source: 'agent',
          id: `dsh:${provider.provider}`,
          status: 'unchanged',
          code: 'dry_run',
          message: 'dry run: no vendor request was made',
        })),
      }
    }
    const credentials = ctx.get?.('credentials')
    const limit = Number.isInteger(concurrency) && concurrency > 0 ? concurrency : AGENT_OBSERVE_CONCURRENCY
    const budget = Number.isInteger(deadlineMs) && deadlineMs > 0 ? deadlineMs : AGENT_OBSERVE_DEADLINE_MS
    // Group providers by the identity whose reading they share, in discovery
    // order. Each unique identity becomes one vendor read however many cards
    // reuse it, and the item order stays the discovered provider order.
    const entries = []
    const tasks = []
    const byIdentity = new Map()
    for (const provider of discovered.providers) {
      const vendor = vendorOf(provider)
      if (vendor === undefined) {
        entries.push({ provider, task: undefined })
        continue
      }
      const identity = `${vendor.id}::${provider.apiKeyEnv ?? provider.provider}`
      let task = byIdentity.get(identity)
      if (task === undefined) {
        task = { provider, vendor, record: undefined }
        byIdentity.set(identity, task)
        tasks.push(task)
      }
      entries.push({ provider, task })
    }
    await observeWithBudget(tasks, {
      limit,
      deadlineMs: budget,
      observe: (task, signal) => observeProvider({
        provider: task.provider.provider,
        baseURL: task.provider.baseURL,
        apiKeyEnv: task.provider.apiKeyEnv,
        signal,
        // Resolution is part of this explicit observation only.
        resolveKey: async ref => (await credentials?.resolve?.(ref))?.value,
      }),
    })
    const items = []
    for (const { provider, task } of entries) {
      const record = task?.record
      if (record !== undefined) {
        const key = `dsh:${provider.provider}`
        const previous = observed.get(key)
        const reasonCode = typeof record.reasonCode === 'string' ? record.reasonCode : null
        if (reasonCode !== null && BLOCKED_REASON_CODES.has(reasonCode)) {
          // A blocked observation is a precondition, not an attempt: it never
          // replaces a cached reading. With no reading yet, keep the reason so
          // the page can name the blocker without inventing a value.
          if (previous === undefined) observed.set(key, record)
        } else if (reasonCode !== null && FAILED_REASON_CODES.has(reasonCode)) {
          observed.set(key, mergeFailure(previous, record))
        } else {
          observed.set(key, record)
        }
      }
      items.push(agentItemFor(provider, record))
    }
    return { items }
  }

  /** Drop cached readings; used by tests and teardown. */
  function clear() {
    observed.clear()
  }

  return { discover, providerRecords, usageRecords, observeAgent, clear }
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
