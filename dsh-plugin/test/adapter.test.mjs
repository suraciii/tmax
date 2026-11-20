import assert from 'node:assert/strict'
import { copyFile, mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { pathToFileURL } from 'node:url'
import { apply, RESOLVED_PROFILE_DEFAULTS } from '../adapter.js'

function fakeModules() {
  return {
    '@deepseek-ai/dsh-llm-pi-ai': {
      Config: options => ({
        providers: {
          get: () => Object.fromEntries(Object.entries(options.providers ?? {}).map(([name, source]) => [name, {
            ...RESOLVED_PROFILE_DEFAULTS,
            defaultInput: ['text'],
            ...source,
          }])),
        },
      }),
      PiAiAdapter: class {
        constructor(options) { this.options = options }
      },
    },
    '@deepseek-ai/dsh-llm': {
      LlmError: class extends Error {
        constructor(message, code) {
          super(message)
          this.code = code
        }
      },
      resolveRetryPolicy: () => ({ mode: 'normal', maxRetries: 0 }),
      resolveImageAttachmentAccess: () => undefined,
    },
    '@deepseek-ai/dsh-credentials': {
      credentialRef: ref => ref,
      normalizeApiKey: raw => {
        const value = raw.trim()
        if (value.length === 0) return { ok: false, reason: 'empty' }
        return /^[\x21-\x7E]+$/.test(value)
          ? { ok: true, value }
          : { ok: false, reason: 'illegalCharacters' }
      },
    },
    '@earendil-works/pi-ai': { InMemoryCredentialStore: class {} },
    '@earendil-works/pi-ai/api/openai-completions.lazy': { openAICompletionsApi: () => ({ stream() {}, streamSimple() {} }) },
    '@earendil-works/pi-ai/api/openai-responses.lazy': { openAIResponsesApi: () => ({ stream() {}, streamSimple() {} }) },
    '@earendil-works/pi-ai/api/anthropic-messages.lazy': { anthropicMessagesApi: () => ({ stream() {}, streamSimple() {} }) },
  }
}

function fakeContext({ credential = 'test-key' } = {}) {
  const state = { routes: [], adapterOptions: undefined, warnings: [] }
  const modules = fakeModules()
  return {
    state,
    ctx: {
      fiber: { entry: { parent: { tree: { import: async specifier => modules[specifier] } } } },
      get: name => name === 'credentials'
        ? { resolve: async () => credential == null ? undefined : { value: credential } }
        : undefined,
      logger: { warn: message => state.warnings.push(message) },
      llm: {
        registerAdapter(routes, adapter) {
          state.routes = [...routes]
          state.adapterOptions = adapter.options
          return () => { state.routes = [] }
        },
      },
    },
  }
}

const options = {
  providers: {
    local: {
      api: 'openai-responses',
      baseURL: 'http://127.0.0.1:9',
      apiKeyEnv: 'TMAX_TEST',
      displayName: 'Local',
      models: [{ id: 'public-model', input: ['text', 'image'], contextWindow: 4096 }],
    },
  },
}

test('home mode maps llm-tmax routes onto the shared adapter', async () => {
  const { ctx, state } = fakeContext()
  await apply(ctx, options)

  assert.deepEqual(state.routes, ['local'])
  const profile = state.adapterOptions.profiles().get('local')
  assert.equal(profile.displayName, 'Local')
  assert.equal(profile.apiKeyEnv, 'TMAX_TEST')
  assert.equal(profile.streamIdleTimeoutMs, 300_000)
  const model = profile.piProvider.getModels()[0]
  assert.equal(model.id, 'public-model')
  assert.equal(model.api, 'openai-responses')
  assert.equal(model.baseUrl, 'http://127.0.0.1:9')
  assert.deepEqual(model.input, ['text', 'image'])
  assert.equal(model.contextWindow, 4096)
  assert.equal(await state.adapterOptions.resolveApiKey('local', profile), 'test-key')
})

test('home mode rejects an unrepresentable route before registering it', async () => {
  const { ctx, state } = fakeContext()
  await assert.rejects(
    apply(ctx, { providers: { local: { ...options.providers.local, api: 'unsupported' } } }),
    error => error.code === 'INVALID_CONFIG' && /supported api/.test(error.message),
  )
  assert.deepEqual(state.routes, [])
})

test('home mode names the missing credential with its own plugin label', async () => {
  const { ctx, state } = fakeContext({ credential: null })
  await apply(ctx, options)
  const profile = state.adapterOptions.profiles().get('local')
  await assert.rejects(
    state.adapterOptions.resolveApiKey('local', profile),
    error => error.code === 'MISSING_CREDENTIAL' && /^llm-tmax: no credential/.test(error.message),
  )
})

test('the home artifact loads as a standalone file', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'tmax-dsh-'))
  try {
    const file = join(directory, 'tmax-dsh.mjs')
    await copyFile(new URL('../adapter.js', import.meta.url), file)
    const standalone = await import(pathToFileURL(file).href)
    assert.equal(standalone.name, 'llm-tmax')
    assert.deepEqual(standalone.inject, ['llm'])
    assert.equal(typeof standalone.apply, 'function')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
})
