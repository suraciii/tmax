// Host half: project tmax's DSH gateway access into the live LLM registry.
//
// The plugin is deliberately thin: tmax owns provider/model/alias/quota facts;
// this module only reads `tmax dsh projection` and maps it onto DSH's adapter,
// tool, and skill surfaces. No credential value ever enters this module.
import { execFile } from 'node:child_process'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'

export const name = 'tmax'
export const inject = ['llm']

const execFileAsync = promisify(execFile)
const THINKING_LEVELS = ['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max']
const API_NAMES = {
  'chat-completions': 'openai-completions',
  'responses': 'openai-responses',
  'messages': 'anthropic-messages',
}

// `PiAiAdapter` reads these resolved scalars on every stream; `llm-pi-ai` does
// not export its defaults, so this plugin mirrors them. Values match
// `@deepseek-ai/dsh-llm-pi-ai` 0.2.2-rc.1 (src/config.ts): a missing
// `streamIdleTimeoutMs` reaches `idleWatchdog` and fails the stream.
const RESOLVED_PROFILE_DEFAULTS = {
  streamIdleTimeoutMs: 300_000,
  maxRequestImageBytes: 20 * 1024 * 1024,
  maxRequestImages: 50,
  imageOffloadCountQuantum: 10,
  requestImagePixelBudget: 2048 * 2048,
  requestImageMaxBytes: 1024 * 1024,
}

/** Read one tmax CLI envelope, or fail with its code and message. */
async function readProjection(config) {
  const tmax = config.tmaxPath || 'tmax'
  const args = ['dsh', 'projection']
  if (config.target) args.push(String(config.target))
  else {
    const home = config.home || process.env.DSH_HOME || join(homedir(), '.dsh')
    args.push('--home', home)
  }
  const { stdout } = await execFileAsync(tmax, args, {
    timeout: 10_000,
    maxBuffer: 2 * 1024 * 1024,
    env: process.env,
  })
  const envelope = JSON.parse(stdout)
  if (envelope.code !== 'ok' || envelope.status !== 'success') {
    throw new Error(`tmax projection failed: ${envelope.code || 'unknown'}${envelope.message ? ` (${envelope.message})` : ''}`)
  }
  return envelope.data
}

/** Run one tmax command and parse its JSON envelope. Never logs a key. */
async function runTmax(config, args) {
  const tmax = config.tmaxPath || 'tmax'
  const { stdout } = await execFileAsync(tmax, args, {
    timeout: 20_000,
    maxBuffer: 2 * 1024 * 1024,
    env: process.env,
  })
  return JSON.parse(stdout)
}

/** Arguments selecting the configured DSH target. */
function targetArgs(config) {
  if (config.target) return [String(config.target)]
  const home = config.home || process.env.DSH_HOME || join(homedir(), '.dsh')
  return ['--home', home]
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
  let lastProjection
  let lastError

  const log = message => ctx.logger?.warn?.(`tmax: ${message}`)

  function rebuild(projection) {
    const next = new Map()
    const prefix = typeof config.providerPrefix === 'string' ? config.providerPrefix : ''
    for (const route of projection.routes || []) {
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

  const adapter = new native.PiAiAdapter({
    profiles: () => profiles,
    resolveApiKey: async (provider, profile) => {
      const hit = await ctx.get('credentials')?.resolve(profile.apiKeyEnv)
      if (hit?.value?.length) return llm.assertUsableApiKey(hit.value, name, profile.apiKeyEnv)
      throw new llm.LlmError(
        `tmax: no credential for route "${provider}"; resolve ${profile.apiKeyEnv} through the DSH credentials service`,
        'MISSING_CREDENTIAL',
      )
    },
    auth: {
      credentials: new pi.InMemoryCredentialStore(),
      authContext: {
        env: async ref => (await ctx.get('credentials')?.resolve(ref))?.value,
        fileExists: async () => { throw new Error('tmax does not use credential files') },
      },
    },
    resolveAttachments: () => ctx.get('attachments'),
    resolveImageAccess: (attachments, ref) => llm.resolveImageAttachmentAccess(
      attachments,
      hostPath => ctx.get('fs')?.processPathFromHostPath(hostPath),
      ref,
    ),
    onReplayDegrade: ({ provider, model, reason }) => log(
      `unusable replay state for route "${provider}/${model}"; sending provider-neutral content (${reason})`,
    ),
  })

  async function refresh() {
    if (refreshing) return refreshing
    refreshing = (async () => {
      const projection = await readProjection(config)
      lastProjection = projection
      lastError = undefined
      const next = rebuild(projection)
      profiles = next
      const routes = [...next.keys()]
      if (routes.length === 0) {
        if (registration) {
          registration()
          registration = undefined
        }
        return projection
      }
      if (registration === undefined) {
        registration = ctx.llm.registerAdapter(routes, adapter)
      } else {
        registration.replace(routes)
      }
      for (const route of routes) ctx.emit('llm/models-updated', route)
      return projection
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
    log(`initial projection unavailable: ${lastError}`)
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
  const connection = ctx.get('connection')
  if (webServer !== undefined && connection !== undefined) {
    const send = (res, status, payload) => {
      res.statusCode = status
      res.setHeader('content-type', 'application/json; charset=utf-8')
      res.setHeader('cache-control', 'no-store')
      res.end(JSON.stringify(payload))
    }
    const rejected = (req, res) => {
      const rejection = connection.requestRejection(req)
      if (rejection === undefined) return false
      res.statusCode = rejection
      res.end()
      return true
    }
    const summary = () => ({
      target: lastProjection?.target,
      gateway: lastProjection?.gateway,
      routes: (lastProjection?.routes || []).map(route => ({
        name: route.name,
        protocol: route.protocol,
        models: (route.models || []).map(model => ({
          id: model.id,
          name: model.display_name || model.id,
          kind: model.kind,
          availability: model.availability,
        })),
      })),
      blockers: lastProjection?.blockers || [],
      lastError,
    })
    ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/status',
      handler: (req, res) => {
        if (rejected(req, res)) return
        if (req.method !== 'GET') { send(res, 405, { code: 'method-not-allowed' }); return }
        send(res, 200, summary())
      },
    }), 'tmax: GET /_tmax/status')
    ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/usage',
      handler: async (req, res) => {
        if (rejected(req, res)) return
        if (req.method !== 'GET') { send(res, 405, { code: 'method-not-allowed' }); return }
        try { send(res, 200, { ok: true, ...(await readUsage(config)) }) }
        catch (error) { send(res, 502, { ok: false, error: error instanceof Error ? error.message : String(error) }) }
      },
    }), 'tmax: GET /_tmax/usage')
    ctx.effect(() => webServer.register({
      kind: 'exact',
      path: '/_tmax/connect',
      handler: async (req, res) => {
        if (rejected(req, res)) return
        if (req.method !== 'POST') { send(res, 405, { code: 'method-not-allowed' }); return }
        try {
          const envelope = await runTmax(config, ['dsh', 'connect', ...targetArgs(config)])
          if (envelope.code !== 'ok' || envelope.status !== 'success') {
            send(res, 409, { ok: false, code: envelope.code || 'connect_failed', message: envelope.message || 'tmax connect failed', ...summary() })
            return
          }
          await refresh()
          send(res, 200, { ok: true, connect: envelope.data, ...summary() })
        } catch (error) {
          send(res, 502, { ok: false, code: 'connect_unavailable', message: error instanceof Error ? error.message : String(error), ...summary() })
        }
      },
    }), 'tmax: POST /_tmax/connect')
  }

  const tools = ctx.get('tools')
  if (tools) {
    ctx.effect(() => tools.register(toolsModule.defineTool({
      name: 'tmax_status',
      description: 'Read tmax model-access state: authority, gateway readiness, projected routes/models, and blockers. Use it before changing model access or explaining a tmax failure.',
      parameters: {},
      output: {
        schema: { type: 'object', additionalProperties: true },
        render: (_args, value) => [{ type: 'text', text: JSON.stringify(value, null, 2) }],
      },
      async execute() {
        try {
          const projection = await refresh()
          return {
            authority: projection.authority,
            target: projection.target?.id,
            gateway: projection.gateway,
            routes: (projection.routes || []).map(route => ({
              name: route.name,
              protocol: route.protocol,
              models: route.models.map(model => model.id),
            })),
            blockers: projection.blockers || [],
          }
        } catch (error) {
          return {
            authority: '',
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
    `tmax: ${lastProjection?.routes?.length || 0} route(s), ${lastProjection?.routes?.reduce((count, route) => count + route.models.length, 0) || 0} model(s)${lastError ? `; last error: ${lastError}` : ''}`,
  )
}
