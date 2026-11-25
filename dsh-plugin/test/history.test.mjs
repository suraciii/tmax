import assert from 'node:assert/strict'
import { chmod, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { apply } from '../index.js'
import {
  HISTORY_READ_TIMEOUT_MS,
  HISTORY_RECORD_LIMIT,
  createHistoryService,
  mapHistoryBlockers,
  mapHistoryCoverage,
  mapHistoryRead,
  mapHistoryRecord,
} from '../host/history.js'

// ---------------------------------------------------------------------------
// Fixtures: the compact retained-history shape tmax reports.

function coverage(count) {
  return {
    process_instance: 'proc-1',
    process_started_at: '2026-11-23T09:00:00Z',
    last_clear_at: null,
    limits: { records: 256, record_bytes: 32_768 },
    retention: 'oldest terminal record evicted first; no disk history',
    retained_count: count,
    oldest: '2026-11-23T10:00:00Z',
    newest: '2026-11-23T10:02:00Z',
    evicted: 0,
    incomplete_capture: false,
    prior_process_coverage: 'unavailable',
    direct_traffic_coverage: 'unobserved',
  }
}

function historyRecord(index) {
  return {
    request: `req-${index}`,
    time: `2026-11-23T10:${String(index % 60).padStart(2, '0')}:00Z`,
    time_basis: 'admission',
    model: 'deepseek-v4.1-flash',
    model_kind: 'alias',
    protocol: 'responses',
    provider: 'opencode-go-2',
    target: 'dsh-main',
    agent_provider: null,
    outcome: index % 3 === 0 ? 'failed' : 'completed',
    reason_code: index % 3 === 0 ? 'upstream_rejection' : null,
  }
}

const successData = (recordCount = 3, extra = {}) => ({
  coverage: coverage(recordCount),
  requested_range: { since: null, until: null },
  includes_in_progress: true,
  matched_retained_count: recordCount,
  omitted_by_limit: 0,
  records: Array.from({ length: recordCount }, (_, index) => historyRecord(index)),
  ...extra,
})

const successEnvelope = () => ({ code: 'ok', status: 'success' })

function successRead(recordCount = 3, extra = {}) {
  const data = successData(recordCount, extra)
  return { ok: true, code: 'ok', status: 'success', message: undefined, data, envelope: successEnvelope() }
}

function agentInspectData() {
  return {
    target: 'dsh-main',
    agent: 'dsh',
    path: '/tmp/dsh',
    configuration_revision: 'rev-1',
    providers: [{
      name: 'tmax',
      source: { provider: 'tmax' },
      relayed: true,
      base_url: 'http://127.0.0.1:17840',
      credential_ref: 'TMAX_TEST',
      applied_delivery: null,
      models: [{ id: 'public-model', protocol: 'responses' }],
    }],
    default: null,
    native_projection: [{
      name: 'tmax',
      source: { provider: 'tmax' },
      models: [{ id: 'public-model', protocol: 'responses', capabilities: { contextWindow: 200000 }, efforts: [], projection: { fields: {} } }],
    }],
    native_default: null,
    installation: null,
    applied_state: 'unknown',
    blockers: [],
  }
}

function gatewayStatusData() {
  return {
    daemon: { discovery_state: 'unidentified', reason_code: 'daemon_identity_unavailable', caller_version: '0.1.0' },
    gateway: { ready: false, listen: null, key_ref: null, config_revision: null },
    supervision: { state: 'unavailable', reason_code: 'service_manager_unavailable' },
    autostart: { saved_intent: false, registration: 'unavailable', unit_matches: false },
    shared_key: { loaded_state: 'absent' },
  }
}

// ---------------------------------------------------------------------------
// Pure mapping helpers

test('a compact history row is an allowlist: unknown fields and secrets never cross', () => {
  const request = `req-${'x'.repeat(400)}`
  const row = mapHistoryRecord({
    ...historyRecord(0),
    request,
    // Fields the contract does not name: a body/header/credential archive.
    token: 'sk-history-SECRET',
    body: { messages: ['private'] },
    headers: { authorization: 'Bearer sk-history-SECRET' },
    authorization: 'Bearer sk-history-SECRET',
  })
  assert.deepEqual(Object.keys(row).sort(), [
    'agentProvider',
    'model',
    'modelKind',
    'outcome',
    'protocol',
    'provider',
    'reasonCode',
    'request',
    'target',
    'time',
    'timeBasis',
  ])
  assert.equal(JSON.stringify(row).includes('sk-history-SECRET'), false)
  assert.equal(JSON.stringify(row).includes('private'), false)
  assert.ok(row.request.length <= 128, 'the request handle is bounded')
  assert.equal(row.timeBasis, 'admission')
  assert.equal(row.outcome, 'failed')
  assert.equal(row.reasonCode, 'upstream_rejection')

  // Optional retained evidence is mapped only when it exists.
  const selected = mapHistoryRecord({
    ...historyRecord(1),
    qualified_model: 'opencode-go-2/deepseek-v4.1-flash',
    duration_ms: 42,
  })
  assert.equal(selected.qualifiedModel, 'opencode-go-2/deepseek-v4.1-flash')
  assert.equal(selected.durationMs, 42)
  const nested = mapHistoryRecord({ ...historyRecord(2), selection: { qualified_model: 'a/b' } })
  assert.equal(nested.qualifiedModel, 'a/b')
  assert.equal('durationMs' in selected, true)
  assert.equal('qualifiedModel' in mapHistoryRecord(historyRecord(3)), false)
  // A handle spelling is the same stable row identity.
  assert.equal(mapHistoryRecord({ handle: 'handle-1' }).request, 'handle-1')
})

test('coverage and blockers are bounded to their allowlisted evidence', () => {
  const mapped = mapHistoryCoverage({
    ...coverage(7),
    retention: 'y'.repeat(400),
    unknown_field: 'dropped',
  })
  assert.equal(mapped.processInstance, 'proc-1')
  assert.equal(mapped.retainedCount, 7)
  assert.equal(mapped.priorProcessCoverage, 'unavailable')
  assert.equal(mapped.directTrafficCoverage, 'unobserved')
  assert.deepEqual(mapped.limits, { records: 256, recordBytes: 32_768 })
  assert.ok(mapped.retention.length <= 160)
  assert.equal(mapped.incompleteCapture, false, 'the older boolean spelling still maps')
  assert.equal('unknown_field' in mapped, false)
  assert.equal(mapHistoryCoverage(undefined), null)
  assert.equal(mapHistoryCoverage({}), null)

  const blockers = mapHistoryBlockers([
    { code: 'daemon_identity_unavailable', message: 'Identity mismatch', affected: ['x'], secret: 'sk-b' },
    { code: '', message: 'no code' },
    { message: 'no code at all' },
  ])
  assert.deepEqual(blockers, [{ code: 'daemon_identity_unavailable', message: 'Identity mismatch' }])
  assert.equal(JSON.stringify(blockers).includes('sk-b'), false)
})

test('a numeric incomplete-capture count survives mapping as material gap evidence', () => {
  // Current tmax reports a count, including a meaningful numeric zero.
  assert.equal(mapHistoryCoverage({ incomplete_capture: 0 }).incompleteCapture, 0)
  assert.equal(mapHistoryCoverage({ incomplete_capture: 7 }).incompleteCapture, 7)
  // Older output used a boolean; both spellings stay readable.
  assert.equal(mapHistoryCoverage({ incomplete_capture: true }).incompleteCapture, true)
  assert.equal(mapHistoryCoverage({ incomplete_capture: false }).incompleteCapture, false)
  assert.equal(mapHistoryCoverage({ incomplete_capture: 'many' }).incompleteCapture, null)

  // The gap reaches the route coverage, not just the pure helper.
  const read = successRead(2)
  read.data.coverage = { ...coverage(2), incomplete_capture: 7 }
  assert.equal(mapHistoryRead(read, { generatedAt: '2026-11-23T12:00:00Z' }).body.coverage.incompleteCapture, 7)
  const zero = successRead(2)
  zero.data.coverage = { ...coverage(2), incomplete_capture: 0 }
  assert.equal(mapHistoryRead(zero).body.coverage.incompleteCapture, 0, 'numeric zero is a fact, not unknown')
})

test('a successful read caps records, reports omission, and bounds the payload', () => {
  const read = successRead(60, { omitted_by_limit: 5 })
  const outcome = mapHistoryRead(read, { generatedAt: '2026-11-23T12:00:00Z' })
  assert.equal(outcome.status, 200)
  assert.equal(outcome.body.ok, true)
  assert.equal(outcome.body.generatedAt, '2026-11-23T12:00:00Z')
  assert.equal(outcome.body.records.length, HISTORY_RECORD_LIMIT)
  assert.equal(HISTORY_RECORD_LIMIT, 50)
  assert.equal(outcome.body.truncated, true)
  assert.equal(outcome.body.omitted, 5 + (60 - HISTORY_RECORD_LIMIT))
  assert.equal(outcome.body.coverage.retainedCount, 60)
  assert.equal(outcome.body.records[0].request, 'req-0')
  assert.equal(outcome.body.records.at(-1).request, `req-${HISTORY_RECORD_LIMIT - 1}`)

  const small = mapHistoryRead(successRead(2), { generatedAt: '2026-11-23T12:00:00Z' })
  assert.equal(small.body.records.length, 2)
  assert.equal('truncated' in small.body, false)
  assert.equal('omitted' in small.body, false)
})

test('typed unavailability is 200 evidence, never an empty history', () => {
  const blocked = mapHistoryRead({
    ok: false,
    code: 'daemon_identity_unavailable',
    status: 'blocked',
    message: 'Daemon identity challenge, authority, or process did not match',
    data: { coverage: null },
    envelope: {
      code: 'daemon_identity_unavailable',
      status: 'blocked',
      data: null,
      blockers: [{
        code: 'daemon_identity_unavailable',
        message: 'Daemon identity challenge, authority, or process did not match',
        affected: [],
        extra: 'sk-blocker',
      }],
    },
  })
  assert.equal(blocked.status, 200)
  assert.equal(blocked.body.ok, false)
  assert.equal(blocked.body.code, 'daemon_identity_unavailable')
  assert.match(blocked.body.message, /Daemon identity challenge/)
  assert.deepEqual(blocked.body.blockers, [{
    code: 'daemon_identity_unavailable',
    message: 'Daemon identity challenge, authority, or process did not match',
  }])
  assert.equal('records' in blocked.body, false)
  assert.equal(JSON.stringify(blocked.body).includes('sk-blocker'), false)

  const noDaemon = mapHistoryRead({
    ok: true,
    code: 'ok',
    status: 'success',
    data: { snapshot_state: 'absent', coverage: coverage(0), records: [], reason_code: 'no_daemon_history' },
    envelope: successEnvelope(),
  })
  assert.equal(noDaemon.status, 200)
  assert.equal(noDaemon.body.ok, false)
  assert.equal(noDaemon.body.code, 'no_daemon_history')
  assert.equal(noDaemon.body.message, 'no retained daemon history')
  assert.equal(noDaemon.body.coverage.retainedCount, 0)
  assert.equal('records' in noDaemon.body, false, 'no daemon history is not an empty record list')

  const malformed = mapHistoryRead({ ok: true, code: 'ok', status: 'success', data: {}, envelope: successEnvelope() })
  assert.equal(malformed.status, 200)
  assert.equal(malformed.body.ok, false)
  assert.equal(malformed.body.code, 'history_unavailable')

  const transport = mapHistoryRead({ ok: false, envelope: undefined, message: 'tmax did not answer before the timeout' })
  assert.equal(transport.status, 502)
  assert.equal(transport.body.ok, false)
  assert.equal(transport.body.code, 'history_unavailable')
  assert.match(transport.body.message, /did not answer/)
})

test('the history service issues exactly one bounded read and stamps the read time', async () => {
  const calls = []
  const service = createHistoryService({
    config: { tmaxPath: '/definitely/tmax' },
    log: () => {},
    now: () => '2026-11-23T12:00:00Z',
    read: async (config, argv, options) => {
      calls.push({ config, argv, options })
      return successRead(1)
    },
  })
  const outcome = await service.read()
  assert.equal(outcome.status, 200)
  assert.equal(outcome.body.generatedAt, '2026-11-23T12:00:00Z')
  assert.equal(calls.length, 1)
  assert.deepEqual(calls[0].argv, ['history', 'list'])
  assert.equal(calls[0].options.timeoutMs, HISTORY_READ_TIMEOUT_MS)
  assert.equal(HISTORY_READ_TIMEOUT_MS, 45_000)
  assert.equal(calls[0].options.env, process.env)

  const failed = createHistoryService({
    config: {},
    log: () => {},
    read: async () => { throw new Error('spawn failed') },
  })
  const transport = await failed.read()
  assert.equal(transport.status, 502)
  assert.equal(transport.body.code, 'history_unavailable')
})

// ---------------------------------------------------------------------------
// The HTTP route through the real plugin surface

const FAKE_SCRIPT = `#!/usr/bin/env node
const fs = require('node:fs')
const scenarioPath = __SCENARIO__
const logPath = __LOG__
const argv = process.argv.slice(2)
fs.appendFileSync(logPath, JSON.stringify(argv) + "\\n")
const scenario = JSON.parse(fs.readFileSync(scenarioPath, 'utf8'))
const key = argv.join(' ')
function pick() {
  const dispatch = scenario.dispatch || {}
  if (Object.prototype.hasOwnProperty.call(dispatch, key)) return dispatch[key]
  if (key === 'history list') return scenario.history
  return undefined
}
const body = pick() || {
  code: 'unknown_resource',
  status: 'error',
  data: null,
  blockers: [{ code: 'unknown_resource', message: 'fake tmax has no response for: ' + key }],
  __exit: 2,
}
if (typeof body.__raw === 'string') {
  process.stdout.write(body.__raw)
  process.exit(body.__exit === undefined ? 4 : body.__exit)
}
const exitCode = body.__exit === undefined ? (body.status === 'success' || body.status === 'dry_run' ? 0 : 4) : body.__exit
const envelope = { ...body }
delete envelope.__exit
process.stdout.write(JSON.stringify(envelope))
process.exit(exitCode)
`

async function fakeTmax(scenario) {
  const directory = await mkdtemp(join(tmpdir(), 'tmax-history-cli-'))
  const scenarioPath = join(directory, 'scenario.json')
  const logPath = join(directory, 'calls.jsonl')
  const scriptPath = join(directory, 'tmax')
  await writeFile(scenarioPath, JSON.stringify(scenario))
  await writeFile(logPath, '')
  await writeFile(scriptPath, FAKE_SCRIPT.replace('__SCENARIO__', JSON.stringify(scenarioPath)).replace('__LOG__', JSON.stringify(logPath)))
  await chmod(scriptPath, 0o755)
  return {
    path: scriptPath,
    async patch(mutator) {
      const current = JSON.parse(await readFile(scenarioPath, 'utf8'))
      mutator(current)
      await writeFile(scenarioPath, JSON.stringify(current))
    },
    async calls() {
      const text = await readFile(logPath, 'utf8')
      return text.split('\n').filter(line => line !== '').map(line => JSON.parse(line))
    },
    async clearCalls() {
      await writeFile(logPath, '')
    },
  }
}

function fakeContext() {
  const state = { httpRoutes: [], effects: [] }
  const modules = {
    '@deepseek-ai/dsh-llm-pi-ai': {
      PiAiAdapter: class {
        constructor(options) {
          this.options = options
        }
        stream() { throw new Error('not called') }
      },
    },
    '@deepseek-ai/dsh-llm': {
      LlmError: class extends Error {},
      resolveRetryPolicy: () => ({ mode: 'normal', maxRetries: 0 }),
      resolveImageAttachmentAccess: () => undefined,
    },
    '@deepseek-ai/dsh-credentials': {
      credentialRef: ref => ref,
      normalizeApiKey: raw => ({ ok: true, value: raw.trim() }),
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
      if (name === 'credentials') {
        return {
          credentialRef: ref => ref,
          resolve: async () => undefined,
          describe: async () => ({ configured: false }),
        }
      }
      if (name === 'settings') return { describe: () => [] }
      if (name === 'agentDefaultModel') return { currentSelection: () => null }
      if (name === 'webServer') {
        return {
          register(route) {
            state.httpRoutes.push(route)
            return () => {}
          },
        }
      }
      if (name === 'tools') return { register: () => () => {} }
      if (name === 'skills') return { register: () => () => {} }
      if (name === 'commands') return { register: () => () => {} }
      return undefined
    },
    llm: {
      listProviders: () => [],
      listConfigurableProviders: () => [],
      registerAdapter(names) {
        state.routes = [...names]
        const handle = () => {
          state.routes = []
        }
        handle.replace = next => {
          state.routes = [...next]
        }
        return handle
      },
    },
    on: () => () => {},
    emit: () => {},
    effect: callback => {
      state.effects.push(callback())
    },
    inject(deps, callback) {
      const child = Object.create(ctx)
      for (const name of deps) {
        Object.defineProperty(child, name, { get: () => ctx.get(name), configurable: true })
      }
      callback(child)
      return { dispose() {} }
    },
  }
  return { ctx, state }
}

function makeRes() {
  const out = { statusCode: undefined, headers: {}, body: undefined }
  const res = {
    out,
    setHeader(name, value) { out.headers[name.toLowerCase()] = value },
    end(value) { out.body = value === undefined ? undefined : JSON.parse(value) },
  }
  Object.defineProperty(res, 'statusCode', {
    get: () => out.statusCode,
    set: value => { out.statusCode = value },
  })
  return res
}

function makeReq(method, { body, headers = {} } = {}) {
  const chunks = body === undefined ? [] : [Buffer.from(body)]
  return {
    method,
    url: '/',
    headers: { host: 'localhost:3080', ...headers },
    async *[Symbol.asyncIterator]() {
      for (const chunk of chunks) yield chunk
    },
  }
}

async function callRoute(route, method, options) {
  const res = makeRes()
  await route.handler(makeReq(method, options), res)
  return res.out
}

function routeOf(state, path) {
  const route = state.httpRoutes.find(candidate => candidate.path === path)
  assert.ok(route, `route ${path} is registered`)
  return route
}

function baseScenario() {
  return {
    dispatch: {
      'agent inspect dsh-main': { code: 'ok', status: 'success', data: agentInspectData() },
      'gateway status': { code: 'ok', status: 'success', data: gatewayStatusData() },
      status: { code: 'ok', status: 'success', data: { config_path: '/tmp/tmax/tmax.toml', file_revision: 'rev-1', configuration_state: 'valid', daemon: gatewayStatusData() } },
      'provider list': { code: 'ok', status: 'success', data: { providers: [] } },
      'config validate': { code: 'ok', status: 'success', data: { valid: true, revision: 'rev-1', snapshot_state: 'available' } },
      'model explain public-model --protocol responses': {
        code: 'ok',
        status: 'success',
        data: { selection: null, priority_evaluations: [], excluded_candidates: [] },
      },
    },
    history: { code: 'ok', status: 'success', data: successData(3) },
  }
}

async function applied({ scenario = baseScenario() } = {}) {
  const fake = await fakeTmax(scenario)
  const home = await mkdtemp(join(tmpdir(), 'tmax-history-home-'))
  const { ctx, state } = fakeContext()
  await apply(ctx, { target: 'dsh-main', tmaxPath: fake.path, home, refreshSeconds: 3600 })
  await fake.clearCalls()
  return { fake, state, home }
}

test('GET /_tmax/history is an exact authenticated GET that reads one bounded history list', async () => {
  const { fake, state } = await applied()
  const route = routeOf(state, '/_tmax/history')
  assert.equal(route.kind, 'exact', 'DSH admits only the exact authenticated route')
  assert.equal(typeof route.handler, 'function')
  assert.equal('method' in route, false, 'the handler owns the GET-only guard; DSH registers the exact path')

  const result = await callRoute(route, 'GET', {})
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.ok, true)
  assert.match(result.body.generatedAt, /^\d{4}-\d{2}-\d{2}T/u)
  assert.equal(result.body.records.length, 3)
  assert.equal(result.body.records[0].request, 'req-0')
  assert.equal(result.body.records[0].timeBasis, 'admission')
  assert.equal(result.body.coverage.retainedCount, 3)
  assert.equal(result.body.coverage.limits.records, 256)
  assert.deepEqual(await fake.calls(), [['history', 'list']], 'one read; no other tmax argv')
})

test('the history route is GET only and never mutates or observes', async () => {
  const { fake, state } = await applied()
  const route = routeOf(state, '/_tmax/history')
  const post = await callRoute(route, 'POST', { body: '{}', headers: { 'content-type': 'application/json' } })
  assert.equal(post.statusCode, 405)
  assert.equal(post.headers.allow, 'GET')
  assert.equal(post.body.code, 'method_not_allowed')
  const put = await callRoute(route, 'PUT', {})
  assert.equal(put.statusCode, 405)
  assert.deepEqual(await fake.calls(), [], 'a wrong method never runs tmax')

  await callRoute(route, 'GET', {})
  const calls = await fake.calls()
  assert.deepEqual(calls, [['history', 'list']])
  for (const argv of calls) {
    assert.equal(argv[0], 'history', 'no provider observation, connect, gateway, or web argv')
    assert.equal(argv[1], 'list', 'never history clear/inspect')
  }
})

test('the route reports typed unavailability at 200 and transport at 502', async () => {
  const scenario = baseScenario()
  scenario.history = {
    code: 'daemon_identity_unavailable',
    status: 'blocked',
    data: null,
    blockers: [{ code: 'daemon_identity_unavailable', message: 'Daemon identity challenge, authority, or process did not match' }],
    __exit: 3,
  }
  const typed = await applied({ scenario })
  const typedResult = await callRoute(routeOf(typed.state, '/_tmax/history'), 'GET', {})
  assert.equal(typedResult.statusCode, 200)
  assert.equal(typedResult.body.ok, false)
  assert.equal(typedResult.body.code, 'daemon_identity_unavailable')
  assert.equal(typedResult.body.blockers[0].code, 'daemon_identity_unavailable')
  assert.equal('records' in typedResult.body, false)

  const noDaemon = baseScenario()
  noDaemon.history = { code: 'ok', status: 'success', data: { snapshot_state: 'absent', coverage: coverage(0), records: [], reason_code: 'no_daemon_history' } }
  const absent = await applied({ scenario: noDaemon })
  const absentResult = await callRoute(routeOf(absent.state, '/_tmax/history'), 'GET', {})
  assert.equal(absentResult.statusCode, 200)
  assert.equal(absentResult.body.ok, false)
  assert.equal(absentResult.body.code, 'no_daemon_history')
  assert.equal('records' in absentResult.body, false)

  const scenarioRaw = baseScenario()
  scenarioRaw.history = { __raw: 'not a tmax envelope', __exit: 4 }
  const transport = await applied({ scenario: scenarioRaw })
  const transportResult = await callRoute(routeOf(transport.state, '/_tmax/history'), 'GET', {})
  assert.equal(transportResult.statusCode, 502)
  assert.equal(transportResult.body.ok, false)
  assert.equal(transportResult.body.code, 'history_unavailable')
})
