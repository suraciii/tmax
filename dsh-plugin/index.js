// Host half: project tmax's declared DSH target into the live LLM registry.
//
// The plugin is deliberately thin: tmax owns provider/model/alias/quota facts;
// this module reads `tmax agent list`, `tmax agent inspect`, and
// `tmax gateway status`, then maps the target's relayed entries onto DSH's
// adapter, tool, and skill surfaces. No credential value ever enters this module.
import { execFile } from 'node:child_process'
import { realpathSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'

import { observeProvider, vendorFor, vendorForKey } from './quota.js'
import { createAdapter, RESOLVED_PROFILE_DEFAULTS, THINKING_LEVELS } from './adapter.js'

export const name = 'tmax'
export const inject = ['llm', 'settings', 'credentials']

const execFileAsync = promisify(execFile)
const API_NAMES = {
  'chat-completions': 'openai-completions',
  'responses': 'openai-responses',
  'messages': 'anthropic-messages',
}

/** Run one tmax command and parse its JSON envelope. Never logs a key. */
async function runTmax(config, args, timeout = 20_000) {
  const tmax = config.tmaxPath || 'tmax'
  let stdout
  try {
    ({ stdout } = await execFileAsync(tmax, args, {
      timeout,
      maxBuffer: 2 * 1024 * 1024,
      env: process.env,
    }))
  } catch (error) {
    // tmax writes its failure envelope to stdout with a nonzero exit status;
    // keep that code and message instead of the exec error.
    if (typeof error?.stdout !== 'string' || error.stdout.trim() === '') throw error
    stdout = error.stdout
  }
  return JSON.parse(stdout)
}

/** One user-actionable failure: the next action is a tmax or configuration change. */
function actionableError(code, message) {
  const error = new Error(message)
  error.code = code
  error.actionable = true
  return error
}

/** The error for one rejected tmax command, carrying its own code and message. */
function envelopeError(command, envelope) {
  const blocker = Array.isArray(envelope.blockers) ? envelope.blockers[0] : undefined
  const message = envelope.message || blocker?.message
  return actionableError(envelope.code || 'unknown',
    `tmax ${command} failed: ${envelope.code || 'unknown'}${message ? ` (${message})` : ''}`)
}

/** Run one read command and return its data, or fail with tmax's code and message. */
async function readCommand(config, args) {
  const envelope = await runTmax(config, args, 10_000)
  if (envelope.code !== 'ok' || envelope.status !== 'success') {
    throw envelopeError(args.slice(0, 2).join(' '), envelope)
  }
  return envelope.data || {}
}

/** One comparable path: the real path when it exists, else separators and trailing slashes removed. */
function normalizePath(value) {
  let path = String(value)
  // Only an existing home can be resolved; an absent one stays comparable by spelling.
  try { path = realpathSync(path) } catch {}
  return path.replaceAll('\\', '/').replace(/\/+$/u, '')
}

/** Resolve the declared DSH target: config.target, or the one target matching the DSH home. */
async function resolveTarget(config) {
  if (config.target) return String(config.target)
  const home = config.home || process.env.DSH_HOME || join(homedir(), '.dsh')
  const listed = await readCommand(config, ['agent', 'list'])
  const wanted = normalizePath(home)
  const matches = (Array.isArray(listed.agents) ? listed.agents : []).filter(agent =>
    agent?.agent === 'dsh' && typeof agent.path === 'string' && normalizePath(agent.path) === wanted)
  const [match] = matches
  if (match === undefined) {
    throw actionableError('unknown_resource',
      `tmax agent list failed: unknown_resource (No declared DSH target matches the DSH home ${home})`)
  }
  if (matches.length > 1) {
    throw actionableError('invalid_arguments',
      'tmax agent list failed: invalid_arguments (Multiple DSH targets match the DSH home; set target or home)')
  }
  return String(match.target)
}

/** One route's model rows: tmax's native projection when inspect carries it, else bare selections. */
function projectModels(projected, selected) {
  const models = Array.isArray(projected?.models)
    ? projected.models
    : selected.map(model => ({ id: model.id, protocol: model.protocol, capabilities: null }))
  return models.map(model => ({
    id: model.id,
    protocol: model.protocol,
    display_name: model.display_name || model.id,
    capabilities: model.capabilities && typeof model.capabilities === 'object' ? model.capabilities : null,
    availability: 'unverified',
  }))
}

/**
 * Compose the target inventory from `tmax agent list`, `tmax agent inspect`,
 * and `tmax gateway status`. The adapter and page read the same fields they
 * always have, so only the source of the data changed.
 */
async function readInventory(config) {
  const target = await resolveTarget(config)
  const [inspect, statusEnvelope] = await Promise.all([
    readCommand(config, ['agent', 'inspect', target]),
    runTmax(config, ['gateway', 'status'], 10_000).catch(() => undefined),
  ])
  if (inspect.agent !== 'dsh') {
    throw actionableError('invalid_arguments',
      'tmax agent inspect failed: invalid_arguments (The selected target is not a DSH target)')
  }
  // A missing or failing gateway status leaves gateway facts unknown; it must
  // not erase the routes that the desired configuration already describes.
  const status = statusEnvelope?.code === 'ok' && statusEnvelope.status === 'success'
    ? statusEnvelope.data || {}
    : undefined
  const daemonState = status?.daemon?.discovery_state ?? null
  const daemonReason = status?.daemon?.reason_code
  const reported = status?.gateway !== null && typeof status?.gateway === 'object' ? status.gateway : undefined
  const providers = Array.isArray(inspect.providers) ? inspect.providers : []
  const projected = Array.isArray(inspect.native_projection) ? inspect.native_projection : []
  // An older tmax predates the relayed source and native capability fields.
  // Fail loudly instead of projecting a target whose routes cannot be read.
  const outdated = providers.some(entry => typeof entry?.relayed !== 'boolean')
    || projected.some(entry => (Array.isArray(entry?.models) ? entry.models : []).some(model =>
      model === null || typeof model !== 'object' || !('capabilities' in model)))
  if (outdated) {
    throw actionableError('unsupported_tmax',
      'tmax agent inspect failed: unsupported_tmax (inspect does not report relayed provider entries and native model capabilities; update tmax)')
  }
  const relayed = providers.filter(entry => entry?.relayed === true)
  const configured = relayed.length > 0 || reported?.key_ref != null || reported?.listen != null
  let ready = null
  let reasonCode = null
  if (!configured) {
    reasonCode = 'gateway_unconfigured'
  } else if (daemonState === 'compatible') {
    ready = reported?.ready === true
    reasonCode = ready ? null : 'gateway_not_ready'
  } else if (daemonState === 'absent') {
    // A configured gateway with no running daemon is not ready, not unknown:
    // tmax status reports readiness false for the same state.
    ready = false
    reasonCode = 'gateway_not_ready'
  } else {
    reasonCode = daemonReason || 'daemon_identity_unavailable'
  }
  const blockers = []
  const routes = []
  const names = []
  if (!configured) {
    blockers.push({
      code: 'gateway_unconfigured',
      message: 'Configure and explicitly start the tmax gateway before this target can use it',
    })
  }
  for (const entry of relayed) {
    const name = String(entry.name)
    const selected = Array.isArray(entry.models) ? entry.models : []
    const protocols = [...new Set(selected.map(model => model?.protocol).filter(Boolean))]
    if (selected.length === 0) {
      blockers.push({ code: 'target_incomplete', message: `Gateway route "${name}" has no selected models` })
      continue
    }
    if (protocols.length !== 1) {
      blockers.push({ code: 'incompatible', message: `Gateway route "${name}" mixes protocols; split it into one route per protocol` })
      continue
    }
    names.push(name)
    routes.push({
      name,
      protocol: protocols[0],
      base_url: entry.base_url ?? null,
      credential_ref: entry.credential_ref ?? null,
      models: projectModels(projected.find(item => item?.name === name), selected),
    })
  }
  if (routes.length === 0 && blockers.length === 0) {
    blockers.push({ code: 'gateway_entry_missing', message: 'This DSH target has no gateway provider entry' })
  }
  if (Array.isArray(inspect.blockers)) blockers.push(...inspect.blockers)
  return {
    schema_version: 1,
    configuration_revision: inspect.configuration_revision ?? null,
    generated_at: new Date().toISOString(),
    target: {
      id: String(inspect.target ?? target),
      path: inspect.path,
      default: inspect.default ?? null,
      routes: names,
    },
    gateway: {
      configured,
      ready,
      listen: reported?.listen ?? null,
      daemon_discovery_state: daemonState,
      reason_code: reasonCode,
    },
    routes,
    blockers,
  }
}

/** One JSON request body; a preview request is the only body this plugin reads. */
function readJsonBody(req) {
  if (req === null || typeof req.on !== 'function') return Promise.resolve({})
  return new Promise(resolve => {
    let body = ''
    req.setEncoding?.('utf8')
    req.on('data', chunk => { if (body.length <= 64 * 1024) body += chunk })
    req.on('end', () => {
      try {
        const value = JSON.parse(body === '' ? '{}' : body)
        resolve(value !== null && typeof value === 'object' ? value : {})
      } catch { resolve({}) }
    })
    req.on('error', () => resolve({}))
  })
}

/** Whether one connect request asks for a dry run instead of a native write. */
function previewRequested(req, body) {
  if (body?.preview === true || body?.dryRun === true) return true
  return req.url !== undefined && /[?&](?:preview|dry-run)=1(&|$)/u.test(req.url)
}

/** Read compact, secret-free provider usage for the Models card. */
async function readUsage(config) {
  const listed = await runTmax(config, ['provider', 'list'])
  if (listed.code !== 'ok' || listed.status !== 'success') {
    throw new Error(`${listed.code || 'provider_list_failed'}: ${listed.message || 'tmax provider list failed'}`)
  }
  const candidates = (listed.data?.providers || []).filter(provider =>
    provider.observations?.billing_state === 'observed'
    || provider.quota_suspension?.state === 'available'
    || provider.quota_suspension?.state === 'suspended')
  const providers = await Promise.all(candidates.map(async provider => {
    const inspected = await runTmax(config, ['provider', 'inspect', String(provider.id)])
    if (inspected.code !== 'ok' || inspected.status !== 'success') {
      return { id: provider.id, name: provider.name, error: inspected.code || 'inspect_failed' }
    }
    const data = inspected.data || {}
    const billing = data.observations?.billing
    return {
      id: provider.id,
      name: provider.name || provider.id,
      enabled: provider.enabled,
      quota: {
        state: data.quota_suspension?.state,
        reason: data.quota_suspension?.reason_code,
        resetAt: data.quota_suspension?.reset_at,
        source: data.quota_suspension?.source,
        window: data.quota_suspension?.window,
      },
      observedAt: data.observations?.billing?.observation?.observed_at,
      // The vendor an account belongs to is a stable identity; the readings are
      // not, because the two sources observe at different moments.
      integration: data.provider?.integration,
      billing: billing === undefined ? undefined : {
        kind: billing.kind,
        state: billing.state,
        dashboard: billing.dashboard,
        freshness: billing.freshness,
        rows: (billing.display?.rows || []).map(row => ({
          label: row.label,
          basis: row.basis,
          percent: row.percent,
          counters: row.counters,
          reset: row.reset,
          status: row.status,
        })),
      },
    }
  }))
  return { providers }
}

/** Map tmax's DSH capabilities to the pi-ai model shape the adapter consumes. */
function modelFromProjection(route, model, providerName = route.name) {
  const caps = model.capabilities && typeof model.capabilities === 'object' ? model.capabilities : {}
  const efforts = caps.reasoningEfforts
  let thinkingLevelMap
  if (efforts && typeof efforts === 'object' && !Array.isArray(efforts)) {
    thinkingLevelMap = {}
    for (const level of THINKING_LEVELS) {
      const wire = efforts[level]
      thinkingLevelMap[level] = wire === undefined ? null : wire
    }
  }
  // pi-ai requires every model descriptor to carry a concrete modality list;
  // `PiAiAdapter.listModels()` spreads it unconditionally. Unknown modalities
  // fall back to text-only, which is conservative and does not claim vision.
  const input = Array.isArray(caps.input) && caps.input.length > 0 ? [...caps.input] : ['text']
  return {
    id: model.id,
    name: model.display_name || model.id,
    provider: providerName,
    api: API_NAMES[route.protocol],
    baseUrl: route.base_url,
    input,
    ...(typeof caps.contextWindow === 'number' ? { contextWindow: caps.contextWindow } : {}),
    // Output capacity is intentionally unmanaged: a model maxTokens becomes a
    // per-request default in DSH and would change generation budgets.
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    reasoning: thinkingLevelMap !== undefined,
    ...(thinkingLevelMap === undefined ? {} : { thinkingLevelMap }),
    compat: {},
  }
}

export async function apply(ctx, config = {}) {
  const tree = ctx.fiber.entry?.parent.tree
  if (tree === undefined) throw new Error('tmax requires a DSH Loader entry')

  const [native, llm, credentials, pi, chat, responses, messages, toolsModule] = await Promise.all([
    tree.import('@deepseek-ai/dsh-llm-pi-ai'),
    tree.import('@deepseek-ai/dsh-llm'),
    tree.import('@deepseek-ai/dsh-credentials'),
    tree.import('@earendil-works/pi-ai'),
    tree.import('@earendil-works/pi-ai/api/openai-completions.lazy'),
    tree.import('@earendil-works/pi-ai/api/openai-responses.lazy'),
    tree.import('@earendil-works/pi-ai/api/anthropic-messages.lazy'),
    tree.import('@deepseek-ai/dsh-tools'),
  ])
  const protocolApis = {
    'chat-completions': chat.openAICompletionsApi,
    'responses': responses.openAIResponsesApi,
    'messages': messages.anthropicMessagesApi,
  }

  let profiles = new Map()
  let registration
  let refreshing
  let lastInventory
  let lastError

  const manageRoutes = config.manageRoutes !== false
  const log = message => ctx.logger?.warn?.(`tmax: ${message}`)

  function rebuild(inventory) {
    const next = new Map()
    const prefix = typeof config.providerPrefix === 'string' ? config.providerPrefix : ''
    for (const route of inventory.routes || []) {
      const factory = protocolApis[route.protocol]
      if (factory === undefined) {
        log(`route "${route.name}" uses unsupported protocol "${route.protocol}"`)
        continue
      }
      const providerName = `${prefix}${route.name}`
      const models = (route.models || []).filter(model => {
        const contextWindow = model.capabilities?.contextWindow
        if (!Number.isInteger(contextWindow) || contextWindow <= 0) {
          log(`hiding model "${model.id}" on route "${route.name}": contextWindow is unknown`)
          return false
        }
        return true
      }).map(model => modelFromProjection(route, model, providerName))
      if (models.length === 0) continue
      const displayName = route.name
      const api = factory()
      const apiKeyEnv = credentials.credentialRef(route.credential_ref)
      next.set(providerName, {
        ...RESOLVED_PROFILE_DEFAULTS,
        apiKeyEnv,
        api: API_NAMES[route.protocol],
        provider: providerName,
        displayName,
        configuredMaxTokens: new Map(),
        modelErrors: new Map(),
        retryPolicy: llm.resolveRetryPolicy({ mode: 'normal', maxRetries: 0 }, `tmax: route "${providerName}" retryPolicy`),
        piProvider: {
          id: providerName,
          name: displayName,
          baseUrl: route.base_url,
          auth: {
            apiKey: {
              name: displayName,
              resolve: async ({ credential }) => ({ auth: { apiKey: credential?.key }, source: name }),
            },
          },
          getModels: () => models,
          stream: (model, context, request) => api.stream(model, context, request),
          streamSimple: (model, context, request) => api.streamSimple(model, context, request),
        },
      })
    }
    return next
  }

  const adapter = manageRoutes
    ? createAdapter({ native, llm, credentials, pi, ctx, profiles: () => profiles, label: 'tmax', log })
    : undefined

  async function refresh() {
    if (refreshing) return refreshing
    refreshing = (async () => {
      const inventory = await readInventory(config)
      lastInventory = inventory
      lastError = undefined
      const next = manageRoutes ? rebuild(inventory) : new Map()
      profiles = next
      const routes = [...next.keys()]
      if (!manageRoutes || routes.length === 0) {
        if (registration) {
          registration()
          registration = undefined
        }
        return inventory
      }
      if (registration === undefined) {
        registration = ctx.llm.registerAdapter(routes, adapter)
      } else {
        registration.replace(routes)
      }
      for (const route of routes) ctx.emit('llm/models-updated', route)
      return inventory
    })().catch(error => {
      lastError = error instanceof Error ? error.message : String(error)
      throw error
    }).finally(() => {
      refreshing = undefined
    })
    return refreshing
  }

  try {
    await refresh()
  } catch (error) {
    lastError = error instanceof Error ? error.message : String(error)
    log(`initial inventory unavailable: ${lastError}`)
  }
  const seconds = Number(config.refreshSeconds)
  const interval = Number.isFinite(seconds) && seconds >= 10 ? seconds : 60
  const timer = setInterval(() => {
    refresh().catch(error => log(error instanceof Error ? error.message : String(error)))
  }, interval * 1000)
  timer.unref?.()

  ctx.effect(() => () => {
    clearInterval(timer)
    registration?.()
    registration = undefined
    profiles = new Map()
  }, 'tmax: adapter and refresh')

  const webServer = ctx.get('webServer')
  if (webServer !== undefined) {
    const send = (res, status, payload) => {
      res.statusCode = status
      res.setHeader('content-type', 'application/json; charset=utf-8')
      res.setHeader('cache-control', 'no-store')
      res.end(JSON.stringify(payload))
    }
    const summary = () => ({
      target: lastInventory?.target,
      gateway: lastInventory?.gateway,
      routes: (lastInventory?.routes || []).map(route => ({
        name: route.name,
        protocol: route.protocol,
        models: (route.models || []).map(model => ({
          id: model.id,
          name: model.display_name || model.id,
          availability: model.availability,
        })),
      })),
      blockers: lastInventory?.blockers || [],
      lastError,
    })
    /**
   * The providers the agent itself is configured with, read through the live
   * settings and credential services. `apiKeyEnv` is a reference; the value
   * never leaves the resolver below.
   */
  const discoverAgentProviders = () => {
    const settings = ctx.get('settings')
    const credentials = ctx.get('credentials')
    if (settings === undefined || credentials === undefined) return undefined
    let descriptors
    try {
      descriptors = settings.describe({ redactSecrets: true })
    } catch {
      return undefined
    }
    const byNs = new Map()
    for (const descriptor of descriptors ?? []) byNs.set(String(descriptor.ns), descriptor.value)
    const providers = []
    for (const entry of ctx.llm.listConfigurableProviders()) {
      let node = byNs.get(String(entry.settingsNs))
      for (const key of entry.settingsPath) {
        node = node !== null && typeof node === 'object' ? Reflect.get(node, key) : undefined
      }
      const profile = node !== null && typeof node === 'object' ? node : {}
      // The directory lists every route the adapter *could* activate, including
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
        vendor: (vendorFor(baseURL) ?? vendorForKey(entry.provider))?.id,
        baseURL,
        providerKey: entry.provider,
        apiKeyEnv: typeof profile.apiKeyEnv === 'string' ? profile.apiKeyEnv : undefined,
        models: Array.isArray(profile.models) ? profile.models.map(model => model?.id).filter(Boolean) : [],
        defaultBaseURL: entry.defaultBaseURL,
      })
    }
    return { providers, credentials }
  }

  // Reading a provider the agent configures costs the same live request it
  // costs tmax, so it obeys the same rule: serve the cache until it is older
  // than the stale mark, and let a deliberate Refresh bypass it.
  const AGENT_CACHE_MS = 5 * 60 * 1000
  let agentCache = { at: 0, value: undefined }
  const readAgentQuota = async ({ force = false } = {}) => {
    if (!force && agentCache.value !== undefined && Date.now() - agentCache.at < AGENT_CACHE_MS) {
      return { ...agentCache.value, cached: true }
    }
    const discovered = discoverAgentProviders()
    if (discovered === undefined) return { ok: true, providers: [], reason: 'agent-settings-unavailable' }
    const { providers, credentials } = discovered
    const byAccount = new Map()
    const records = await Promise.all(providers.map(async provider => {
      const identity = `${provider.vendor ?? 'unknown'}::${provider.apiKeyEnv ?? provider.provider}`
      const shared = byAccount.get(identity)
      // The reading is shared; the identity is not — two configuration entries
      // remain two cards, each with its own id.
      if (shared !== undefined) return { ...shared, id: `dsh:${provider.provider}` }
      const record = await observeProvider({
        provider: provider.provider,
        baseURL: provider.baseURL,
        apiKeyEnv: provider.apiKeyEnv,
        resolveKey: async ref => (await credentials.resolve(ref))?.value,
      })
      byAccount.set(identity, record)
      return record
    }))
    const value = {
      ok: true,
      generatedAt: new Date().toISOString(),
      providers: records.map((record, index) => ({
        ...record,
        displayName: providers[index].displayName,
        models: providers[index].models,
      })),
    }
    agentCache = { at: Date.now(), value }
    return value
  }

  ctx.effect(() => webServer.register({
    kind: 'exact',
    path: '/_tmax/dsh-usage',
    handler: async (req, res) => {
      if (req.method !== 'GET') { send(res, 405, { code: 'method-not-allowed' }); return }
      const force = req.url !== undefined && /[?&]force=1(&|$)/.test(req.url)
      try { send(res, 200, await readAgentQuota({ force })) }
      catch (error) { send(res, 502, { ok: false, error: error instanceof Error ? error.message : String(error) }) }
    },
  }), 'tmax: GET /_tmax/dsh-usage')
  ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/status',
      handler: (req, res) => {
        if (req.method !== 'GET') { send(res, 405, { code: 'method-not-allowed' }); return }
        send(res, 200, summary())
      },
    }), 'tmax: GET /_tmax/status')
    ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/usage',
      handler: async (req, res) => {
        if (req.method !== 'GET') { send(res, 405, { code: 'method-not-allowed' }); return }
        try { send(res, 200, { ok: true, ...(await readUsage(config)) }) }
        catch (error) { send(res, 502, { ok: false, error: error instanceof Error ? error.message : String(error) }) }
      },
    }), 'tmax: GET /_tmax/usage')
    ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/usage/refresh',
      handler: async (req, res) => {
        if (req.method !== 'POST') { send(res, 405, { code: 'method-not-allowed' }); return }
        try {
          const before = await readUsage(config)
          // Quota checks reach the provider, so only providers that already
          // report a billing observation are asked; the rest have nothing to
          // refresh and would only cost a failed round trip.
          const candidates = before.providers.filter(provider =>
            provider.billing?.state === 'observed' && provider.error === undefined)
          await Promise.all(candidates.map(provider =>
            runTmax(config, ['provider', 'refresh', String(provider.id), '--scope', 'quota'])
              .catch(error => log(`quota refresh failed for "${provider.id}": ${error instanceof Error ? error.message : String(error)}`))))
          send(res, 200, { ok: true, ...(await readUsage(config)) })
        } catch (error) {
          send(res, 502, { ok: false, error: error instanceof Error ? error.message : String(error) })
        }
      },
    }), 'tmax: POST /_tmax/usage/refresh')
    ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/connect',
      handler: async (req, res) => {
        if (req.method !== 'POST') { send(res, 405, { code: 'method-not-allowed' }); return }
        try {
          const body = await readJsonBody(req)
          const preview = previewRequested(req, body)
          const target = await resolveTarget(config)
          const envelope = await runTmax(config, ['agent', 'apply', target, ...(preview ? ['--dry-run'] : [])])
          if (envelope.status !== 'success' && envelope.status !== 'dry_run') {
            const blocker = Array.isArray(envelope.blockers) ? envelope.blockers[0] : undefined
            send(res, 409, {
              ok: false,
              code: envelope.code || 'connect_failed',
              message: envelope.message || blocker?.message || 'tmax agent apply failed',
              ...summary(),
            })
            return
          }
          if (!preview) await refresh()
          send(res, 200, { ok: true, connect: envelope.data, ...summary() })
        } catch (error) {
          if (error instanceof Error && error.actionable === true) {
            send(res, 409, { ok: false, code: error.code || 'connect_failed', message: error.message, ...summary() })
            return
          }
          send(res, 502, { ok: false, code: 'connect_unavailable', message: error instanceof Error ? error.message : String(error), ...summary() })
        }
      },
    }), 'tmax: POST /_tmax/connect')
  }

  const tools = ctx.get('tools')
  if (tools) {
    ctx.effect(() => tools.register(toolsModule.defineTool({
      name: 'tmax_status',
      description: 'Read tmax model-access state: the DSH target, gateway readiness, route models, and blockers. Use it before changing model access or explaining a tmax failure.',
      parameters: {},
      output: {
        schema: { type: 'object', additionalProperties: true },
        render: (_args, value) => [{ type: 'text', text: JSON.stringify(value, null, 2) }],
      },
      async execute() {
        try {
          const inventory = await refresh()
          return {
            target: inventory.target?.id,
            gateway: inventory.gateway,
            routes: (inventory.routes || []).map(route => ({
              name: route.name,
              protocol: route.protocol,
              models: route.models.map(model => model.id),
            })),
            blockers: inventory.blockers || [],
          }
        } catch (error) {
          return {
            target: undefined,
            gateway: undefined,
            routes: [],
            blockers: [{
              code: 'tmax_unavailable',
              message: error instanceof Error ? error.message : String(error),
            }],
          }
        }
      },
      presentCall: () => ({ card: 'generic', title: 'tmax status', kind: 'read', rawInput: {} }),
    })), 'tmax: status tool')
  }

  const skills = ctx.get('skills')
  if (skills) {
    ctx.effect(() => skills.register({
      name: 'tmax',
      description: 'Use tmax-managed model access and inspect tmax status, subscriptions, quotas, aliases, and routing evidence.',
      content: [
        'When the user asks about model access, subscriptions, quotas, accounts, aliases, routing, or tmax configuration, start with `tmax_status`.',
        'tmax CLI help is authoritative for syntax and operands. Prefer explicit operations; use `--dry-run` before mutations.',
        'Never put keys or tokens in tool arguments, commands, logs, or chat. Credentials remain a user action through tmax protected input or sign-in; this plugin only uses DSH credential references.',
        'Do not infer quota or availability from a missing value. Report observed values with their source and time; say unknown when unknown.',
        'If tmax is unavailable or the gateway is not ready, report the blocker and the next action; do not claim success or silently switch models.',
      ].join('\n'),
      invocation: { modelInvocable: true, userInvocable: true },
    }), 'tmax: tmax skill')
  }

  ctx.logger?.info?.(
    `tmax: ${lastInventory?.routes?.length || 0} route(s), ${lastInventory?.routes?.reduce((count, route) => count + route.models.length, 0) || 0} model(s)${lastError ? `; last error: ${lastError}` : ''}`,
  )
}
