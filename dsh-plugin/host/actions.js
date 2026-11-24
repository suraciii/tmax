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
  if (action === 'connect') return 'connect'
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

  async function run(body) {
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
