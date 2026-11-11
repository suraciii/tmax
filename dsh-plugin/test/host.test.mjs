import assert from 'node:assert/strict'
import { chmod, mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { apply } from '../index.js'

const projection = {
  schema_version: 1,
  authority: '/tmp/tmax',
  target: { id: 'dsh-main', path: '/tmp/dsh' },
  gateway: { configured: true, ready: true, listen: '127.0.0.1:17840', reason_code: null },
  routes: [{
    name: 'tmax',
    protocol: 'responses',
    base_url: 'http://127.0.0.1:17840',
    credential_ref: 'TMAX_TEST',
    models: [{
      id: 'public-model',
      display_name: 'Public Model',
      protocol: 'responses',
      kind: 'alias',
      continuity: 'stateless',
      availability: 'unverified',
      candidate_count: 1,
      capabilities: {
        contextWindow: 200000,
        input: ['text', 'image'],
        reasoningEfforts: { low: 'low', high: 'high' },
      },
    }, {
      id: 'text-only',
      display_name: 'Text Only',
      protocol: 'responses',
      kind: 'concrete',
      availability: 'unverified',
      capabilities: { contextWindow: 1000 },
    }, {
      id: 'no-context',
      display_name: 'No Context',
      protocol: 'responses',
      kind: 'concrete',
      availability: 'unverified',
      capabilities: {},
    }],
  }],
  blockers: [],
}

async function fakeTmax(body) {
  const directory = await mkdtemp(join(tmpdir(), 'tmax-test-'))
  const path = join(directory, 'tmax')
  await writeFile(path, `#!/bin/sh\nprintf '%s\\n' '${JSON.stringify(body).replaceAll("'", "'\\''")}'\n`)
  await chmod(path, 0o755)
  return path
}

function fakeContext() {
  const state = { routes: [], tools: [], skills: [], events: [], effects: [], adapterOptions: undefined, httpRoutes: [] }
  const modules = {
    '@deepseek-ai/dsh-llm-pi-ai': {
      PiAiAdapter: class {
        constructor(options) { state.adapterOptions = options }
        stream() { throw new Error('not called') }
      },
    },
    '@deepseek-ai/dsh-llm': {
      LlmError: class extends Error {},
      assertUsableApiKey: value => value,
      resolveRetryPolicy: () => ({ mode: 'normal', maxRetries: 0 }),
      resolveImageAttachmentAccess: () => undefined,
    },
    '@deepseek-ai/dsh-credentials': { credentialRef: ref => ref },
    '@earendil-works/pi-ai': { InMemoryCredentialStore: class {} },
    '@earendil-works/pi-ai/api/openai-completions.lazy': { openAICompletionsApi: () => ({ stream() {}, streamSimple() {} }) },
    '@earendil-works/pi-ai/api/openai-responses.lazy': { openAIResponsesApi: () => ({ stream() {}, streamSimple() {} }) },
    '@earendil-works/pi-ai/api/anthropic-messages.lazy': { anthropicMessagesApi: () => ({ stream() {}, streamSimple() {} }) },
    '@deepseek-ai/dsh-tools': { defineTool: definition => definition },
  }
  const ctx = {
    fiber: { entry: { parent: { tree: { import: async specifier => modules[specifier] } } } },
    logger: { info() {}, warn() {} },
    get(name) {
      if (name === 'credentials') return { credentialRef: ref => ref, resolve: async () => ({ value: 'test-key' }) }
      if (name === 'tools') return { register(definition) { state.tools.push(definition); return () => {} } }
      if (name === 'skills') return { register(definition) { state.skills.push(definition); return () => {} } }
      if (name === 'attachments') return undefined
      if (name === 'fs') return undefined
      if (name === 'webServer') return { register(route) { state.httpRoutes.push(route); return () => {} } }
      if (name === 'connection') return { requestRejection: () => undefined }
      return undefined
    },
    llm: {
      registerAdapter(routes, adapter) {
        state.routes = [...routes]
        state.adapter = adapter
        const handle = () => { state.routes = [] }
        handle.replace = next => { state.routes = [...next] }
        return handle
      },
    },
    emit(event, payload) { state.events.push([event, payload]) },
    effect(callback) { state.effects.push(callback()) },
  }
  return { ctx, state }
}

test('registers one dynamic route, a status tool, and a skill without exposing maxTokens', async () => {
  const tmaxPath = await fakeTmax({ code: 'ok', status: 'success', data: projection })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath, providerPrefix: 'tmaxp-', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, ['tmaxp-tmax'])
  const profiles = state.adapterOptions.profiles()
  const profile = profiles.get('tmaxp-tmax')
  assert.equal(profile.displayName, 'tmax')
  // The adapter reads resolved scalars directly; a missing idle timeout fails
  // every stream at `idleWatchdog`.
  for (const [key, value] of Object.entries({
    streamIdleTimeoutMs: 300_000,
    maxRequestImageBytes: 20 * 1024 * 1024,
    maxRequestImages: 50,
    imageOffloadCountQuantum: 10,
    requestImagePixelBudget: 2048 * 2048,
    requestImageMaxBytes: 1024 * 1024,
  })) {
    assert.equal(profile[key], value, key)
  }
  const models = profile.piProvider.getModels()
  assert.equal(models.length, 2)
  assert.equal(models[0].id, 'public-model')
  assert.equal(models[0].provider, 'tmaxp-tmax')
  assert.equal(models[0].api, 'openai-responses')
  assert.equal(models[0].contextWindow, 200000)
  assert.deepEqual(models[0].thinkingLevelMap, { off: null, minimal: null, low: 'low', medium: null, high: 'high', xhigh: null, max: null })
  assert.equal(models[0].maxTokens, undefined)
  assert.deepEqual(models[1].input, ['text'])
  assert.equal(profile.streamIdleTimeoutMs, 300000)
  assert.equal(profile.maxRequestImageBytes, 20 * 1024 * 1024)
  assert.equal(profile.maxRequestImages, 50)
  assert.equal(profile.imageOffloadCountQuantum, 10)
  assert.equal(profile.requestImagePixelBudget, 2048 * 2048)
  assert.equal(profile.requestImageMaxBytes, 1024 * 1024)
  assert.equal(state.tools[0].name, 'tmax_status')
  assert.equal(state.skills[0].name, 'tmax')
  assert.deepEqual(state.httpRoutes.map(route => route.path), ['/_tmax/status', '/_tmax/usage', '/_tmax/usage/refresh', '/_tmax/connect'])
  const status = await state.tools[0].execute()
  assert.equal(status.target, 'dsh-main')
  assert.deepEqual(status.routes, [{ name: 'tmax', protocol: 'responses', models: ['public-model', 'text-only', 'no-context'] }])
})

test('a missing tmax does not fail activation and the status tool reports the blocker', async () => {
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: '/definitely/missing/tmax', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, [])
  const status = await state.tools[0].execute()
  assert.equal(status.blockers[0].code, 'tmax_unavailable')
})

test('http routes expose status and run connect without returning a key', async () => {
  const tmaxPath = await fakeTmax({ code: 'ok', status: 'success', data: projection })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath, providerPrefix: 'tmaxp-', refreshSeconds: 3600 })
  const statusRoute = state.httpRoutes.find(route => route.path === '/_tmax/status')
  const connectRoute = state.httpRoutes.find(route => route.path === '/_tmax/connect')
  let statusBody
  statusRoute.handler({ method: 'GET', headers: {} }, {
    setHeader() {}, end(value) { statusBody = JSON.parse(value) },
  })
  assert.equal(statusBody.routes[0].name, 'tmax')
  assert.equal(statusBody.routes[0].models.length, 3)
  let connectBody
  await connectRoute.handler({ method: 'POST', headers: {} }, {
    setHeader() {}, end(value) { connectBody = JSON.parse(value) },
  })
  assert.equal(connectBody.ok, true)
  assert.equal(connectBody.connect.schema_version, 1)
  assert.equal(JSON.stringify(connectBody).includes('gateway-key'), false)
})
