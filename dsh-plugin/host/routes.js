// Exact HTTP routes for the tmax page and seats.
//
// GET routes are effect-free reads of the host state/observation cache: no
// provider refresh, no vendor request, no connect, and no credential
// resolution. POST routes are explicit operations with defense-in-depth checks
// on top of DSH's authenticated route admission: JSON media type, a 16 KiB
// body bound, same-origin when the browser states an origin, single-flight per
// action, and typed results. No route accepts arbitrary argv: `observe` builds
// one fixed `provider refresh` per provider tmax lists, `connect` runs the one
// `agent apply` form, and `action` delegates the allowlist and exact argv to
// `host/actions.js`.
import { secretFree } from './actions.js'
import { TmaxError } from './cli.js'
import { observeTmax } from './observations.js'

export const MAX_BODY_BYTES = 16 * 1024
const SEARCH_SCOPES = new Set(['all', 'tmax', 'agent'])

function headerOf(req, name) {
  const headers = req?.headers ?? {}
  let value = headers[name] ?? headers[name.toLowerCase()]
  if (value === undefined) {
    for (const [key, entry] of Object.entries(headers)) {
      if (key.toLowerCase() === name) {
        value = entry
        break
      }
    }
  }
  if (Array.isArray(value)) return value[0]
  return typeof value === 'string' ? value : undefined
}

function sendJson(res, status, payload, extraHeaders = {}) {
  res.statusCode = status
  res.setHeader('content-type', 'application/json; charset=utf-8')
  res.setHeader('cache-control', 'no-store')
  res.setHeader('x-content-type-options', 'nosniff')
  for (const [name, value] of Object.entries(extraHeaders)) res.setHeader(name, value)
  res.end(JSON.stringify(payload))
}

function methodNotAllowed(res, allowed) {
  sendJson(res, 405, { ok: false, code: 'method_not_allowed', message: `use ${allowed}` }, { allow: allowed })
}

function sameOrigin(req) {
  const site = headerOf(req, 'sec-fetch-site')
  if (site !== undefined && site !== 'same-origin' && site !== 'none') return false
  const origin = headerOf(req, 'origin')
  if (origin === undefined) return true
  if (origin === 'null') return false
  let originHost
  try {
    originHost = new URL(origin).host
  } catch {
    return false
  }
  const host = headerOf(req, 'host')
  return typeof host === 'string' && host !== '' && originHost === host
}

function bodyTooLarge() {
  const error = new Error('body_too_large')
  error.code = 'body_too_large'
  return error
}

async function readBody(req) {
  let size = 0
  const chunks = []
  const push = chunk => {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)
    size += buffer.byteLength
    if (size > MAX_BODY_BYTES) throw bodyTooLarge()
    chunks.push(buffer)
  }
  if (typeof req?.[Symbol.asyncIterator] === 'function') {
    for await (const chunk of req) push(chunk)
  } else if (typeof req?.on === 'function') {
    await new Promise((resolve, reject) => {
      req.on('data', chunk => {
        try {
          push(chunk)
        } catch (error) {
          reject(error)
        }
      })
      req.on('end', resolve)
      req.on('error', reject)
    })
  }
  return Buffer.concat(chunks).toString('utf8')
}

/**
 * Read and validate one POST body. Responds and returns undefined on any guard
 * failure, so a handler can `if (body === undefined) return`.
 */
async function readJsonPost(req, res) {
  if (req?.method !== 'POST') {
    methodNotAllowed(res, 'POST')
    return undefined
  }
  if (!sameOrigin(req)) {
    sendJson(res, 403, { ok: false, code: 'origin_not_allowed', message: 'cross-origin requests are refused' })
    return undefined
  }
  const contentType = headerOf(req, 'content-type')
  const mediaType = typeof contentType === 'string' ? contentType.split(';')[0].trim().toLowerCase() : ''
  if (mediaType !== 'application/json') {
    sendJson(res, 415, { ok: false, code: 'unsupported_media_type', message: 'content-type must be application/json' })
    return undefined
  }
  const declaredLength = headerOf(req, 'content-length')
  if (declaredLength !== undefined) {
    const length = Number(declaredLength)
    if (!Number.isFinite(length) || length < 0) {
      sendJson(res, 400, { ok: false, code: 'invalid_body', message: 'content-length is not a valid size' })
      return undefined
    }
    if (length > MAX_BODY_BYTES) {
      sendJson(res, 413, { ok: false, code: 'body_too_large', message: `request bodies are limited to ${MAX_BODY_BYTES} bytes` })
      return undefined
    }
  }
  let raw
  try {
    raw = await readBody(req)
  } catch (error) {
    if (error?.code === 'body_too_large') {
      sendJson(res, 413, { ok: false, code: 'body_too_large', message: `request bodies are limited to ${MAX_BODY_BYTES} bytes` })
      return undefined
    }
    sendJson(res, 400, { ok: false, code: 'invalid_body', message: 'the request body could not be read' })
    return undefined
  }
  if (raw.trim() === '') return {}
  let parsed
  try {
    parsed = JSON.parse(raw)
  } catch {
    sendJson(res, 400, { ok: false, code: 'invalid_json', message: 'the request body is not JSON' })
    return undefined
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    sendJson(res, 400, { ok: false, code: 'invalid_body', message: 'the request body must be a JSON object' })
    return undefined
  }
  return parsed
}

/** Legacy usage payload derived from the state read model (cache-only). */
function legacyUsage(state) {
  return {
    ok: state.ok,
    generatedAt: state.generatedAt,
    providers: (state.providers?.tmax ?? []).map(provider => ({
      id: provider.id,
      name: provider.name ?? provider.id,
      enabled: provider.enabled,
      quota: {
        state: provider.suspension?.state,
        reason: provider.suspension?.reasonCode,
        resetAt: provider.suspension?.resetAt,
        source: provider.suspension?.source,
        window: provider.suspension?.window,
      },
      observedAt: provider.billing?.observedAt,
      integration: provider.integration,
      billing: {
        kind: provider.billing?.kind,
        state: provider.billing?.state,
        dashboard: provider.billing?.dashboard,
        freshness: provider.billing?.freshness,
        rows: provider.billing?.rows ?? [],
      },
      ...(provider.error === null || provider.error === undefined ? {} : { error: provider.error.code ?? provider.error }),
    })),
  }
}

/** Compact projection summary that the first-generation page still reads. */
function statusSummary(state, declaredBlockers) {
  return {
    ok: state.ok,
    errors: state.errors ?? [],
    schemaVersion: state.schemaVersion,
    generatedAt: state.generatedAt,
    configurationRevision: state.configurationRevision,
    tmaxVersion: state.tmaxVersion,
    authority: state.authority,
    target: state.target,
    ownership: state.ownership,
    gateway: state.gateway,
    config: state.config,
    routes: (state.routes ?? []).map(route => ({
      name: route.name,
      provider: route.provider,
      protocol: route.protocol,
      baseUrl: route.baseUrl,
      credentialConfigured: route.credentialConfigured,
      scope: route.scope,
      active: route.active === true,
      blockers: route.blockers ?? [],
      models: (route.models ?? []).map(model => ({
        id: model.id,
        name: model.name,
        kind: model.kind,
        availability: model.availability,
      })),
    })),
    providers: state.providers,
    routing: state.routing,
    diagnostics: state.diagnostics,
    blockers: deriveBlockers(state, declaredBlockers),
    lastError: state.diagnostics?.lastError ?? null,
  }
}

/**
 * Everything that currently prevents the reported access from being usable.
 * Declared route-build blockers come first; section facts follow. This is a
 * view, never a mutation.
 */
export function deriveBlockers(state, declaredBlockers = []) {
  const blockers = []
  const add = (source, code, message, extra) => {
    if (typeof code !== 'string' || code === '') return
    blockers.push({
      source,
      code,
      message: typeof message === 'string' && message !== '' ? message : code,
      ...(extra === undefined ? {} : extra),
    })
  }
  for (const blocker of declaredBlockers) {
    add('route', blocker.code, blocker.message, Array.isArray(blocker.models) ? { models: blocker.models } : undefined)
  }
  for (const blocker of state.target?.blockers ?? []) add('target', blocker.code, blocker.message)
  if (state.ownership?.conflict !== null && state.ownership?.conflict !== undefined) {
    add('ownership', state.ownership.conflict.code, state.ownership.conflict.message)
  }
  const sectionErrors = Array.isArray(state.errors) ? state.errors : []
  const projectionFailure = sectionErrors.find(error => error?.section === 'projection')
  for (const error of sectionErrors) {
    // A typed projection failure keeps its own code; it is not an unreachable
    // tmax CLI. Other failed sections remain section-scoped facts.
    if (error?.section === 'projection') add('projection', error.code, error.message)
    else add('section', error.code, `${error.section}: ${error.message}`)
  }
  for (const error of state.config?.errors ?? []) add('config', error.code, error.message)
  if (projectionFailure === undefined && typeof state.diagnostics?.lastError === 'string' && state.diagnostics.lastError !== '') {
    // No route-source section error exists, so the read model itself was
    // built; a retained tmax last error is degraded inventory evidence, not a
    // missing read. The transport pair stays source 'projection' through the
    // section error above and is never claimed twice.
    add('inventory', 'inventory_degraded', state.diagnostics.lastError)
  }
  for (const provider of state.providers?.tmax ?? []) {
    if (provider.error !== null && provider.error !== undefined) {
      add('provider', provider.error.code ?? 'provider_unavailable', `${provider.id}: ${provider.error.message ?? 'provider inspect failed'}`)
    }
  }
  return blockers
}

/**
 * Build the exact route table.
 * @param {object} input
 * @param {object} input.config
 * @param {object} input.snapshot - state service with `state()`/`invalidate()`
 * @param {object} input.observations - observation cache
 * @param {{run: (body: object) => Promise<{status: number, body: object}>}} input.actions - action service
 * @param {{read: () => Promise<{status: number, body: object}>}} input.history - retained-history read service
 * @param {Set<string>} [input.locks] - single-flight set shared with the action service
 * @param {(options: {dryRun: boolean}) => Promise<object>} input.connectTmax
 * @param {() => object[]} input.getDeclaredBlockers
 * @param {(message: string) => void} [input.log]
 */
export function createRoutes({ config, snapshot, observations, actions, history, locks = new Set(), connectTmax, getDeclaredBlockers, log }) {
  // The action service shares this set: legacy `/connect` and action `connect`
  // are one lock, legacy `observe` and action observations are one lock.
  const running = locks

  function acquire(key) {
    if (running.has(key)) return false
    running.add(key)
    return true
  }

  function release(key) {
    running.delete(key)
  }

  async function readState() {
    return snapshot.state()
  }

  const routes = [
    {
      path: '/_tmax/state',
      method: 'GET',
      async handler(req, res) {
        if (req?.method !== 'GET') return methodNotAllowed(res, 'GET')
        try {
          sendJson(res, 200, await readState())
        } catch (error) {
          sendJson(res, 502, { ok: false, code: 'state_unavailable', message: error instanceof Error ? error.message : String(error) })
        }
      },
    },
    {
      path: '/_tmax/status',
      method: 'GET',
      async handler(req, res) {
        if (req?.method !== 'GET') return methodNotAllowed(res, 'GET')
        try {
          const state = await readState()
          sendJson(res, 200, statusSummary(state, getDeclaredBlockers()))
        } catch (error) {
          sendJson(res, 502, { ok: false, code: 'status_unavailable', message: error instanceof Error ? error.message : String(error) })
        }
      },
    },
    {
      path: '/_tmax/usage',
      method: 'GET',
      async handler(req, res) {
        if (req?.method !== 'GET') return methodNotAllowed(res, 'GET')
        try {
          sendJson(res, 200, legacyUsage(await readState()))
        } catch (error) {
          sendJson(res, 502, { ok: false, code: 'usage_unavailable', message: error instanceof Error ? error.message : String(error) })
        }
      },
    },
    {
      path: '/_tmax/dsh-usage',
      method: 'GET',
      handler(req, res) {
        if (req?.method !== 'GET') return methodNotAllowed(res, 'GET')
        // Cache-only. A GET never resolves a credential or contacts a vendor;
        // the absence of a reading is reported as `not_observed`.
        try {
          sendJson(res, 200, observations.usageRecords())
        } catch (error) {
          sendJson(res, 502, { ok: false, code: 'usage_unavailable', message: error instanceof Error ? error.message : String(error) })
        }
      },
    },
    {
      path: '/_tmax/history',
      method: 'GET',
      async handler(req, res) {
        if (req?.method !== 'GET') return methodNotAllowed(res, 'GET')
        // One read-only, bounded `tmax history list`: no observation, refresh,
        // service start, clear, or write. The service maps typed availability.
        try {
          const outcome = await history.read()
          sendJson(res, outcome.status, outcome.body)
        } catch (error) {
          sendJson(res, 502, { ok: false, code: 'history_unavailable', message: error instanceof Error ? error.message : String(error) })
        }
      },
    },
    {
      path: '/_tmax/observe',
      method: 'POST',
      async handler(req, res) {
        const body = await readJsonPost(req, res)
        if (body === undefined) return
        const scope = body.scope ?? 'all'
        if (typeof scope !== 'string' || !SEARCH_SCOPES.has(scope)) {
          sendJson(res, 400, { ok: false, code: 'invalid_scope', message: "scope must be 'all', 'tmax', or 'agent'" })
          return
        }
        const dryRun = body.dryRun ?? false
        if (typeof dryRun !== 'boolean') {
          sendJson(res, 400, { ok: false, code: 'invalid_dry_run', message: 'dryRun must be a boolean' })
          return
        }
        if (!acquire('observe')) {
          sendJson(res, 409, { ok: false, code: 'busy', message: 'an observation is already running' })
          return
        }
        try {
          const items = []
          if (scope === 'tmax' || scope === 'all') items.push(...(await observeTmax(config, { dryRun })).items)
          if (scope === 'agent' || scope === 'all') items.push(...(await observations.observeAgent({ dryRun })).items)
          snapshot.invalidate()
          const state = await snapshot.state({ force: true })
          const ok = !items.some(item => item.status === 'failed')
          sendJson(res, ok ? 200 : 207, { ok, generatedAt: new Date().toISOString(), dryRun, items, state })
        } catch (error) {
          log?.(`observe failed: ${error instanceof Error ? error.message : String(error)}`)
          sendJson(res, 502, { ok: false, code: 'observe_failed', message: error instanceof Error ? error.message : String(error) })
        } finally {
          release('observe')
        }
      },
    },
    {
      path: '/_tmax/connect',
      method: 'POST',
      async handler(req, res) {
        const body = await readJsonPost(req, res)
        if (body === undefined) return
        const dryRun = body.dryRun ?? false
        if (typeof dryRun !== 'boolean') {
          sendJson(res, 400, { ok: false, code: 'invalid_dry_run', message: 'dryRun must be a boolean' })
          return
        }
        if (!acquire('connect')) {
          sendJson(res, 409, { ok: false, code: 'busy', message: 'a connect is already running' })
          return
        }
        try {
          const result = await connectTmax({ dryRun })
          if (result.envelope === undefined) {
            // A transport/timeout failure is not a typed refusal.
            sendJson(res, 502, {
              ok: false,
              code: 'connect_unavailable',
              message: result.message ?? 'tmax agent apply did not answer',
            })
            return
          }
          snapshot.invalidate()
          const state = await snapshot.state({ force: true })
          if (!result.ok) {
            const connect = {
              status: result.status ?? null,
              code: result.code,
              message: result.message ?? null,
              blockers: Array.isArray(result.envelope?.blockers) ? result.envelope.blockers : null,
            }
            sendJson(res, 409, {
              ok: false,
              code: result.code,
              message: result.message ?? 'tmax agent apply did not complete',
              connect: secretFree(connect),
              state,
            })
            return
          }
          sendJson(res, 200, { ok: true, dryRun, connect: secretFree(result.data ?? {}), state })
        } catch (error) {
          // A typed target-resolution failure is a refusal the client can act
          // on, not a transport failure: keep it a 409 with its own code.
          if (error instanceof TmaxError) {
            sendJson(res, 409, { ok: false, code: error.code, message: error.message })
            return
          }
          log?.(`connect failed: ${error instanceof Error ? error.message : String(error)}`)
          sendJson(res, 502, { ok: false, code: 'connect_unavailable', message: error instanceof Error ? error.message : String(error) })
        } finally {
          release('connect')
        }
      },
    },
    {
      path: '/_tmax/action',
      method: 'POST',
      async handler(req, res) {
        const body = await readJsonPost(req, res)
        if (body === undefined) return
        try {
          // The action service owns the allowlist, exact argv, single-flight,
          // timeouts, and the typed 200/409/502 mapping. This route only reads
          // the shared POST body guards above.
          const outcome = await actions.run(body)
          sendJson(res, outcome.status, outcome.body)
        } catch (error) {
          log?.(`action failed: ${error instanceof Error ? error.message : String(error)}`)
          const message = error instanceof Error ? error.message : String(error)
          sendJson(res, 502, {
            ok: false,
            action: typeof body.action === 'string' ? body.action : null,
            dryRun: body.dryRun === true,
            code: 'action_failed',
            message,
            result: { code: 'action_failed', status: 'error', message },
          })
        }
      },
    },
    {
      path: '/_tmax/usage/refresh',
      method: 'POST',
      async handler(req, res) {
        // First-generation page compatibility: one explicit tmax observation,
        // then the same usage payload. It is still an explicit POST, and a
        // preview is a preview: `--dry-run` reaches every refresh and no
        // provider is contacted.
        const body = await readJsonPost(req, res)
        if (body === undefined) return
        const requested = body.dryRun ?? body.preview ?? false
        if (typeof requested !== 'boolean') {
          sendJson(res, 400, { ok: false, code: 'invalid_dry_run', message: 'dryRun must be a boolean' })
          return
        }
        const preview = requested === true
          || (typeof req?.url === 'string' && /[?&](?:preview|dry-run)=1(&|$)/u.test(req.url))
        if (!acquire('observe')) {
          sendJson(res, 409, { ok: false, code: 'busy', message: 'an observation is already running' })
          return
        }
        try {
          const result = await observeTmax(config, { dryRun: preview })
          if (!preview) snapshot.invalidate()
          const state = await snapshot.state({ force: !preview })
          const ok = !result.items.some(item => item.status === 'failed')
          sendJson(res, ok ? 200 : 207, {
            ok,
            dryRun: preview,
            ...legacyUsage(state),
            generatedAt: new Date().toISOString(),
            items: result.items,
          })
        } catch (error) {
          sendJson(res, 502, { ok: false, code: 'observe_failed', message: error instanceof Error ? error.message : String(error) })
        } finally {
          release('observe')
        }
      },
    },
  ]
  return routes
}
