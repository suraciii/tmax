// The host read model behind GET /_tmax/state.
//
// One plugin-owned composition of existing tmax reads — no new tmax contract,
// no provider request, no credential resolution. The route source is the
// target inventory (`host/inventory.js`), which replaced `tmax dsh projection`;
// the remaining sections come from `provider list`/`provider inspect`,
// `config validate`, and one `model explain` per projected model. The payload
// deliberately mirrors `test/fixtures/state.json`; `unknown` is a value, a
// failed read keeps a typed error, and sections never silently disappear. A
// short cache plus single-flight keeps the page from spawning a tmax process
// storm.
import { readEnvelope, READ_TIMEOUT_MS } from './cli.js'
import { composeInventory, readInventoryFacts } from './inventory.js'
import { liveProviderIds } from './ownership.js'

export const STATE_SCHEMA_VERSION = 1
const CACHE_TTL_MS = 5_000
const PROTOCOLS = new Set(['chat-completions', 'responses', 'messages'])
const AGENT_SETTINGS_UNAVAILABLE =
  'the DSH settings service is unavailable, so agent-configured providers cannot be listed'

function list(value) {
  return Array.isArray(value) ? value : []
}

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {}
}

function text(value) {
  return typeof value === 'string' && value !== '' ? value : null
}

function nullableBoolean(value) {
  return value === true || value === false ? value : null
}

function mapQuotaRow(row) {
  const source = object(row)
  const mapped = { label: source.label ?? null, basis: source.basis ?? null }
  if (source.percent !== undefined) mapped.percent = source.percent
  if (source.counters !== undefined) mapped.counters = source.counters
  if (source.status !== undefined) mapped.status = source.status
  if (source.reset !== undefined) mapped.reset = source.reset
  if (source.expiry !== undefined) mapped.expiry = source.expiry
  return mapped
}

function mapRouteModel(model) {
  const source = object(model)
  const capabilities = object(source.capabilities)
  const mappedCapabilities = {}
  if (Number.isInteger(capabilities.contextWindow)) mappedCapabilities.contextWindow = capabilities.contextWindow
  if (Array.isArray(capabilities.input)) mappedCapabilities.input = [...capabilities.input]
  // `false` is tmax's known "reasoning disabled" value, not a missing field.
  if (capabilities.reasoningEfforts === false) mappedCapabilities.reasoningEfforts = false
  else if (object(capabilities.reasoningEfforts) && Object.keys(object(capabilities.reasoningEfforts)).length > 0) {
    mappedCapabilities.reasoningEfforts = { ...object(capabilities.reasoningEfforts) }
  }
  return {
    id: text(source.id) ?? text(source.display_name),
    name: text(source.display_name) ?? text(source.id),
    // `agent inspect` does not report these; null is unknown, never absent.
    kind: text(source.kind),
    continuity: text(source.continuity),
    candidateCount: Number.isInteger(source.candidate_count) ? source.candidate_count : null,
    availability: text(source.availability),
    capabilities: mappedCapabilities,
  }
}

function mapBilling(data) {
  const observations = object(data.observations)
  const billing = object(observations.billing)
  const display = object(billing.display)
  return {
    kind: text(billing.kind) ?? 'unknown',
    state: text(billing.state) ?? 'unknown',
    freshness: text(object(billing.freshness).state) ?? 'missing',
    dashboard: text(billing.dashboard),
    observedAt: text(object(billing.observation).observed_at),
    notes: list(display.notes).filter(note => typeof note === 'string'),
    rows: list(display.rows).map(mapQuotaRow),
  }
}

function mapSuspension(data) {
  const quota = object(data.quota_suspension)
  return {
    state: text(quota.state) ?? 'unknown',
    reasonCode: text(quota.reason_code),
    resetAt: text(quota.reset_at),
    source: text(quota.source),
    window: text(quota.window),
  }
}

function mapFailure(data) {
  const failure = object(data.failure_state)
  return {
    state: text(failure.state) ?? 'unknown',
    consecutiveFailures: Number.isInteger(failure.consecutive_failures) ? failure.consecutive_failures : null,
    activePriority: failure.active_priority ?? null,
    expiresAt: text(failure.expires_at),
    reasonCode: text(failure.reason_code),
  }
}

/** Quota-attempt evidence carried by `provider list` (scope `quota`). */
function providerAttempt(entry) {
  const observations = object(object(entry).observations)
  const quota = list(observations.observations).find(candidate => object(candidate).scope === 'quota')
  const record = object(quota)
  const reasonCode = text(record.last_attempt_reason_code)
  const outcome = text(record.last_attempt_outcome)
  return {
    ...(reasonCode === null ? {} : { reasonCode }),
    // A completed attempt needs no separate status: the reading itself says so.
    ...(outcome === null || outcome === 'completed' ? {} : { lastAttempt: outcome }),
  }
}

function mapProvider(entry, inspected, fallbackId) {
  const listed = object(entry)
  const attempt = providerAttempt(listed)
  const id = text(listed.id) ?? String(fallbackId)
  const base = {
    id,
    name: text(listed.name) ?? id,
    enabled: nullableBoolean(listed.enabled),
    authentication: text(listed.authentication),
    affectedTargets: Number.isInteger(listed.affected_targets) ? listed.affected_targets : 0,
    accountIdentity: null,
    subscription: null,
    integration: null,
    billing: { kind: 'unknown', state: 'unknown', freshness: 'missing', dashboard: null, observedAt: null, notes: [], rows: [], ...attempt },
    suspension: { state: 'unavailable', reasonCode: null, resetAt: null, source: null, window: null },
    failure: { state: 'unavailable', consecutiveFailures: null, activePriority: null, expiresAt: null, reasonCode: null },
    error: null,
  }
  if (inspected === undefined || !inspected.ok) {
    const code = inspected?.code ?? 'tmax_unavailable'
    return {
      ...base,
      suspension: { ...base.suspension, state: 'unavailable', reasonCode: code },
      failure: { ...base.failure, reasonCode: code },
      error: { code, message: inspected?.message ?? 'provider inspect failed' },
    }
  }
  const data = object(inspected.data)
  const provider = object(data.provider)
  const observations = object(data.observations)
  return {
    ...base,
    name: text(object(provider.overrides).name) ?? text(listed.name) ?? id,
    enabled: nullableBoolean(provider.enabled) ?? base.enabled,
    authentication: text(listed.authentication) ?? text(object(provider.auth).method),
    accountIdentity: text(observations.account_identity),
    subscription: provider.subscription ?? null,
    integration: text(provider.integration),
    billing: { ...mapBilling(data), ...attempt },
    suspension: mapSuspension(data),
    failure: mapFailure(data),
    error: null,
  }
}

function mapSelection(selection) {
  if (selection === null || typeof selection !== 'object') return null
  return {
    qualifiedModel: text(selection.qualified_model),
    provider: text(selection.provider),
    model: text(selection.model),
    priority: Number.isInteger(selection.priority) ? selection.priority : null,
  }
}

/**
 * Build one routing row from an inventory model and its `model explain` read.
 * Candidates keep tmax's evaluation order except that the selected candidate
 * leads; excluded candidates carry their reason code, never a guess.
 */
function mapRoutingRow(model, route, explanation) {
  const source = object(model)
  const row = {
    model: text(source.id) ?? text(source.display_name),
    protocol: text(source.protocol) ?? text(route.protocol),
    kind: text(source.kind),
    continuity: text(source.continuity),
    candidateCount: Number.isInteger(source.candidate_count) ? source.candidate_count : null,
    availability: text(source.availability),
    selection: null,
    candidates: [],
    excluded: [],
    error: null,
  }
  if (explanation === undefined || !explanation.ok) {
    row.error = {
      code: explanation?.code ?? 'model_explain_unavailable',
      message: explanation?.message ?? 'model explain did not complete',
    }
    return row
  }
  const data = object(explanation.data)
  row.selection = mapSelection(data.selection)
  const candidates = list(data.priority_evaluations).map(evaluation => {
    const entry = object(evaluation)
    const priority = object(entry.priority_evaluation)
    return {
      qualifiedModel: text(entry.qualified_model),
      priority: Number.isInteger(priority.effective_priority) ? priority.effective_priority : null,
      selected: row.selection !== null && text(entry.qualified_model) === row.selection.qualifiedModel,
    }
  })
  // Only the selected candidate moves; the remaining rows stay in tmax's
  // evaluation order (sort is stable) rather than a local priority ranking.
  candidates.sort((left, right) => (right.selected ? 1 : 0) - (left.selected ? 1 : 0))
  row.candidates = candidates
  row.excluded = list(data.excluded_candidates).map(entry => ({
    qualifiedModel: text(object(entry).qualified_model),
    reasonCode: text(object(entry).reason_code),
  }))
  return row
}

/** The daemon-identity token the page contract understands. */
export function normalizeDaemonDiscovery(value) {
  if (value === 'compatible' || value === 'identified') return 'identified'
  if (value === 'unidentified' || value === 'absent') return 'unidentified'
  return 'unknown'
}

/** The shared-key load state in the page contract's vocabulary. */
export function normalizeSharedKeyState(value) {
  if (value === 'ready' || value === 'loaded') return 'loaded'
  if (value === 'absent') return 'absent'
  return 'unknown'
}

/** The ownership view exposed by the state; absent decisions read as unknown. */
export function ownershipSection(decision) {
  if (decision === undefined) {
    return { owner: 'unknown', scope: null, reason: null, conflict: null }
  }
  return {
    owner: decision.owner,
    scope: decision.scope ?? null,
    reason: decision.reason ?? null,
    conflict: decision.conflict ?? null,
  }
}

/** Target blockers from the inventory and the agent dossier, deduplicated. */
function targetBlockers(inventory, inspect) {
  const mapped = []
  const seen = new Set()
  for (const row of [...list(inventory.blockers), ...list(inspect.blockers)]) {
    const source = object(row)
    const code = text(source.code)
    if (code === null) continue
    const message = text(source.message)
    const key = `${code}\u0000${message ?? ''}`
    if (seen.has(key)) continue
    seen.add(key)
    mapped.push({ code, message })
  }
  return mapped
}

function targetSection(inventory, inspect, effective) {
  const installation = object(inspect.installation)
  return {
    id: text(object(inventory.target).id) ?? text(inspect.target),
    path: text(object(inventory.target).path) ?? text(inspect.path),
    default: object(inventory.target).default ?? inspect.default ?? null,
    nativeDefault: inspect.native_default ?? null,
    effectiveDefault: effective,
    appliedState: text(inspect.applied_state),
    pendingNativeChanges: inspect.pending_native_changes ?? null,
    blockers: targetBlockers(inventory, inspect),
    // The contract carries the detected installation only; tmax's full
    // agent-inspect dossier (files, recovery, paths) does not cross.
    installation: Object.keys(installation).length === 0
      ? null
      : {
          version: text(installation.version),
          state: text(installation.state),
          source: text(installation.source),
        },
  }
}

function gatewaySection(inventory, statusRead) {
  const projectionGateway = object(inventory.gateway)
  const status = statusRead?.ok === true ? object(statusRead.data) : {}
  const daemon = object(status.daemon)
  const supervision = object(status.supervision)
  const autostart = object(status.autostart)
  const sharedKey = object(status.shared_key)
  return {
    configured: nullableBoolean(projectionGateway.configured),
    ready: nullableBoolean(projectionGateway.ready),
    listen: text(projectionGateway.listen) ?? text(object(status.gateway).listen),
    daemonDiscoveryState: normalizeDaemonDiscovery(
      text(daemon.discovery_state) ?? text(projectionGateway.daemon_discovery_state),
    ),
    reasonCode: text(projectionGateway.reason_code) ?? text(daemon.reason_code),
    supervision: {
      state: text(supervision.state),
      reasonCode: text(supervision.reason_code),
    },
    autostart: {
      savedIntent: nullableBoolean(autostart.saved_intent),
      registration: text(autostart.registration),
      unitMatches: nullableBoolean(autostart.unit_matches),
    },
    sharedKey: {
      loadedState: normalizeSharedKeyState(text(sharedKey.loaded_state)),
      reasonCode: text(sharedKey.reason_code),
    },
  }
}

function configSection(configRead) {
  // A tmax answer is authoritative even when the envelope status is an error:
  // an invalid configuration must stay readable instead of hiding its error.
  if (configRead.envelope === undefined) {
    return {
      state: 'unavailable',
      revision: null,
      errors: [{ code: configRead.code ?? 'unavailable', message: configRead.message ?? 'config validate failed' }],
    }
  }
  const data = object(configRead.data)
  const valid = data.valid === true
  const blockers = Array.isArray(configRead.envelope.blockers) ? configRead.envelope.blockers : []
  const errors = valid
    ? []
    : blockers.map(blocker => ({
      code: text(object(blocker).code) ?? configRead.code ?? 'invalid_config',
      message: text(object(blocker).message) ?? configRead.message ?? 'configuration is invalid',
    }))
  if (!valid && errors.length === 0) {
    errors.push({
      code: configRead.code ?? 'invalid_config',
      message: configRead.message ?? 'configuration is invalid',
    })
  }
  return {
    state: valid ? 'valid' : 'invalid',
    revision: text(data.revision),
    errors,
  }
}

function diagnosticsSection(inventory, statusRead) {
  const daemon = statusRead?.ok === true ? object(statusRead.data?.daemon) : {}
  const compatible = daemon.discovery_state === 'compatible'
  return {
    history: {
      state: compatible ? 'available' : 'unavailable',
      reasonCode: compatible ? null : (text(daemon.reason_code) ?? 'daemon_identity_unavailable'),
    },
    // The inventory's own read time; the field name is the page contract's.
    lastProjectionAt: text(inventory.generated_at),
    lastError: null,
    readModel: 'inventory',
  }
}

/**
 * Create the state service.
 * @param {object} input
 * @param {object} input.config
 * @param {object} input.ctx
 * @param {object} input.observations - observation cache
 * @param {() => object|undefined} input.getOwnership - latest route-ownership decision
 * @param {() => object[]} [input.getRouteBlockers] - declared route-build blockers
 * @param {() => string[]} [input.getLiveNames] - live provider route ids
 */
export function createSnapshotService({ config, ctx, observations, getOwnership, getRouteBlockers, getLiveNames }) {
  let cache = { at: 0, value: undefined }
  let inFlight

  async function credentialConfigured(ref) {
    if (typeof ref !== 'string' || ref === '') return null
    const credentials = ctx.get?.('credentials')
    if (credentials === undefined || typeof credentials.describe !== 'function') return null
    try {
      const info = await credentials.describe(ref)
      if (info?.configured === true) return true
      if (info?.configured === false) return false
      // A store that cannot answer is unknown, not "missing".
      return null
    } catch {
      // A store failure is unknown, not "missing".
      return null
    }
  }

  function effectiveDefault() {
    const service = ctx.get?.('agentDefaultModel')
    if (service === undefined || typeof service.currentSelection !== 'function') return null
    try {
      const selection = service.currentSelection()
      if (selection === null || typeof selection !== 'object' || typeof selection.model !== 'string') return null
      return {
        provider: text(selection.provider),
        model: selection.model,
        reasoningEffort: text(selection.reasoningEffort),
        source: 'agent-default-model',
      }
    } catch {
      return null
    }
  }

  /**
   * The state where the target inventory could not be read: the route source is
   * unknown, but the sections that do not depend on it (providers, config) are
   * still composed and their own failures named. Empty never stands in for
   * unknown.
   */
  function inventoryFailureState(error, { startedAt, providerRows, listedRead, configRead, agentRecords }) {
    const code = typeof error?.code === 'string' && error.code !== '' ? error.code : 'inventory_unavailable'
    const message = typeof error?.message === 'string' && error.message !== ''
      ? error.message
      : 'tmax did not report the DSH target inventory'
    const errors = [{ section: 'projection', code, message }]
    if (listedRead.envelope === undefined) errors.push({ section: 'providers', code: listedRead.code, message: listedRead.message ?? listedRead.code })
    else if (!listedRead.ok) errors.push({ section: 'providers', code: listedRead.code, message: listedRead.message ?? listedRead.code })
    if (configRead.envelope === undefined) errors.push({ section: 'config', code: configRead.code, message: configRead.message ?? configRead.code })
    else if (!configRead.ok) errors.push({ section: 'config', code: configRead.code, message: configRead.message ?? configRead.code })
    if (!agentRecords.available) {
      errors.push({ section: 'providers', code: 'agent_settings_unavailable', message: AGENT_SETTINGS_UNAVAILABLE })
    }
    // Availability is "tmax answered something": a failed target read must not
    // hide provider or configuration evidence that did answer.
    const ok = listedRead.envelope !== undefined || configRead.envelope !== undefined
    return {
      ok,
      errors,
      schemaVersion: STATE_SCHEMA_VERSION,
      generatedAt: startedAt,
      configurationRevision: null,
      tmaxVersion: null,
      authority: null,
      target: null,
      ownership: ownershipSection(getOwnership?.()),
      gateway: null,
      config: configSection(configRead),
      routes: [],
      providers: { tmax: providerRows, agent: agentRecords.providers },
      routing: [],
      diagnostics: {
        history: { state: 'unavailable', reasonCode: code },
        lastProjectionAt: null,
        lastError: message,
        readModel: 'inventory',
      },
    }
  }

  /** Compose one state payload from the inventory plus the remaining tmax reads. */
  async function compose() {
    const startedAt = new Date().toISOString()
    const inventoryRead = await readInventoryFacts(config)
      .then(data => ({ ok: true, data }))
      .catch(error => ({ ok: false, error }))
    const [listedRead, configRead] = await Promise.all([
      readEnvelope(config, ['provider', 'list']),
      readEnvelope(config, ['config', 'validate']),
    ])
    const listed = listedRead.ok ? list(object(listedRead.data).providers) : []
    const inspected = await Promise.all(listed.map(entry => {
      const id = text(object(entry).id)
      if (id === null) {
        return Promise.resolve({
          ok: false,
          code: 'invalid_provider_id',
          message: 'tmax provider list returned a provider without an id',
        })
      }
      return readEnvelope(config, ['provider', 'inspect', id], { timeoutMs: READ_TIMEOUT_MS })
    }))
    const providerRows = listed.map((entry, index) => mapProvider(entry, inspected[index], object(entry).id))
    const agentRecords = observations.providerRecords()
    if (!inventoryRead.ok) {
      return inventoryFailureState(inventoryRead.error, { startedAt, providerRows, listedRead, configRead, agentRecords })
    }
    const inventory = composeInventory(inventoryRead.data)
    const facts = inventoryRead.data

    const projectedModels = []
    for (const route of list(inventory.routes)) {
      for (const model of list(route.models)) projectedModels.push({ route, model })
    }
    const explained = await Promise.all(projectedModels.map(({ route, model }) => {
      const id = text(object(model).id)
      const protocol = text(object(model).protocol) ?? text(route.protocol)
      if (id === null || protocol === null || !PROTOCOLS.has(protocol)) {
        return Promise.resolve({
          ok: false,
          code: 'model_explain_unavailable',
          message: id === null ? 'model has no id' : 'model protocol is not a supported protocol',
        })
      }
      return readEnvelope(config, ['model', 'explain', id, '--protocol', protocol])
    }))
    const routing = projectedModels.map(({ route, model }, index) => mapRoutingRow(model, route, explained[index]))

    const decision = getOwnership?.()
    const scope = decision?.scope ?? null
    const declaredRouteBlockers = typeof getRouteBlockers === 'function' ? list(getRouteBlockers()) : []
    const live = new Set(typeof getLiveNames === 'function' ? list(getLiveNames()) : liveProviderIds(ctx))
    const routes = await Promise.all(list(inventory.routes).map(async route => {
      const source = object(route)
      const credentialRef = text(source.credential_ref)
      const name = text(source.name)
      // The route name is the provider name as the settings and the registry spell it.
      const provider = name
      return {
        name,
        provider,
        protocol: text(source.protocol),
        baseUrl: text(source.base_url) ?? '',
        credentialRef,
        credentialConfigured: await credentialConfigured(credentialRef),
        scope,
        models: list(source.models).map(mapRouteModel),
        active: provider !== null && live.has(provider),
        blockers: declaredRouteBlockers
          .filter(blocker => blocker?.route === name)
          .map(blocker => ({
            code: text(blocker.code),
            message: text(blocker.message),
            ...(Array.isArray(blocker.models) && blocker.models.length > 0 ? { models: [...blocker.models] } : {}),
          })),
      }
    }))

    // Every failed core read is named in one place; empty sections never stand
    // in for a failure the client cannot see.
    const errors = []
    const addError = (section, code, message) => {
      const resolvedCode = typeof code === 'string' && code !== '' ? code : 'unavailable'
      errors.push({
        section,
        code: resolvedCode,
        message: typeof message === 'string' && message !== '' ? message : resolvedCode,
      })
    }
    if (listedRead.envelope === undefined) addError('providers', listedRead.code, listedRead.message)
    else if (!listedRead.ok) addError('providers', listedRead.code, listedRead.message)
    if (configRead.envelope === undefined) addError('config', configRead.code, configRead.message)
    else if (!configRead.ok) addError('config', configRead.code, configRead.message)
    if (facts.gateway.envelope === undefined) addError('gateway', facts.gateway.code, facts.gateway.message)
    else if (!facts.gateway.ok) addError('gateway', facts.gateway.code, facts.gateway.message)
    if (facts.status.envelope === undefined) addError('authority', facts.status.code, facts.status.message)
    else if (!facts.status.ok) addError('authority', facts.status.code, facts.status.message)
    if (!agentRecords.available) addError('providers', 'agent_settings_unavailable', AGENT_SETTINGS_UNAVAILABLE)

    return {
      ok: true,
      errors,
      schemaVersion: STATE_SCHEMA_VERSION,
      generatedAt: startedAt,
      configurationRevision: text(inventory.configuration_revision),
      tmaxVersion: text(inventory.tmax_version),
      authority: text(inventory.authority),
      target: targetSection(inventory, facts.inspect, effectiveDefault()),
      ownership: ownershipSection(decision),
      gateway: gatewaySection(inventory, facts.gateway),
      config: configSection(configRead),
      routes,
      providers: {
        tmax: providerRows,
        agent: agentRecords.providers,
      },
      routing,
      diagnostics: diagnosticsSection(inventory, facts.gateway),
    }
  }

  /**
   * Read the state, serving the short cache unless `force` is set.
   * Concurrent callers share one composition (single-flight).
   */
  async function state({ force = false } = {}) {
    if (!force && cache.value !== undefined && Date.now() - cache.at < CACHE_TTL_MS) return cache.value
    if (inFlight !== undefined) {
      if (!force) return inFlight
      // A forced read after an explicit action must observe the new state, so
      // it waits for the stale in-flight composition instead of reusing it.
      await inFlight.catch(() => {})
      if (!force && cache.value !== undefined && Date.now() - cache.at < CACHE_TTL_MS) return cache.value
    }
    inFlight = compose()
      .then(value => {
        cache = { at: Date.now(), value }
        return value
      })
      .finally(() => {
        inFlight = undefined
      })
    return inFlight
  }

  /** Drop the cache so the next read reflects a completed explicit operation. */
  function invalidate() {
    cache = { at: 0, value: undefined }
  }

  return { state, invalidate }
}
