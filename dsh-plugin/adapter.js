// The one DSH adapter implementation.
//
// This file has two consumers. The profile plugin imports `createAdapter`,
// `THINKING_LEVELS`, and the resolved-profile defaults; tmax embeds the whole
// file as DSH_HOME/tmax-dsh.mjs, where the top-level `name`, `inject`, and
// `apply` make it the independent home plugin. Keep it self-contained: the
// installed copy has no sibling modules to import.
//
// Home mode resolves dependencies through the owning Loader tree so the active
// profile's runtime selects the same public DSH packages as its other adapters.
export const name = 'llm-tmax'
export const inject = ['llm']

export const THINKING_LEVELS = ['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max']

// `PiAiAdapter` reads these resolved scalars on every stream; `llm-pi-ai` does
// not export its defaults, so the profile plugin mirrors them. Values match
// `@deepseek-ai/dsh-llm-pi-ai` 0.2.2-rc.5 (src/config.ts): a missing
// `streamIdleTimeoutMs` reaches `idleWatchdog` and fails the stream.
export const RESOLVED_PROFILE_DEFAULTS = {
  streamIdleTimeoutMs: 300_000,
  maxRequestImageBytes: 20 * 1024 * 1024,
  maxRequestImages: 50,
  imageOffloadCountQuantum: 10,
  requestImagePixelBudget: 2048 * 2048,
  requestImageMaxBytes: 1024 * 1024,
}

// The native schema materializes absent template argument dictionaries as {}.
// Empty dictionaries declare no override, just as in DSH's catalog resolver.
function configuredCompat(compat) {
  return Object.fromEntries(Object.entries(compat ?? {}).filter(([, value]) =>
    value === null || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length !== 0,
  ))
}

/**
 * Build the adapter over already-resolved profiles.
 * @param {object} dependencies - native modules and the live context.
 * @param {string} label - diagnostic prefix, `tmax` or `llm-tmax`.
 * @param {(message: string) => void} log - warning sink.
 */
export function createAdapter({ native, llm, credentials, pi, ctx, profiles, label, log }) {
  return new native.PiAiAdapter({
    profiles,
    resolveApiKey: async (provider, profile) => {
      const hit = await ctx.get('credentials')?.resolve(profile.apiKeyEnv)
      if (hit?.value?.length) {
        const checked = credentials.normalizeApiKey(hit.value)
        if (checked.ok) return checked.value
        throw new llm.LlmError(
          `${label}: credential ${profile.apiKeyEnv} for route "${provider}" is unusable (${checked.reason})`,
          'INVALID_CREDENTIAL',
        )
      }
      throw new llm.LlmError(
        `${label}: no credential for route "${provider}"; resolve ${profile.apiKeyEnv} through the DSH credentials service`,
        'MISSING_CREDENTIAL',
      )
    },
    // Routes use only explicit reference credentials, never provider login
    // records. A real in-memory store satisfies the native collection contract
    // without reading another plugin's grants or any credential files.
    auth: {
      credentials: new pi.InMemoryCredentialStore(),
      authContext: {
        env: async ref => (await ctx.get('credentials')?.resolve(ref))?.value,
        fileExists: async () => { throw new Error(`${label} does not use credential files`) },
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
}

/** Home-mode entry: build profiles from the managed `llm-tmax` configuration. */
export async function apply(ctx, options) {
  const tree = ctx.fiber.entry?.parent.tree
  if (tree === undefined) throw new Error('llm-tmax requires a DSH Loader entry')
  const [native, llm, credentials, pi, chat, responses, messages] = await Promise.all([
    tree.import('@deepseek-ai/dsh-llm-pi-ai'),
    tree.import('@deepseek-ai/dsh-llm'),
    tree.import('@deepseek-ai/dsh-credentials'),
    tree.import('@earendil-works/pi-ai'),
    tree.import('@earendil-works/pi-ai/api/openai-completions.lazy'),
    tree.import('@earendil-works/pi-ai/api/openai-responses.lazy'),
    tree.import('@earendil-works/pi-ai/api/anthropic-messages.lazy'),
  ])
  const protocols = {
    'openai-completions': chat.openAICompletionsApi,
    'openai-responses': responses.openAIResponsesApi,
    'anthropic-messages': messages.anthropicMessagesApi,
  }
  // Reuse the native schema for modalities, capacities, reasoning levels,
  // credential-reference annotations and adapter-owned stream/image defaults.
  // The outer Loader entry deliberately has no volatile schema: config changes
  // remount this fiber and dispose its registrations through native ctx.effect.
  const config = native.Config(structuredClone(options ?? {}))
  const profiles = new Map()
  const log = message => ctx.logger?.warn?.(`llm-tmax: ${message}`)
  const invalid = message => { throw new llm.LlmError(`llm-tmax: ${message}`, 'INVALID_CONFIG') }
  for (const [provider, source] of Object.entries(config.providers.get())) {
    if (provider.length === 0) invalid('provider route names must be non-empty')
    const factory = protocols[source.api]
    if (factory === undefined) invalid(`route "${provider}" must name a supported api`)
    if (!source.baseURL) invalid(`route "${provider}" must name its baseURL`)
    if (!source.apiKeyEnv) invalid(`route "${provider}" must name its apiKeyEnv credential reference`)
    const apiKeyEnv = credentials.credentialRef(source.apiKeyEnv)
    if (!source.models?.length) invalid(`route "${provider}" must list its models`)
    const displayName = source.displayName ?? provider
    const seen = new Set()
    const configuredMaxTokens = new Map()
    const models = source.models.map(entry => {
      if (!entry.id || seen.has(entry.id)) invalid(`route "${provider}" has an empty or duplicate model id`)
      seen.add(entry.id)
      if (entry.maxTokens !== undefined) configuredMaxTokens.set(entry.id, entry.maxTokens)
      const efforts = entry.reasoningEfforts
      let thinkingLevelMap
      if (efforts !== undefined && efforts !== false) {
        if (efforts === null || !Object.keys(efforts).some(level => level !== 'off')) {
          invalid(`route "${provider}" model "${entry.id}" must declare a thinking effort beyond off`)
        }
        thinkingLevelMap = {}
        for (const level of THINKING_LEVELS) {
          const wire = efforts[level]
          if (wire === null && level === 'off') continue
          if (wire !== undefined && (typeof wire !== 'string' || wire.length === 0)) {
            invalid(`route "${provider}" model "${entry.id}" has an invalid reasoningEfforts.${level}`)
          }
          thinkingLevelMap[level] = wire ?? null
        }
      }
      return {
        id: entry.id,
        name: entry.name ?? entry.id,
        provider,
        api: source.api,
        baseUrl: source.baseURL,
        input: entry.input?.length ? [...entry.input] : [...source.defaultInput],
        contextWindow: entry.contextWindow ?? source.defaultContextWindow,
        maxTokens: entry.maxTokens ?? source.defaultMaxTokens,
        // DSH does not report spend; zero here means no declared pricing.
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        reasoning: thinkingLevelMap !== undefined,
        ...(thinkingLevelMap === undefined ? {} : { thinkingLevelMap }),
        compat: { ...configuredCompat(source.compat), ...configuredCompat(entry.compat) },
      }
    })
    const api = factory()
    const piProvider = {
      id: provider,
      name: displayName,
      baseUrl: source.baseURL,
      // PiAiAdapter resolves the reference before dispatch; this method only
      // presents that explicit override to pi-ai. No ambient key discovery.
      auth: { apiKey: {
        name: displayName,
        resolve: async ({ credential }) => ({ auth: { apiKey: credential?.key }, source: name }),
      } },
      getModels: () => models,
      stream: (model, context, request) => api.stream(model, context, request),
      streamSimple: (model, context, request) => api.streamSimple(model, context, request),
    }
    profiles.set(provider, {
      ...source,
      apiKeyEnv,
      provider,
      displayName,
      piProvider,
      modelErrors: new Map(),
      configuredMaxTokens,
      retryPolicy: llm.resolveRetryPolicy(source.retryPolicy, `llm-tmax: route "${provider}" retryPolicy`),
    })
  }
  const adapter = createAdapter({
    native, llm, credentials, pi, ctx, profiles: () => profiles, label: 'llm-tmax', log,
  })
  // Do not apply the llm-pi-ai plugin: it declares the entire builtin provider
  // directory and login flows. The shared registry must retain user adapters.
  if (profiles.size) ctx.llm.registerAdapter([...profiles.keys()], adapter)
}
