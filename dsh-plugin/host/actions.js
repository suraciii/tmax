// The allowlisted host action runner behind POST /_tmax/action.
//
// The client may name one action and one operand set; it never names a command,
// a flag, or a path. This module owns the exact argv for every action, the
// closed scope enum, the per-action timeout, and the single-flight key. A
// provider ID is accepted only when it is exactly one ID from the current host
// state, so an arbitrary client string can never reach argv. Connect is the
// native application `tmax agent apply TARGET`; the legacy `tmax dsh connect`
// form no longer exists.
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
])

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
 * process startup), so it gets the longest deadline.
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
 * verb locks itself.
 */
export function actionLockKey(action, provider = null) {
  if (action === 'connect' || action === 'add_model') return 'connect'
  if (action === 'observe_provider') return 'observe'
  if (action === 'resume_quota' || action === 'reset_failure') {
    return typeof provider === 'string' && provider !== '' ? `${action}:${provider}` : action
  }
  if (action === 'open_management') return 'open_management'
  if (GATEWAY_ACTIONS.includes(action)) return `gateway:${GATEWAY_VERBS[action]}`
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
  if (action === 'add_model') {
    const provider = body.provider
    const model = body.model
    const protocol = body.protocol
    if (typeof provider !== 'string' || provider === '') return invalid(400, 'invalid_provider', 'add_model requires a provider ID')
    if (typeof model !== 'string' || model === '') return invalid(400, 'invalid_model', 'add_model requires a model ID')
    if (typeof protocol !== 'string' || !ADD_PROTOCOLS.includes(protocol)) {
      return invalid(400, 'invalid_protocol', `protocol must be one of ${ADD_PROTOCOLS.join(', ')}`)
    }
    return { ok: true, request: { action, provider, model, protocol, scope: null, dryRun } }
  }
  if (PROVIDER_ACTIONS.includes(action)) {
    const provider = body.provider
    if (provider === undefined || provider === null) return invalid(400, 'missing_provider', `${action} requires a provider`)
    if (typeof provider !== 'string' || provider === '') {
      return invalid(400, 'invalid_provider', 'provider must be a non-empty provider ID')
    }
    return { ok: true, request: { action, provider, scope, dryRun } }
  }
  // A provider for an action that names none is ignored, never argv.
  return { ok: true, request: { action, provider: null, scope, dryRun } }
}

/**
 * Plan one fixed target-selection write: add one existing catalog model to the
 * declared DSH target and apply it. The browser never names an entry, a command,
 * or a flag; every operand is derived from the current catalog. Pure.
 *
 * @returns {{ok: true, plan: {provider: string, model: string, protocol: string, target: string, entry: {name: string, mode: 'existing'|'create'}, steps: {name: string, argv: string[]}[]}}
 *   | {ok: false, status: number, code: string, message: string}}
 */
export function planAddModel(request, { state, target } = {}) {
  if (!isPlainObject(request) || request.action !== 'add_model') return invalid(400, 'invalid_action', 'an add_model request is required')
  const providerId = textOf(request.provider)
  const modelId = textOf(request.model)
  const protocol = textOf(request.protocol)
  const resolvedTarget = textOf(target)
  if (providerId === null) return invalid(400, 'invalid_provider', 'add_model requires a provider ID')
  if (modelId === null) return invalid(400, 'invalid_model', 'add_model requires a model ID')
  if (protocol === null || !ADD_PROTOCOLS.includes(protocol)) return invalid(400, 'invalid_protocol', `protocol must be one of ${ADD_PROTOCOLS.join(', ')}`)
  if (resolvedTarget === null) return invalid(409, 'target_unresolved', 'no declared DSH target resolves for this home; declare one before adding a model')
  // `agent apply` writes the target's whole desired state. Never let one add
  // silently carry unrelated pending selections/default/role edits: require a
  // standalone Apply first so the add applies only the state the user saw.
  const pending = state?.target?.pendingNativeChanges
  // Only a readable, empty pending list proves the target has no hidden deltas.
  // tmax reports null when planning/scope comparison could not run; that is an
  // unreadable target, distinct from a known non-empty pending list.
  if (!Array.isArray(pending)) {
    return invalid(409, 'target_uninspectable', 'the target pending state is unreadable; apply the target before adding a model')
  }
  if (pending.length > 0) {
    return invalid(409, 'pending_native_changes', 'the target has pending native changes; apply them before adding a model')
  }
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
  if (model.joined !== true && model.joined !== false) {
    return invalid(409, 'target_uninspectable', 'the target selection cannot be read, so the model cannot be added safely')
  }
  if (provider.entries === null || provider.entries === undefined) {
    return invalid(409, 'target_uninspectable', 'the target entries cannot be read, so the model cannot be added safely')
  }
  const joinedProtocols = listOf(model.joinedProtocols)
  if (joinedProtocols.includes(protocol)) {
    return invalid(409, 'already_joined', `model "${providerId}/${modelId}" is already selected for protocol "${protocol}"`)
  }
  if (provider.source === 'gateway' && state?.gateway?.configured !== true) {
    return invalid(409, 'gateway_not_configured', 'the tmax gateway is not configured, so its route cannot be added to the target')
  }
  const entries = listOf(provider.entries).filter(entry => isPlainObject(entry) && textOf(entry.name) !== null)
  // Prefer an entry that already serves this protocol; otherwise fill an
  // existing entry that has no model yet before creating another name.
  const existing = entries.find(entry => listOf(entry.protocols).includes(protocol))
    ?? entries.find(entry => listOf(entry.protocols).length === 0)
  let entry
  if (existing !== undefined) {
    entry = { name: textOf(existing.name), mode: 'existing' }
  } else {
    // A DSH entry holds one protocol. Reuse a matching entry; otherwise create
    // a target-local name that no declared entry already uses.
    const names = new Set()
    for (const row of listOf(catalog.providers)) {
      for (const candidate of listOf(row?.entries)) {
        const name = textOf(candidate?.name)
        if (name !== null) names.add(name)
      }
    }
    const base = provider.source === 'gateway' ? 'tmax' : String(provider.id)
    if (/[\s\u0000-\u001f]/u.test(base)) {
      return invalid(409, 'entry_name_unavailable', 'the provider ID cannot form a native entry name')
    }
    let name = base
    if (names.has(name)) name = `${base}-${protocol}`
    for (let suffix = 2; names.has(name) && suffix < 10; suffix += 1) name = `${base}-${protocol}-${suffix}`
    if (names.has(name)) return invalid(409, 'entry_name_conflict', 'no free target entry name is available for this provider')
    entry = { name, mode: 'create' }
  }
  const steps = []
  if (entry.mode === 'create') {
    steps.push({ name: 'provider_add', argv: ['agent', 'provider', 'add', resolvedTarget, entry.name, '--provider', String(provider.id)] })
  }
  steps.push({ name: 'model_add', argv: ['agent', 'model', 'add', resolvedTarget, entry.name, String(model.id), '--protocol', protocol] })
  steps.push({ name: 'apply', argv: ['agent', 'apply', resolvedTarget] })
  return {
    ok: true,
    plan: {
      provider: String(provider.id),
      model: String(model.id),
      protocol,
      target: resolvedTarget,
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
 * Validate one request against the addressable provider IDs and build its exact
 * argv, timeout, and single-flight key. Pure: no CLI, no clock, no mutation.
 * @returns {{ok: true, plan: {action: string, provider: string|null, scope: string|null, dryRun: boolean, argv: string[], timeoutMs: number}}
 *   | {ok: false, status: number, code: string, message: string}}
 */
export function planAction(body, { providerIds = [], config = {}, env = process.env, target = undefined } = {}) {
  const validated = validateActionRequest(body)
  if (!validated.ok) return validated
  const request = validated.request
  if (request.provider !== null) {
    const known = Array.isArray(providerIds) ? providerIds : []
    if (!known.includes(request.provider)) {
      return invalid(409, 'unknown_provider', `provider "${request.provider}" is not in the current tmax state`)
    }
  }
  const argv = actionArgv(request.action, { ...request, config, env, target })
  if (argv === null) {
    if (request.action === 'connect') {
      return invalid(409, 'target_unresolved', 'no declared DSH target resolves for this home; declare one before applying it')
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
  function addPlanResult(plan) {
    return {
      provider: plan.provider,
      model: plan.model,
      protocol: plan.protocol,
      target: plan.target,
      entry: plan.entry,
      steps: plan.steps.map(step => step.name),
    }
  }

  /** Merge plan/step metadata into one tmax step's data without dropping its evidence. */
  function addResultData(base, steps, extra, evidence) {
    const data = isPlainObject(evidence?.data) ? { ...evidence.data } : {}
    return { ...data, ...base, steps, ...extra }
  }

  /**
   * One explicitly previewed target-selection write. A preview is the
   * catalog-derived plan (no tmax invocation, no write); execution runs the
   * fixed provider/model/apply sequence, stops at the first refusal, and
   * reports every completed step and the fresh state. There is no rollback:
   * a partial write is visible and the page can apply or repair it.
   */
  async function runAddModel(body) {
    const validated = validateActionRequest(body)
    if (!validated.ok) {
      return reply(validated.status, {
        ok: false,
        action: 'add_model',
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
      log?.(`add_model state read failed: ${error instanceof Error ? error.message : String(error)}`)
      current = undefined
    }
    if (current === undefined || current.ok !== true) {
      return reply(502, {
        ok: false,
        action: 'add_model',
        dryRun,
        code: 'state_unavailable',
        message: 'the current tmax state is unavailable, so the selection cannot be validated',
      })
    }
    const target = explicitTarget(config) ?? (typeof current.target?.id === 'string' && current.target.id !== '' ? current.target.id : undefined)
    const planned = planAddModel(request, { state: current, target })
    if (!planned.ok) {
      return reply(planned.status, {
        ok: false,
        action: 'add_model',
        dryRun,
        code: planned.code,
        message: planned.message,
      })
    }
    const plan = planned.plan
    if (dryRun) {
      return reply(200, {
        ok: true,
        action: 'add_model',
        dryRun: true,
        result: secretFree({ code: 'ok', status: 'dry_run', data: addPlanResult(plan) }),
        state: current,
      })
    }
    const key = actionLockKey('add_model')
    if (running.has(key)) {
      return reply(409, {
        ok: false,
        action: 'add_model',
        dryRun: false,
        code: 'busy',
        message: 'another target write is already running',
      })
    }
    running.add(key)
    const steps = []
    const base = { provider: plan.provider, model: plan.model, protocol: plan.protocol, entry: plan.entry }
    const deadline = Date.now() + actionTimeoutMs('add_model')
    let applyEvidence
    try {
      for (const step of plan.steps) {
        const remaining = deadline - Date.now()
        if (remaining <= 0) {
          const message = 'the add-model deadline expired before the next step'
          const state = await freshState(current)
          return reply(502, {
            ok: false,
            action: 'add_model',
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
            action: 'add_model',
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
          if (step.name === 'apply') {
            try {
              await refreshOwnership?.()
            } catch (error) {
              log?.(`add_model ownership re-evaluation failed: ${error instanceof Error ? error.message : String(error)}`)
            }
          }
          const state = await freshState(current)
          return reply(409, {
            ok: false,
            action: 'add_model',
            dryRun: false,
            code: result.code,
            message,
            result: secretFree({
              ...result,
              data: addResultData(
                base,
                [...steps, { name: step.name, code: result.code, status: result.status }],
                { failedStep: step.name },
                result,
              ),
            }),
            state,
          })
        }
        steps.push({ name: step.name, code: result.code, status: result.status })
        if (step.name === 'apply') {
          applyEvidence = result
          try {
            await refreshOwnership?.()
          } catch (error) {
            log?.(`add_model ownership re-evaluation failed: ${error instanceof Error ? error.message : String(error)}`)
          }
        }
      }
      const success = {
        code: applyEvidence?.code ?? 'ok',
        status: 'success',
        data: addResultData(base, steps, {}, applyEvidence),
      }
      for (const field of ['effects', 'blockers', 'next_actions']) {
        if (applyEvidence?.[field] !== undefined) success[field] = applyEvidence[field]
      }
      const state = await freshState(current)
      return reply(200, {
        ok: true,
        action: 'add_model',
        dryRun: false,
        result: secretFree(success),
        state,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      log?.(`add_model failed: ${message}`)
      const state = await freshState(current)
      return reply(502, {
        ok: false,
        action: 'add_model',
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
    if (body?.action === 'add_model') return runAddModel(body)
    const actionName = typeof body?.action === 'string' ? body.action : null
    const dryRun = body?.dryRun === true
    // Provider actions validate an exact provider ID against state; gateway
    // actions validate that a configured gateway exists; connect resolves the
    // target it applies. Each needs the state read first when the request does
    // not already pin the fact; everything else is answered from the request.
    const providerAction = PROVIDER_ACTIONS.includes(actionName)
    const gatewayAction = GATEWAY_ACTIONS.includes(actionName)
    const connectAction = actionName === 'connect'
    const configuredTarget = explicitTarget(config)
    const needsProviders = providerAction
      && typeof body?.provider === 'string'
      && body.provider !== ''
    const needsState = needsProviders || gatewayAction || (connectAction && configuredTarget === undefined)
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
      config,
      env,
      target: connectTarget,
    })
    if (!planned.ok) return reply(planned.status, { ok: false, action: actionName, dryRun, code: planned.code, message: planned.message })
    const plan = planned.plan
    const key = actionLockKey(plan.action, plan.provider) ?? plan.action
    if (running.has(key)) {
      return reply(409, {
        ok: false,
        action: plan.action,
        dryRun: plan.dryRun,
        code: 'busy',
        message: `another ${plan.action}${plan.provider === null ? '' : ` for "${plan.provider}"`} is already running`,
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
