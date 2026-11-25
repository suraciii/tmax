// The allowlisted host action runner behind POST /_tmax/action.
//
// The client may name one action and one operand set; it never names a command,
// a flag, or a path. This module owns the exact argv for every action, the
// closed scope enum, the per-action timeout, and the single-flight key. A
// provider ID is accepted only when it is exactly one ID from the current host
// state, and a target is accepted only when it is exactly one declared Harness
// in `state.harnesses`, so an arbitrary client string can never reach argv.
// Connect is the legacy native application `tmax agent apply TARGET`; the
// legacy `tmax dsh connect` form no longer exists.
//
// A tmax answer is data: a typed envelope that is not a completed operation is
// a 409 (and its code/status/blockers/next_actions/effects are preserved), while
// no envelope at all is a 502 transport result. Every outcome carries fresh host
// state and passes through the same secret-freeing projection as the legacy
// routes.
import {
  CONNECT_TIMEOUT_MS,
  envelopeMessage,
  explicitTarget,
  readEnvelope,
  REFRESH_TIMEOUT_MS,
} from './cli.js'

/** Every action the page may request, in contract order. */
export const ACTIONS = Object.freeze([
  'connect',
  'observe_provider',
  'resume_quota',
  'reset_failure',
  'open_management',
  'gateway_start',
  'gateway_stop',
  'gateway_restart',
  'add_model',
  'remove_model',
  'apply_target',
])

/** Actions that change one target's desired provider/model selection. */
export const MODEL_ACTIONS = Object.freeze(['add_model', 'remove_model'])

/** Actions that name one tmax provider and therefore need an exact snapshot ID. */
export const PROVIDER_ACTIONS = Object.freeze(['observe_provider', 'resume_quota', 'reset_failure'])

/** Gateway lifecycle actions; each verb needs a configured gateway. */
export const GATEWAY_ACTIONS = Object.freeze(['gateway_start', 'gateway_stop', 'gateway_restart'])

const GATEWAY_VERBS = Object.freeze({
  gateway_start: 'start',
  gateway_stop: 'stop',
  gateway_restart: 'restart',
})

/** `provider refresh` scopes. The only operand a client may widen is a closed set. */
export const OBSERVE_SCOPES = Object.freeze(['quota', 'auth', 'models', 'all'])

export const DEFAULT_OBSERVE_SCOPE = 'quota'

/** Protocols a target selection may name; closed before any argv is built. */
export const ADD_PROTOCOLS = Object.freeze(['chat-completions', 'responses', 'messages'])

/**
 * Per-action deadlines. A persisted provider repair can wait on storage; a
 * single-scope observation uses tmax's own 45 s deadline, while a full
 * observation runs auth, models, and quota checks and gets a longer budget.
 * `web status` is a read and stays short; `web open` starts a listener and gets
 * more room. Gateway lifecycle can wait on supervision (unit registration and
 * process startup), so it gets the longest deadline. A desired-configuration
 * write can run a provider creation plus a model selection, so it keeps the
 * longer storage budget; applying one target writes native files through
 * tmax's own recovery and lock sequence and gets the apply deadline.
 */
export const ACTION_TIMEOUTS = Object.freeze({
  connect: CONNECT_TIMEOUT_MS,
  observe_provider: REFRESH_TIMEOUT_MS,
  observeAll: 120_000,
  resume_quota: 45_000,
  reset_failure: 45_000,
  webStatus: 20_000,
  webOpen: 60_000,
  gateway: 90_000,
  add_model: 120_000,
  remove_model: 120_000,
  apply_target: CONNECT_TIMEOUT_MS,
})

/** The deadline for one planned action; scope, phase, and family select it. */
export function actionTimeoutMs(action, { dryRun = false, scope = null } = {}) {
  if (action === 'observe_provider') {
    return scope === 'all' ? ACTION_TIMEOUTS.observeAll : ACTION_TIMEOUTS.observe_provider
  }
  if (action === 'open_management') {
    return dryRun === true ? ACTION_TIMEOUTS.webStatus : ACTION_TIMEOUTS.webOpen
  }
  if (GATEWAY_ACTIONS.includes(action)) return ACTION_TIMEOUTS.gateway
  return ACTION_TIMEOUTS[action] ?? ACTION_TIMEOUTS.connect
}

/**
 * Canonical single-flight key shared by the legacy routes and the action
 * service: both connect forms lock `connect`, every observation locks
 * `observe`, each persisted repair locks its own provider, and each gateway
 * verb locks itself. Desired-configuration writes share the target-write lock
 * with connect because they all change what an apply will write; applying one
 * Harness locks that target alone, so different Harnesses can apply at once
 * while the same one cannot.
 */
export function actionLockKey(action, provider = null, target = null) {
  if (action === 'connect' || action === 'add_model' || action === 'remove_model') return 'connect'
  if (action === 'observe_provider') return 'observe'
  if (action === 'resume_quota' || action === 'reset_failure') {
    return typeof provider === 'string' && provider !== '' ? `${action}:${provider}` : action
  }
  if (action === 'open_management') return 'open_management'
  if (GATEWAY_ACTIONS.includes(action)) return `gateway:${GATEWAY_VERBS[action]}`
  if (action === 'apply_target') {
    return typeof target === 'string' && target !== '' ? `apply:${target}` : 'apply'
  }
  return null
}

/** Strip any value that looks like key material from an action result. */
export function secretFree(value, depth = 0) {
  if (depth > 8 || value === null || typeof value !== 'object') return value
  if (Array.isArray(value)) return value.map(entry => secretFree(entry, depth + 1))
  const forbidden = new Set(['key', 'value', 'token', 'secret', 'password', 'authorization', 'apikey', 'api_key'])
  const result = {}
  for (const [name, entry] of Object.entries(value)) {
    if (forbidden.has(name.toLowerCase())) continue
    result[name] = secretFree(entry, depth + 1)
  }
  return result
}

/** Exact provider IDs from the current host snapshot; nothing else is addressable. */
export function providerIdsFromState(state) {
  const rows = state?.providers?.tmax
  if (!Array.isArray(rows)) return []
  return rows.map(row => row?.id).filter(id => typeof id === 'string' && id !== '')
}

function invalid(status, code, message) {
  return { ok: false, status, code, message }
}

function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function textOf(value) {
  return typeof value === 'string' && value !== '' ? value : null
}

function listOf(value) {
  return Array.isArray(value) ? value : []
}

/**
 * Validate everything about one request that does not depend on host state.
 * A malformed request is a 400; nothing here reads the CLI or the snapshot.
 * @returns {{ok: true, request: {action: string, provider: string|null, scope: string|null, dryRun: boolean}}
 *   | {ok: false, status: number, code: string, message: string}}
 */
export function validateActionRequest(body) {
  if (!isPlainObject(body)) return invalid(400, 'invalid_body', 'the request body must be a JSON object')
  const { action } = body
  if (typeof action !== 'string' || !ACTIONS.includes(action)) {
    return invalid(400, 'invalid_action', `action must be one of ${ACTIONS.join(', ')}`)
  }
  const dryRun = body.dryRun ?? false
  if (typeof dryRun !== 'boolean') return invalid(400, 'invalid_dry_run', 'dryRun must be a boolean')
  let scope = null
  if (body.scope !== undefined) {
    if (typeof body.scope !== 'string' || !OBSERVE_SCOPES.includes(body.scope)) {
      return invalid(400, 'invalid_scope', `scope must be one of ${OBSERVE_SCOPES.join(', ')}`)
    }
    scope = body.scope
  }
  if (action === 'observe_provider' && scope === null) scope = DEFAULT_OBSERVE_SCOPE
  // A target-naming action is a declared Harness; the browser never names an
  // arbitrary target. The operand is validated against state.harnesses by the
  // planner; only its syntax is closed here.
  if (MODEL_ACTIONS.includes(action) || action === 'apply_target') {
    const target = body.target
    if (typeof target !== 'string' || target === '') {
      return invalid(400, 'invalid_target', `${action} requires a target`)
    }
    if (action === 'apply_target') {
      return { ok: true, request: { action, target, provider: null, scope: null, dryRun } }
    }
    const provider = body.provider
    const model = body.model
    const protocol = body.protocol
    if (typeof provider !== 'string' || provider === '') return invalid(400, 'invalid_provider', `${action} requires a provider ID`)
    if (typeof model !== 'string' || model === '') return invalid(400, 'invalid_model', `${action} requires a model ID`)
    if (typeof protocol !== 'string' || !ADD_PROTOCOLS.includes(protocol)) {
      return invalid(400, 'invalid_protocol', `protocol must be one of ${ADD_PROTOCOLS.join(', ')}`)
    }
    return { ok: true, request: { action, target, provider, model, protocol, scope: null, dryRun } }
  }
  if (PROVIDER_ACTIONS.includes(action)) {
    const provider = body.provider
    if (provider === undefined || provider === null) return invalid(400, 'missing_provider', `${action} requires a provider`)
    if (typeof provider !== 'string' || provider === '') {
      return invalid(400, 'invalid_provider', 'provider must be a non-empty provider ID')
    }
    return { ok: true, request: { action, provider, scope, dryRun } }
  }
  // A provider or target for an action that names none is ignored, never argv.
  return { ok: true, request: { action, provider: null, scope, dryRun } }
}

/** Exact declared-Harness IDs from the current host snapshot; nothing else is addressable. */
export function harnessIdsFromState(state) {
  const rows = state?.harnesses
  if (!Array.isArray(rows)) return []
  return rows.map(row => row?.id).filter(id => typeof id === 'string' && id !== '')
}

/**
 * The declared Harness for one target, or the typed refusal that prevents
 * addressing it. A target is addressable only when the Harness list was read
 * and that Harness's own inspection answered, because a selection write reads
 * and extends the target's existing entries.
 * @returns {{ok: true, harness: object} | {ok: false, status: number, code: string, message: string}}
 */
export function harnessFromState(state, target) {
  const rows = Array.isArray(state?.harnesses) ? state.harnesses : null
  if (rows === null) {
    return invalid(409, 'harnesses_unavailable', 'the declared tmax Harnesses could not be read, so the target cannot be validated')
  }
  const failure = listOf(state?.errors).find(error => isPlainObject(error) && error.section === 'harnesses')
  if (failure !== undefined) {
    return invalid(
      409,
      'harnesses_unavailable',
      textOf(failure.message) ?? 'the declared tmax Harnesses could not be read, so the target cannot be validated',
    )
  }
  const harness = rows.find(row => isPlainObject(row) && row.id === target)
  if (harness === undefined) {
    return invalid(409, 'unknown_target', `target "${target}" is not a declared tmax Harness`)
  }
  if (harness.appliedState === null) {
    // The list named the target but its inspection did not answer: its entries
    // are unknown, so a selection could not be placed safely.
    return invalid(409, 'target_uninspectable', `target "${target}" cannot be inspected, so its provider entries cannot be read safely`)
  }
  return { ok: true, harness }
}

/** A target-local provider entry name a write may create, or a typed refusal. */
function entryName(provider, protocol, names) {
  const base = provider.source === 'gateway' ? 'tmax' : String(provider.id)
  if (/[\s\u0000-\u001f]/u.test(base)) {
    return invalid(409, 'entry_name_unavailable', 'the provider ID cannot form a native entry name')
  }
  let name = base
  if (names.has(name)) name = `${base}-${protocol}`
  for (let suffix = 2; names.has(name) && suffix < 10; suffix += 1) name = `${base}-${protocol}-${suffix}`
  if (names.has(name)) return invalid(409, 'entry_name_conflict', 'no free target entry name is available for this provider')
  return { ok: true, name }
}

/**
 * Plan one target-selection add: add one existing catalog model to one declared
 * Harness's desired configuration. The browser never names an entry, a command,
 * or a flag; the target, provider, model, and protocol are validated against
 * the current read model and every following operand is derived from it. Pure.
 *
 * The plan creates the target-local provider entry only when the target does
 * not already hold an entry able to serve the protocol, then adds the model
 * selection. It never plans an apply: actual configuration is applied only
 * through the explicit, per-target `apply_target` action.
 *
 * @returns {{ok: true, plan: {provider: string, model: string, protocol: string, target: string, entry: {name: string, mode: 'existing'|'create'}, steps: {name: string, argv: string[]}[]}}
 *   | {ok: false, status: number, code: string, message: string}}
 */
export function planAddModel(request, { state } = {}) {
  if (!isPlainObject(request) || request.action !== 'add_model') return invalid(400, 'invalid_action', 'an add_model request is required')
  const providerId = textOf(request.provider)
  const modelId = textOf(request.model)
  const protocol = textOf(request.protocol)
  const target = textOf(request.target)
  if (target === null) return invalid(400, 'invalid_target', 'add_model requires a target')
  if (providerId === null) return invalid(400, 'invalid_provider', 'add_model requires a provider ID')
  if (modelId === null) return invalid(400, 'invalid_model', 'add_model requires a model ID')
  if (protocol === null || !ADD_PROTOCOLS.includes(protocol)) return invalid(400, 'invalid_protocol', `protocol must be one of ${ADD_PROTOCOLS.join(', ')}`)
  const addressed = harnessFromState(state, target)
  if (!addressed.ok) return addressed
  const harness = addressed.harness
  const catalog = isPlainObject(state?.catalog) ? state.catalog : null
  if (catalog === null || catalog.state !== 'available') {
    return invalid(409, 'catalog_unavailable', 'the tmax model catalog is unavailable, so the selection cannot be validated')
  }
  const provider = listOf(catalog.providers).find(row => isPlainObject(row) && row.id === providerId)
  if (provider === undefined) return invalid(409, 'unknown_provider', `provider "${providerId}" is not in the current tmax catalog`)
  if (provider.enabled !== true) return invalid(409, 'provider_disabled', `provider "${providerId}" is not enabled`)
  const model = listOf(provider.models).find(row => isPlainObject(row) && row.id === modelId)
  if (model === undefined) return invalid(409, 'unknown_model', `model "${providerId}/${modelId}" is not in the current tmax catalog`)
  if (model.enabled !== true) return invalid(409, 'model_disabled', `model "${providerId}/${modelId}" is not enabled`)
  const protocols = listOf(model.protocols)
  if (!protocols.includes(protocol)) {
    return invalid(409, 'unsupported_protocol', `model "${providerId}/${modelId}" does not publish protocol "${protocol}"`)
  }
  if (provider.source === 'gateway' && state?.gateway?.configured !== true) {
    return invalid(409, 'gateway_not_configured', 'the tmax gateway is not configured, so its route cannot be added to the target')
  }
  // The target's own inspected entries are the only selection evidence: the
  // catalog describes models, not where this Harness already holds them.
  const targetEntries = listOf(harness.providers).filter(entry => isPlainObject(entry) && textOf(entry.name) !== null)
  const entries = targetEntries.filter(entry => entry.sourceProvider === providerId)
  const selected = entries.some(entry => listOf(entry.models)
    .some(selection => isPlainObject(selection) && selection.id === modelId && selection.protocol === protocol))
  if (selected) {
    return invalid(409, 'already_joined', `model "${providerId}/${modelId}" is already selected for protocol "${protocol}" on target "${target}"`)
  }
  // Prefer an entry that already serves this protocol; otherwise fill an
  // existing entry that has no model yet before creating another name.
  const existing = entries.find(entry => listOf(entry.models)
    .some(selection => isPlainObject(selection) && selection.protocol === protocol))
    ?? entries.find(entry => listOf(entry.models).length === 0)
  let entry
  if (existing !== undefined) {
    entry = { name: textOf(existing.name), mode: 'existing' }
  } else {
    // A DSH entry holds one protocol. Reuse a matching entry; otherwise create
    // a target-local name that no declared entry of this target already uses.
    const names = new Set(targetEntries.map(candidate => textOf(candidate.name)).filter(name => name !== null))
    const named = entryName(provider, protocol, names)
    if (!named.ok) return named
    entry = { name: named.name, mode: 'create' }
  }
  const steps = []
  if (entry.mode === 'create') {
    steps.push({ name: 'provider_add', argv: ['agent', 'provider', 'add', target, entry.name, '--provider', String(provider.id)] })
  }
  steps.push({ name: 'model_add', protocol, argv: ['agent', 'model', 'add', target, entry.name, String(model.id), '--protocol', protocol] })
  return {
    ok: true,
    plan: {
      provider: String(provider.id),
      model: String(model.id),
      protocol,
      target,
      entry,
      steps,
    },
  }
}

/**
 * Plan one target-selection remove: find the declared Harness entry that holds
 * the exact provider/model/protocol selection and remove every protocol
 * selection for that model from that target's desired configuration as one
 * host-fixed sequence. Pure; never plans an apply.
 *
 * @returns {{ok: true, plan: {provider: string, model: string, protocol: string, target: string, entry: {name: string, mode: 'existing'}, steps: {name: string, argv: string[]}[]}}
 *   | {ok: false, status: number, code: string, message: string}}
 */
export function planRemoveModel(request, { state } = {}) {
  if (!isPlainObject(request) || request.action !== 'remove_model') return invalid(400, 'invalid_action', 'a remove_model request is required')
  const providerId = textOf(request.provider)
  const modelId = textOf(request.model)
  const protocol = textOf(request.protocol)
  const target = textOf(request.target)
  if (target === null) return invalid(400, 'invalid_target', 'remove_model requires a target')
  if (providerId === null) return invalid(400, 'invalid_provider', 'remove_model requires a provider ID')
  if (modelId === null) return invalid(400, 'invalid_model', 'remove_model requires a model ID')
  if (protocol === null || !ADD_PROTOCOLS.includes(protocol)) return invalid(400, 'invalid_protocol', `protocol must be one of ${ADD_PROTOCOLS.join(', ')}`)
  const addressed = harnessFromState(state, target)
  if (!addressed.ok) return addressed
  // The requested protocol is a selector, not the removal limit: one checkbox
  // represents this provider/model in the Harness, so removing it removes every
  // protocol selection for that model. The host derives the exact entry and
  // protocol list from the Harness, not from browser-named argv.
  const entries = listOf(addressed.harness.providers).filter(entry =>
    isPlainObject(entry)
    && entry.sourceProvider === providerId
    && textOf(entry.name) !== null)
  const holders = entries.filter(entry => listOf(entry.models).some(selection =>
    isPlainObject(selection) && selection.id === modelId))
  if (holders.length > 1) {
    // The browser names no entry, so choosing one would be arbitrary. The user
    // must remove the duplicate selection through the CLI.
    return invalid(409, 'ambiguous_selection', `model "${providerId}/${modelId}" is selected in ${holders.length} entries on target "${target}"; remove it by CLI`)
  }
  const steps = []
  const seen = new Set()
  for (const entry of entries) {
    for (const selection of listOf(entry.models)) {
      if (!isPlainObject(selection) || selection.id !== modelId) continue
      const held = textOf(selection.protocol)
      if (held === null) continue
      const key = `${textOf(entry.name)}\u0000${held}`
      if (seen.has(key)) {
        return invalid(409, 'ambiguous_selection', `model "${providerId}/${modelId}" is selected more than once on target "${target}"; remove it by CLI`)
      }
      seen.add(key)
      steps.push({
        name: 'model_remove',
        protocol: held,
        argv: ['agent', 'model', 'remove', target, String(textOf(entry.name)), modelId, '--protocol', held],
      })
    }
  }
  if (steps.length === 0) {
    return invalid(409, 'not_joined', `model "${providerId}/${modelId}" is not selected on target "${target}"`)
  }
  if (!steps.some(step => step.argv[step.argv.length - 1] === protocol)) {
    return invalid(409, 'not_joined', `model "${providerId}/${modelId}" is not selected for protocol "${protocol}" on target "${target}"`)
  }
  const first = entries.find(entry => listOf(entry.models).some(selection =>
    isPlainObject(selection) && selection.id === modelId))
  const entry = { name: textOf(first?.name), mode: 'existing' }
  return {
    ok: true,
    plan: {
      provider: providerId,
      model: modelId,
      protocol,
      target,
      entry,
      steps,
    },
  }
}

function withPreview(argv, dryRun) {
  if (dryRun === true) argv.push('--dry-run')
  return argv
}

/**
 * The exact argv for one action. Provider operands are only used when they are
 * non-empty strings (validation has already pinned them to the snapshot); an
 * unknown action has no argv form and returns null.
 */
export function actionArgv(action, { provider = null, scope = null, dryRun = false, config = {}, env = process.env, target = undefined } = {}) {
  const providerOperand = typeof provider === 'string' && provider !== '' ? provider : undefined
  if (action === 'connect') {
    // `agent apply` names its target as an operand; it accepts no `--home`
    // selector, so an unresolved target has no argv form at all.
    const resolved = typeof target === 'string' && target !== '' ? target : explicitTarget(config)
    if (resolved === undefined) return null
    return withPreview(['agent', 'apply', resolved], dryRun)
  }
  if (action === 'apply_target') {
    // The target is an explicit operand the snapshot validated; a missing one
    // has no argv form rather than falling back to the configured target.
    const named = typeof target === 'string' && target !== '' ? target : undefined
    if (named === undefined) return null
    return withPreview(['agent', 'apply', named], dryRun)
  }
  if (action === 'observe_provider') {
    if (providerOperand === undefined) return null
    if (scope !== null && !OBSERVE_SCOPES.includes(scope)) return null
    return withPreview(['provider', 'refresh', providerOperand, '--scope', scope ?? DEFAULT_OBSERVE_SCOPE], dryRun)
  }
  if (action === 'resume_quota') {
    if (providerOperand === undefined) return null
    return withPreview(['provider', 'quota-resume', providerOperand], dryRun)
  }
  if (action === 'reset_failure') {
    if (providerOperand === undefined) return null
    return withPreview(['provider', 'failure-reset', providerOperand], dryRun)
  }
  if (action === 'open_management') {
    // Preview reports the current listener; only execute starts one.
    return dryRun === true ? ['web', 'status'] : ['web', 'open', '--no-browser']
  }
  const gatewayVerb = GATEWAY_VERBS[action]
  if (gatewayVerb !== undefined) {
    // Preview reports the planned lifecycle without starting or stopping anything.
    return withPreview(['gateway', gatewayVerb], dryRun)
  }
  return null
}

/**
 * Validate one request against the addressable provider IDs and declared
 * Harness IDs and build its exact argv, timeout, and single-flight key. Pure:
 * no CLI, no clock, no mutation.
 * @returns {{ok: true, plan: {action: string, provider: string|null, scope: string|null, dryRun: boolean, argv: string[], timeoutMs: number}}
 *   | {ok: false, status: number, code: string, message: string}}
 */
export function planAction(body, { providerIds = [], targets, config = {}, env = process.env, target = undefined } = {}) {
  const validated = validateActionRequest(body)
  if (!validated.ok) return validated
  const request = validated.request
  if (request.provider !== null) {
    const known = Array.isArray(providerIds) ? providerIds : []
    if (!known.includes(request.provider)) {
      return invalid(409, 'unknown_provider', `provider "${request.provider}" is not in the current tmax state`)
    }
  }
  if (request.action === 'apply_target') {
    // The target is addressable only when the declared Harness list was read;
    // an empty list from a failed read is unknown, never "no such target".
    const known = Array.isArray(targets) ? targets : null
    if (known === null) {
      return invalid(409, 'harnesses_unavailable', 'the declared tmax Harnesses could not be read, so the target cannot be validated')
    }
    if (!known.includes(request.target)) {
      return invalid(409, 'unknown_target', `target "${request.target}" is not a declared tmax Harness`)
    }
  }
  const argv = actionArgv(request.action, { ...request, config, env, target: request.target ?? target })
  if (argv === null) {
    if (request.action === 'connect') {
      return invalid(409, 'target_unresolved', 'no declared DSH target resolves for this home; declare one before applying it')
    }
    if (request.action === 'apply_target') {
      return invalid(409, 'target_unresolved', 'the target to apply is unresolved')
    }
    return invalid(400, 'invalid_action', `${request.action} has no host argv form`)
  }
  return {
    ok: true,
    plan: {
      ...request,
      argv,
      timeoutMs: actionTimeoutMs(request.action, { dryRun: request.dryRun, scope: request.scope }),
    },
  }
}

/** Whether one tmax envelope reports a completed operation or completed preview. */
export function isActionSuccess(envelope, dryRun = false) {
  if (!isPlainObject(envelope)) return false
  // A completed operation may report a nested code (`awaiting_native_reload`,
  // `daemon_reused`); its status, not its code, is the outcome contract.
  if (envelope.status === 'success') return true
  return dryRun === true && envelope.status === 'dry_run'
}

/**
 * Project a tmax envelope onto the action result contract. Only the declared
 * keys cross; the operation/schema_version wrapper and any key material stay
 * behind.
 */
export function resultFromEnvelope(envelope) {
  const source = isPlainObject(envelope) ? envelope : {}
  const code = typeof source.code === 'string' && source.code !== '' ? source.code : 'unknown'
  const status = typeof source.status === 'string' && source.status !== '' ? source.status : 'unknown'
  const result = { code, status }
  if (typeof source.message === 'string' && source.message !== '') result.message = source.message
  if (source.data !== undefined) result.data = source.data
  if (Array.isArray(source.blockers)) result.blockers = source.blockers
  const nextActions = Array.isArray(source.next_actions)
    ? source.next_actions
    : (Array.isArray(source.data?.next_actions) ? source.data.next_actions : undefined)
  if (nextActions !== undefined) result.next_actions = nextActions
  const effects = source.effects !== undefined ? source.effects : source.data?.effects
  if (effects !== undefined) result.effects = effects
  return secretFree(result)
}

/**
 * The action service: validation, single-flight, one bounded tmax call, and a
 * fresh state read after every outcome.
 *
 * @param {object} input
 * @param {object} input.config
 * @param {object} input.snapshot - state service with `state()`/`invalidate()`
 * @param {Set<string>} [input.locks] - single-flight set shared with the legacy routes
 * @param {(message: string) => void} [input.log]
 * @param {object} [input.env]
 * @param {() => Promise<object|undefined>} [input.refreshOwnership] - re-read the
 *   home integration and ownership decision after a completed operation
 * @returns {{run: (body: object) => Promise<{status: number, body: object}>}}
 */
export function createActionService({ config = {}, snapshot, locks = new Set(), log, env = process.env, refreshOwnership } = {}) {
  // The route table shares this set, so an action and its legacy route form are
  // the same operation: one connect, one observation, one repair per provider.
  const running = locks

  async function readCurrentState() {
    try {
      return await snapshot.state()
    } catch (error) {
      log?.(`action state read failed: ${error instanceof Error ? error.message : String(error)}`)
      return undefined
    }
  }

  async function freshState(fallback) {
    try {
      snapshot.invalidate()
      return await snapshot.state({ force: true })
    } catch (error) {
      log?.(`action state refresh failed: ${error instanceof Error ? error.message : String(error)}`)
      return fallback ?? null
    }
  }

  function reply(status, body) {
    return { status, body }
  }

  /** The browser-visible plan: no argv, no paths, no secret material. */
  function writePlanResult(plan) {
    return {
      target: plan.target,
      provider: plan.provider,
      model: plan.model,
      protocol: plan.protocol,
      entry: plan.entry,
      steps: plan.steps.map(step => step.name),
      protocols: plan.steps.map(step => step.protocol ?? null),
    }
  }

  /** Merge plan/step metadata into one tmax step's data without dropping its evidence. */
  function writeResultData(base, steps, extra, evidence) {
    const data = isPlainObject(evidence?.data) ? { ...evidence.data } : {}
    return { ...data, ...base, steps, ...extra }
  }

  /**
   * One target-selection write on one declared Harness. A preview is the
   * read-model-derived plan (no tmax invocation, no write); execution runs the
   * fixed selection sequence — provider creation only when the target needs an
   * entry, then the model addition or removal — and never an apply. It stops
   * at the first refusal and reports every completed step and the fresh state.
   * The target's actual configuration changes only through `apply_target`.
   */
  async function runModelWrite(body) {
    const validated = validateActionRequest(body)
    const action = validated.ok ? validated.request.action : (typeof body?.action === 'string' ? body.action : 'add_model')
    if (!validated.ok) {
      return reply(validated.status, {
        ok: false,
        action,
        dryRun: body?.dryRun === true,
        code: validated.code,
        message: validated.message,
      })
    }
    const request = validated.request
    const dryRun = request.dryRun
    let current
    try {
      current = dryRun ? await readCurrentState() : await snapshot.state({ force: true })
    } catch (error) {
      log?.(`${request.action} state read failed: ${error instanceof Error ? error.message : String(error)}`)
      current = undefined
    }
    if (current === undefined || current.ok !== true) {
      return reply(502, {
        ok: false,
        action: request.action,
        dryRun,
        code: 'state_unavailable',
        message: 'the current tmax state is unavailable, so the selection cannot be validated',
      })
    }
    const planned = request.action === 'add_model'
      ? planAddModel(request, { state: current })
      : planRemoveModel(request, { state: current })
    if (!planned.ok) {
      return reply(planned.status, {
        ok: false,
        action: request.action,
        dryRun,
        code: planned.code,
        message: planned.message,
      })
    }
    const plan = planned.plan
    if (dryRun) {
      return reply(200, {
        ok: true,
        action: request.action,
        dryRun: true,
        result: secretFree({ code: 'ok', status: 'dry_run', data: writePlanResult(plan) }),
        state: current,
      })
    }
    const key = actionLockKey(request.action)
    if (running.has(key)) {
      return reply(409, {
        ok: false,
        action: request.action,
        dryRun: false,
        code: 'busy',
        message: 'another target write is already running',
      })
    }
    running.add(key)
    const steps = []
    const base = { target: plan.target, provider: plan.provider, model: plan.model, protocol: plan.protocol, entry: plan.entry }
    const deadline = Date.now() + actionTimeoutMs(request.action)
    let evidence
    try {
      for (const step of plan.steps) {
        const remaining = deadline - Date.now()
        if (remaining <= 0) {
          const message = `the ${request.action} deadline expired before the next step`
          const state = await freshState(current)
          return reply(502, {
            ok: false,
            action: request.action,
            dryRun: false,
            code: 'tmax_unavailable',
            message,
            result: secretFree({ code: 'tmax_unavailable', status: 'error', message, data: { ...base, steps, failedStep: step.name } }),
            state,
          })
        }
        const read = await readEnvelope(config, step.argv, { timeoutMs: Math.min(CONNECT_TIMEOUT_MS, remaining), env })
        if (read.envelope === undefined) {
          const message = read.message ?? 'tmax did not answer'
          const state = await freshState(current)
          return reply(502, {
            ok: false,
            action: request.action,
            dryRun: false,
            code: 'tmax_unavailable',
            message,
            result: secretFree({
              code: 'tmax_unavailable',
              status: 'error',
              message,
              data: { ...base, steps, failedStep: step.name },
            }),
            state,
          })
        }
        const result = resultFromEnvelope(read.envelope)
        if (!isActionSuccess(read.envelope, false)) {
          const message = result.message ?? envelopeMessage(read.envelope)
          result.message = message
          const state = await freshState(current)
          return reply(409, {
            ok: false,
            action: request.action,
            dryRun: false,
            code: result.code,
            message,
            result: secretFree({
              ...result,
              data: writeResultData(
                base,
                [...steps, { name: step.name, code: result.code, status: result.status, protocol: step.protocol ?? null }],
                { failedStep: step.name },
                result,
              ),
            }),
            state,
          })
        }
        steps.push({ name: step.name, code: result.code, status: result.status, protocol: step.protocol ?? null })
        evidence = result
      }
      const success = {
        code: evidence?.code ?? 'ok',
        status: 'success',
        data: writeResultData(base, steps, {}, evidence),
      }
      for (const field of ['effects', 'blockers', 'next_actions']) {
        if (evidence?.[field] !== undefined) success[field] = evidence[field]
      }
      const state = await freshState(current)
      return reply(200, {
        ok: true,
        action: request.action,
        dryRun: false,
        result: secretFree(success),
        state,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      log?.(`${request.action} failed: ${message}`)
      const state = await freshState(current)
      return reply(502, {
        ok: false,
        action: request.action,
        dryRun: false,
        code: 'action_failed',
        message,
        result: secretFree({ code: 'action_failed', status: 'error', message, data: { ...base, steps } }),
        state,
      })
    } finally {
      running.delete(key)
    }
  }

  async function run(body) {
    if (MODEL_ACTIONS.includes(body?.action)) return runModelWrite(body)
    const actionName = typeof body?.action === 'string' ? body.action : null
    const dryRun = body?.dryRun === true
    // Provider actions validate an exact provider ID against state; gateway
    // actions validate that a configured gateway exists; connect resolves the
    // target it applies; apply_target addresses one declared Harness. Each
    // needs the state read first when the request does not already pin the
    // fact; everything else is answered from the request.
    const providerAction = PROVIDER_ACTIONS.includes(actionName)
    const gatewayAction = GATEWAY_ACTIONS.includes(actionName)
    const connectAction = actionName === 'connect'
    const applyTargetAction = actionName === 'apply_target'
    const configuredTarget = explicitTarget(config)
    const needsProviders = providerAction
      && typeof body?.provider === 'string'
      && body.provider !== ''
    const needsHarnesses = applyTargetAction
      && typeof body?.target === 'string'
      && body.target !== ''
    const needsState = needsProviders || gatewayAction || needsHarnesses || (connectAction && configuredTarget === undefined)
    let current
    if (needsState) {
      current = await readCurrentState()
      // A state the host itself reports as unavailable cannot prove which
      // providers or gateway exist: that is a 502, never a conflict.
      if (current === undefined || current.ok !== true) {
        if (connectAction) {
          // An unresolvable target is the state's own typed answer, not a
          // transport failure: keep it a 409 the client can act on. A state
          // read that did not answer at all stays a 502.
          const failures = Array.isArray(current?.errors) ? current.errors : []
          const failure = failures.find(error => error?.section === 'projection')
          if (failure !== undefined) {
            return reply(409, {
              ok: false,
              action: actionName,
              dryRun,
              code: typeof failure.code === 'string' ? failure.code : 'target_unresolved',
              message: typeof failure.message === 'string' ? failure.message : 'no declared DSH target resolves for this home',
            })
          }
          return reply(502, {
            ok: false,
            action: actionName,
            dryRun,
            code: 'state_unavailable',
            message: 'the current tmax state is unavailable, so the apply target cannot be resolved',
          })
        }
        return reply(502, {
          ok: false,
          action: actionName,
          dryRun,
          code: 'state_unavailable',
          message: 'the current tmax state is unavailable, so the action cannot be validated',
        })
      }
      if (applyTargetAction) {
        // A Harness list that failed to read is a typed refusal, not an empty
        // list: the target cannot be proven declared, so no apply may run.
        const failure = listOf(current.errors).find(error => isPlainObject(error) && error.section === 'harnesses')
        if (failure !== undefined) {
          return reply(409, {
            ok: false,
            action: actionName,
            dryRun,
            code: 'harnesses_unavailable',
            message: textOf(failure.message) ?? 'the declared tmax Harnesses could not be read, so the target cannot be validated',
          })
        }
      }
      if (gatewayAction) {
        const gateway = current.gateway
        if (gateway === null || gateway === undefined || gateway.configured === null || gateway.configured === undefined) {
          return reply(409, {
            ok: false,
            action: actionName,
            dryRun,
            code: 'gateway_unavailable',
            message: 'the tmax gateway state is unavailable, so its lifecycle cannot be controlled',
          })
        }
        if (gateway.configured !== true) {
          return reply(409, {
            ok: false,
            action: actionName,
            dryRun,
            code: 'gateway_not_configured',
            message: 'the tmax gateway is not configured; configure it before starting or stopping it',
          })
        }
      }
    }
    const connectTarget = configuredTarget ?? current?.target?.id
    if (connectAction && (connectTarget === undefined || connectTarget === null)) {
      const failures = Array.isArray(current?.errors) ? current.errors : []
      const failure = failures.find(error => error?.section === 'projection')
      return reply(409, {
        ok: false,
        action: actionName,
        dryRun,
        code: typeof failure?.code === 'string' ? failure.code : 'target_unresolved',
        message: typeof failure?.message === 'string' ? failure.message : 'no declared DSH target resolves for this home',
      })
    }
    const planned = planAction(body, {
      providerIds: needsProviders ? providerIdsFromState(current) : [],
      targets: needsHarnesses ? harnessIdsFromState(current) : undefined,
      config,
      env,
      target: connectTarget,
    })
    if (!planned.ok) return reply(planned.status, { ok: false, action: actionName, dryRun, code: planned.code, message: planned.message })
    const plan = planned.plan
    const key = actionLockKey(plan.action, plan.provider, plan.target) ?? plan.action
    if (running.has(key)) {
      const named = plan.target ?? plan.provider
      return reply(409, {
        ok: false,
        action: plan.action,
        dryRun: plan.dryRun,
        code: 'busy',
        message: `another ${plan.action}${named === null || named === undefined ? '' : ` for "${named}"`} is already running`,
      })
    }
    running.add(key)
    try {
      const read = await readEnvelope(config, plan.argv, { timeoutMs: plan.timeoutMs, env })
      if (read.envelope !== undefined) {
        // A completed operation may have written native files (connect is the
        // only such action today). Re-read the home facts before composing the
        // state this response carries; a failed re-read keeps the last decision.
        try {
          await refreshOwnership?.()
        } catch (error) {
          log?.(`action ownership re-evaluation failed: ${error instanceof Error ? error.message : String(error)}`)
        }
      }
      const result = resultFromEnvelope(read.envelope)
      const state = await freshState(current)
      if (read.envelope === undefined) {
        // No typed envelope means transport/timeout, never a tmax refusal.
        const message = read.message ?? 'tmax did not answer'
        return reply(502, {
          ok: false,
          action: plan.action,
          dryRun: plan.dryRun,
          code: 'tmax_unavailable',
          message,
          result: { code: 'tmax_unavailable', status: 'error', message },
          state,
        })
      }
      if (!isActionSuccess(read.envelope, plan.dryRun)) {
        // Preserve the refusal: a 409, not a generic failure. The message a
        // client renders may live on the envelope's first blocker.
        const message = result.message ?? envelopeMessage(read.envelope)
        result.message = message
        log?.(`action ${plan.action} refused by tmax: ${result.code}`)
        return reply(409, {
          ok: false,
          action: plan.action,
          dryRun: plan.dryRun,
          code: result.code,
          message,
          result,
          state,
        })
      }
      return reply(200, { ok: true, action: plan.action, dryRun: plan.dryRun, result, state })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      log?.(`action ${plan.action} failed: ${message}`)
      return reply(502, {
        ok: false,
        action: plan.action,
        dryRun: plan.dryRun,
        code: 'action_failed',
        message,
        result: { code: 'action_failed', status: 'error', message },
        state: await freshState(current),
      })
    } finally {
      running.delete(key)
    }
  }

  return { run }
}
