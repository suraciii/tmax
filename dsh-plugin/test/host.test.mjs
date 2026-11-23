import assert from 'node:assert/strict'
import { chmod, mkdir, mkdtemp, readFile, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { apply } from '../index.js'

const HOME = '/tmp/dsh-home'

const ok = data => ({ code: 'ok', status: 'success', data })

const inspect = {
  target: 'dsh-main',
  agent: 'dsh',
  path: HOME,
  configuration_revision: 'rev-1',
  providers: [{
    name: 'tmax',
    source: { provider: 'tmax' },
    relayed: true,
    base_url: 'http://127.0.0.1:17840',
    credential_ref: 'TMAX_TEST',
    models: [
      { id: 'public-model', protocol: 'responses' },
      { id: 'text-only', protocol: 'responses' },
      { id: 'no-context', protocol: 'responses' },
    ],
  }],
  default: { provider: 'tmax', id: 'public-model', protocol: 'responses' },
  native_projection: [{
    name: 'tmax',
    source: { provider: 'tmax' },
    models: [{
      id: 'public-model',
      protocol: 'responses',
      capabilities: { contextWindow: 200000, input: ['text', 'image'], reasoningEfforts: { low: 'low', high: 'high' } },
      efforts: ['low', 'high'],
      projection: { state: 'projected', fields: {} },
    }, {
      id: 'text-only',
      protocol: 'responses',
      capabilities: { contextWindow: 1000 },
      efforts: [],
      projection: { state: 'projected', fields: {} },
    }, {
      id: 'no-context',
      protocol: 'responses',
      capabilities: {},
      efforts: [],
      projection: { state: 'projected', fields: {} },
    }],
  }],
  applied_state: 'applied_reload_pending',
  blockers: [],
}

const gateway = {
  daemon: { discovery_state: 'compatible', reason_code: null },
  gateway: { ready: true, listen: '127.0.0.1:17840', key_ref: 'secret:gateway', config_revision: 'rev-1' },
}

const stopped = {
  daemon: { discovery_state: 'absent', reason_code: null },
  gateway: { ready: false, listen: null, key_ref: null, config_revision: null },
}

/**
 * Write a fake `tmax` executable. `commands` maps a command path ("agent list")
 * to one envelope, or to `{envelope, exitCode}` for a rejection that exits
 * nonzero. Every invocation appends its arguments to a log file.
 */
async function fakeTmax(commands) {
  const directory = await mkdtemp(join(tmpdir(), 'tmax-test-'))
  const path = join(directory, 'tmax')
  const callLog = join(directory, 'calls.log')
  const arms = Object.entries(commands).map(([command, result]) => {
    const envelope = result.envelope ?? result
    const exitCode = result.exitCode ?? 0
    const json = JSON.stringify(envelope).replaceAll("'", "'\\''")
    return `    "${command}") printf '%s\\n' '${json}'; exit ${exitCode} ;;`
  })
  await writeFile(path, [
    '#!/bin/sh',
    `printf '%s\\n' "$*" >> '${callLog}'`,
    'case "$1 $2" in',
    ...arms,
    `    *) printf '%s\\n' '{"code":"invalid_arguments","status":"error"}'; exit 2 ;;`,
    'esac',
    '',
  ].join('\n'))
  await chmod(path, 0o755)
  return {
    path,
    calls: async () => (await readFile(callLog, 'utf8')).split('\n').filter(Boolean),
  }
}

function fakeContext({ credential = 'test-key' } = {}) {
  const state = { routes: [], tools: [], skills: [], events: [], effects: [], adapterOptions: undefined, httpRoutes: [] }
  const modules = {
    '@deepseek-ai/dsh-llm-pi-ai': {
      PiAiAdapter: class {
        constructor(options) { state.adapterOptions = options }
        stream() { throw new Error('not called') }
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
    '@deepseek-ai/dsh-tools': { defineTool: definition => definition },
  }
  const ctx = {
    fiber: { entry: { parent: { tree: { import: async specifier => modules[specifier] } } } },
    logger: { info() {}, warn() {} },
    get(name) {
      if (name === 'credentials') return {
        credentialRef: ref => ref,
        resolve: async () => credential == null ? undefined : { value: credential },
      }
      if (name === 'tools') return { register(definition) { state.tools.push(definition); return () => {} } }
      if (name === 'skills') return { register(definition) { state.skills.push(definition); return () => {} } }
      if (name === 'attachments') return undefined
      if (name === 'fs') return undefined
      if (name === 'webServer') return { register(route) { state.httpRoutes.push(route); return () => {} } }
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

/** One HTTP request with an optional JSON body, as the DSH server would hand it over. */
function request({ method = 'POST', url = '/_tmax/connect', body } = {}) {
  const handlers = new Map()
  const req = {
    method,
    url,
    headers: {},
    setEncoding() {},
    on(event, handler) {
      handlers.set(event, handler)
      return req
    },
  }
  if (body !== undefined) {
    setImmediate(() => handlers.get('data')?.(JSON.stringify(body)))
  }
  setImmediate(() => handlers.get('end')?.())
  return req
}

/** Call one registered route and read back its status and JSON payload. */
async function call(handler, req) {
  const res = {
    statusCode: 0,
    setHeader() {},
    end(value) { res.body = value === undefined ? undefined : JSON.parse(value) },
  }
  await handler(req, res)
  return res
}

test('registers one dynamic route, a status tool, and a skill without exposing maxTokens', async () => {
  const tmax = await fakeTmax({ 'agent inspect': ok(inspect), 'gateway status': ok(gateway) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', providerPrefix: 'tmaxp-', refreshSeconds: 3600 })
  assert.deepEqual(await tmax.calls(), ['agent inspect dsh-main', 'gateway status'])
  assert.deepEqual(state.routes, ['tmaxp-tmax'])
  const profiles = state.adapterOptions.profiles()
  const profile = profiles.get('tmaxp-tmax')
  assert.equal(profile.displayName, 'tmax')
  assert.equal(profile.piProvider.baseUrl, 'http://127.0.0.1:17840')
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
  assert.equal(state.tools[0].name, 'tmax_status')
  assert.equal(state.skills[0].name, 'tmax')
  assert.deepEqual(state.httpRoutes.map(route => route.path), ['/_tmax/dsh-usage', '/_tmax/status', '/_tmax/usage', '/_tmax/usage/refresh', '/_tmax/connect'])
  const status = await state.tools[0].execute()
  assert.equal(status.target, 'dsh-main')
  assert.deepEqual(status.gateway, {
    configured: true,
    ready: true,
    listen: '127.0.0.1:17840',
    daemon_discovery_state: 'compatible',
    reason_code: null,
  })
  assert.deepEqual(status.routes, [{ name: 'tmax', protocol: 'responses', models: ['public-model', 'text-only', 'no-context'] }])
  assert.deepEqual(status.blockers, [])
})

test('manageRoutes false leaves model routes to the home plugin', async () => {
  const tmax = await fakeTmax({ 'agent inspect': ok(inspect), 'gateway status': ok(gateway) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600, manageRoutes: false })
  assert.deepEqual(state.routes, [])
  assert.equal(state.adapterOptions, undefined)
  const status = await state.tools[0].execute()
  assert.deepEqual(status.routes, [{ name: 'tmax', protocol: 'responses', models: ['public-model', 'text-only', 'no-context'] }])
})

test('resolveApiKey trims a usable credential and rejects unusable or absent ones', async () => {
  const tmax = await fakeTmax({ 'agent inspect': ok(inspect), 'gateway status': ok(gateway) })

  const usable = fakeContext({ credential: '  test-key\n' })
  await apply(usable.ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  const profile = usable.state.adapterOptions.profiles().get('tmax')
  assert.equal(await usable.state.adapterOptions.resolveApiKey('tmax', profile), 'test-key')

  const unusable = fakeContext({ credential: 'bad\u0000key' })
  await apply(unusable.ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  await assert.rejects(
    unusable.state.adapterOptions.resolveApiKey('tmax', profile),
    error => error.code === 'INVALID_CREDENTIAL' && /illegalCharacters/.test(error.message),
  )

  const absent = fakeContext({ credential: null })
  await apply(absent.ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  await assert.rejects(
    absent.state.adapterOptions.resolveApiKey('tmax', profile),
    error => error.code === 'MISSING_CREDENTIAL',
  )
})

test('a missing tmax does not fail activation and the status tool reports the blocker', async () => {
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: '/definitely/missing/tmax', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, [])
  const status = await state.tools[0].execute()
  assert.equal(status.blockers[0].code, 'tmax_unavailable')
  const statusRoute = state.httpRoutes.find(route => route.path === '/_tmax/status')
  const response = await call(statusRoute.handler, request({ method: 'GET' }))
  assert.match(response.body.lastError, /ENOENT|tmax/)
})

test('resolves the target whose declared path matches the DSH home', async () => {
  const root = await mkdtemp(join(tmpdir(), 'tmax-home-'))
  const real = join(root, 'real-home')
  const link = join(root, 'link-home')
  await mkdir(real)
  await symlink(real, link)
  const tmax = await fakeTmax({
    'agent list': ok({
      agents: [
        { target: 'pi-main', agent: 'pi', path: join(root, 'pi-home') },
        { target: 'dsh-main', agent: 'dsh', path: real },
        { target: 'dsh-second', agent: 'dsh', path: join(root, 'second-home') },
      ],
    }),
    'agent inspect': ok({ ...inspect, path: real }),
    'gateway status': ok(gateway),
  })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, home: `${link}/`, refreshSeconds: 3600 })
  assert.deepEqual(await tmax.calls(), ['agent list', 'agent inspect dsh-main', 'gateway status'])
  const status = await state.tools[0].execute()
  assert.equal(status.target, 'dsh-main')
})

test('reports an unmatched or ambiguous DSH home as an actionable error', async () => {
  const unmatched = await fakeTmax({
    'agent list': ok({ agents: [{ target: 'dsh-main', agent: 'dsh', path: '/tmp/somewhere-else' }] }),
  })
  const unmatchedContext = fakeContext()
  await apply(unmatchedContext.ctx, { tmaxPath: unmatched.path, home: '/tmp/nowhere', refreshSeconds: 3600 })
  const unmatchedStatus = await unmatchedContext.state.tools[0].execute()
  assert.equal(unmatchedStatus.blockers[0].code, 'tmax_unavailable')
  assert.match(unmatchedStatus.blockers[0].message, /unknown_resource/)
  const unmatchedRoute = unmatchedContext.state.httpRoutes.find(route => route.path === '/_tmax/connect')
  const unmatchedResponse = await call(unmatchedRoute.handler, request())
  assert.equal(unmatchedResponse.statusCode, 409)
  assert.equal(unmatchedResponse.body.code, 'unknown_resource')

  const ambiguous = await fakeTmax({
    'agent list': ok({
      agents: [
        { target: 'dsh-main', agent: 'dsh', path: HOME },
        { target: 'dsh-other', agent: 'dsh', path: `${HOME}/` },
      ],
    }),
  })
  const ambiguousContext = fakeContext()
  await apply(ambiguousContext.ctx, { tmaxPath: ambiguous.path, home: HOME, refreshSeconds: 3600 })
  const ambiguousStatus = await ambiguousContext.state.tools[0].execute()
  assert.equal(ambiguousStatus.blockers[0].code, 'tmax_unavailable')
  assert.match(ambiguousStatus.blockers[0].message, /invalid_arguments/)
})

test('a failing inspect envelope surfaces its code and message', async () => {
  const tmax = await fakeTmax({
    'agent inspect': {
      envelope: {
        code: 'invalid_configuration',
        status: 'error',
        blockers: [{ code: 'invalid_configuration', message: 'Configuration format version 1 is unsupported' }],
      },
      exitCode: 4,
    },
  })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, [])
  const status = await state.tools[0].execute()
  assert.equal(status.blockers[0].code, 'tmax_unavailable')
  assert.match(status.blockers[0].message, /invalid_configuration/)
  assert.match(status.blockers[0].message, /Configuration format version 1 is unsupported/)
})

test('an older inspect shape is reported as unsupported rather than projecting no routes', async () => {
  const legacy = {
    ...inspect,
    providers: [{
      name: 'tmax',
      source: { gateway: true },
      models: [{ id: 'public-model', protocol: 'responses' }],
    }],
    native_projection: [{
      name: 'tmax',
      source: { gateway: true },
      models: [{ id: 'public-model', protocol: 'responses', efforts: [] }],
    }],
  }
  const tmax = await fakeTmax({ 'agent inspect': ok(legacy), 'gateway status': ok(gateway) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, [])
  const status = await state.tools[0].execute()
  assert.equal(status.blockers[0].code, 'tmax_unavailable')
  assert.match(status.blockers[0].message, /unsupported_tmax/)
  assert.match(status.blockers[0].message, /update tmax/)
})

test('mixed protocols add a blocker and skip the route', async () => {
  const mixed = {
    ...inspect,
    providers: [{
      name: 'tmax',
      source: { provider: 'tmax' },
      relayed: true,
      base_url: 'http://127.0.0.1:17840',
      credential_ref: 'TMAX_TEST',
      models: [
        { id: 'public-model', protocol: 'responses' },
        { id: 'public-model', protocol: 'chat-completions' },
      ],
    }],
    native_projection: null,
    blockers: [{
      code: 'incompatible',
      message: 'This native provider has one effective transport; select separate native entries for different protocols',
    }],
  }
  const tmax = await fakeTmax({ 'agent inspect': ok(mixed), 'gateway status': ok(gateway) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, [])
  const status = await state.tools[0].execute()
  assert.deepEqual(status.routes, [])
  assert.ok(status.blockers.some(blocker =>
    blocker.code === 'incompatible'
    && blocker.message === 'Gateway route "tmax" mixes protocols; split it into one route per protocol'))
  assert.ok(!status.blockers.some(blocker => blocker.code === 'gateway_entry_missing'))
})

test('a stopped gateway keeps the routes but reports readiness as not ready', async () => {
  const tmax = await fakeTmax({ 'agent inspect': ok(inspect), 'gateway status': ok(stopped) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  assert.deepEqual(state.routes, ['tmax'])
  const status = await state.tools[0].execute()
  assert.deepEqual(status.gateway, {
    configured: true,
    ready: false,
    listen: null,
    daemon_discovery_state: 'absent',
    reason_code: 'gateway_not_ready',
  })
  assert.deepEqual(status.routes, [{ name: 'tmax', protocol: 'responses', models: ['public-model', 'text-only', 'no-context'] }])
})

test('a failing gateway status leaves gateway facts unknown without losing routes', async () => {
  // No `gateway status` arm: the fake exits with its generic error envelope.
  const tmax = await fakeTmax({ 'agent inspect': ok(inspect) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  const status = await state.tools[0].execute()
  assert.equal(status.gateway.configured, true)
  assert.equal(status.gateway.ready, null)
  assert.equal(status.gateway.listen, null)
  assert.equal(status.gateway.reason_code, 'daemon_identity_unavailable')
  assert.equal(status.routes.length, 1)
})

test('gateway reason codes distinguish an unconfigured gateway from a not-ready one', async () => {
  const unconfigured = await fakeTmax({
    'agent inspect': ok({ ...inspect, providers: [], native_projection: null }),
    'gateway status': ok(stopped),
  })
  const unconfiguredContext = fakeContext()
  await apply(unconfiguredContext.ctx, { tmaxPath: unconfigured.path, target: 'dsh-main', refreshSeconds: 3600 })
  const unconfiguredStatus = await unconfiguredContext.state.tools[0].execute()
  assert.equal(unconfiguredStatus.gateway.configured, false)
  assert.equal(unconfiguredStatus.gateway.ready, null)
  assert.equal(unconfiguredStatus.gateway.reason_code, 'gateway_unconfigured')
  assert.deepEqual(unconfiguredStatus.blockers.map(blocker => blocker.code), ['gateway_unconfigured'])

  const notReady = await fakeTmax({
    'agent inspect': ok(inspect),
    'gateway status': ok({
      daemon: { discovery_state: 'compatible', reason_code: null },
      gateway: { ready: false, listen: '127.0.0.1:17840', key_ref: 'secret:gateway', config_revision: 'rev-1' },
    }),
  })
  const notReadyContext = fakeContext()
  await apply(notReadyContext.ctx, { tmaxPath: notReady.path, target: 'dsh-main', refreshSeconds: 3600 })
  const notReadyStatus = await notReadyContext.state.tools[0].execute()
  assert.equal(notReadyStatus.gateway.configured, true)
  assert.equal(notReadyStatus.gateway.ready, false)
  assert.equal(notReadyStatus.gateway.reason_code, 'gateway_not_ready')
  assert.equal(notReadyStatus.routes.length, 1)
})

test('a target without a relayed entry adds the gateway entry blocker', async () => {
  const direct = {
    ...inspect,
    providers: [{
      name: 'upstream',
      source: { provider: 'upstream' },
      relayed: false,
      base_url: null,
      credential_ref: null,
      models: [{ id: 'deepseek-v4.1-flash', protocol: 'responses' }],
    }],
    native_projection: null,
    blockers: [{ code: 'native_scope_unavailable', message: 'Select an existing DSH home before applying' }],
  }
  const tmax = await fakeTmax({ 'agent inspect': ok(direct), 'gateway status': ok(gateway) })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  const status = await state.tools[0].execute()
  assert.equal(status.gateway.configured, true)
  assert.deepEqual(status.routes, [])
  assert.deepEqual(status.blockers.map(blocker => blocker.code), ['gateway_entry_missing', 'native_scope_unavailable'])
})

test('http routes expose status and apply the target without returning a key', async () => {
  const applied = {
    code: 'awaiting_native_reload',
    status: 'success',
    data: {
      target: 'dsh-main',
      agent: 'dsh',
      path: HOME,
      state: 'applied_reload_pending',
      written: true,
      changed_files: [`${HOME}/.credentials.yaml`],
      unchanged_files: [],
      recovery: 'native-1',
    },
  }
  const tmax = await fakeTmax({
    'agent inspect': ok(inspect),
    'gateway status': ok(gateway),
    'agent apply': applied,
  })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', providerPrefix: 'tmaxp-', refreshSeconds: 3600 })
  const statusRoute = state.httpRoutes.find(route => route.path === '/_tmax/status')
  const connectRoute = state.httpRoutes.find(route => route.path === '/_tmax/connect')
  const statusResponse = await call(statusRoute.handler, request({ method: 'GET' }))
  assert.equal(statusResponse.body.routes[0].name, 'tmax')
  assert.equal(statusResponse.body.routes[0].models.length, 3)
  const connectResponse = await call(connectRoute.handler, request())
  assert.equal(connectResponse.statusCode, 200)
  assert.equal(connectResponse.body.ok, true)
  assert.equal(connectResponse.body.connect.state, 'applied_reload_pending')
  assert.equal(JSON.stringify(connectResponse.body).includes('gateway-key'), false)
  assert.ok((await tmax.calls()).includes('agent apply dsh-main'))
})

test('connect rejects a blocked apply with the envelope code and message', async () => {
  const tmax = await fakeTmax({
    'agent inspect': ok(inspect),
    'gateway status': ok(stopped),
    'agent apply': {
      envelope: {
        code: 'gateway_not_ready',
        status: 'blocked',
        blockers: [{ code: 'gateway_not_ready', message: 'Explicitly start the configured gateway' }],
      },
      exitCode: 3,
    },
  })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  const connectRoute = state.httpRoutes.find(route => route.path === '/_tmax/connect')
  const response = await call(connectRoute.handler, request())
  assert.equal(response.statusCode, 409)
  assert.equal(response.body.ok, false)
  assert.equal(response.body.code, 'gateway_not_ready')
  assert.equal(response.body.message, 'Explicitly start the configured gateway')
  assert.ok((await tmax.calls()).includes('agent apply dsh-main'))
})

test('connect preview passes --dry-run instead of applying', async () => {
  const tmax = await fakeTmax({
    'agent inspect': ok(inspect),
    'gateway status': ok(gateway),
    'agent apply': {
      code: 'dry_run_ready',
      status: 'dry_run',
      data: { target: 'dsh-main', dry_run: true, files: [], effects: { native_writes: false } },
    },
  })
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: tmax.path, target: 'dsh-main', refreshSeconds: 3600 })
  const connectRoute = state.httpRoutes.find(route => route.path === '/_tmax/connect')

  const byBody = await call(connectRoute.handler, request({ body: { preview: true } }))
  assert.equal(byBody.statusCode, 200)
  assert.equal(byBody.body.connect.dry_run, true)

  const byQuery = await call(connectRoute.handler, request({ url: '/_tmax/connect?preview=1' }))
  assert.equal(byQuery.statusCode, 200)

  const applyCalls = (await tmax.calls()).filter(call => call.startsWith('agent apply'))
  assert.deepEqual(applyCalls, ['agent apply dsh-main --dry-run', 'agent apply dsh-main --dry-run'])
})

test('connect reports a transport failure as 502', async () => {
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: '/definitely/missing/tmax', target: 'dsh-main', refreshSeconds: 3600 })
  const connectRoute = state.httpRoutes.find(route => route.path === '/_tmax/connect')
  const response = await call(connectRoute.handler, request())
  assert.equal(response.statusCode, 502)
  assert.equal(response.body.ok, false)
  assert.equal(response.body.code, 'connect_unavailable')
})
