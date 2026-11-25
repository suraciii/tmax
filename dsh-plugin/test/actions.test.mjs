import { parseBlocks } from './fake-yaml.mjs'
import assert from 'node:assert/strict'
import { chmod, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { apply } from '../index.js'
import {
  ACTIONS,
  ACTION_TIMEOUTS,
  GATEWAY_ACTIONS,
  OBSERVE_SCOPES,
  actionArgv,
  actionLockKey,
  actionTimeoutMs,
  createActionService,
  isActionSuccess,
  planAction,
  planAddModel,
  providerIdsFromState,
  resultFromEnvelope,
  validateActionRequest,
} from '../host/actions.js'

// Sleep helper for the shared-lock tests only.
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms))
}

// ---------------------------------------------------------------------------
// Fixtures: the target inventory behind the state service, plus action envelopes.

function agentInspectData() {
  return {
    target: 'dsh-main',
    agent: 'dsh',
    path: '/tmp/dsh',
    scope_state: 'available',
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
      models: [{
        id: 'public-model',
        protocol: 'responses',
        capabilities: { contextWindow: 200000 },
        efforts: [],
        projection: { fields: {} },
      }],
    }],
    native_default: { state: 'available', fields: { provider: 'tmax', model: 'public-model', present: true } },
    installation: { executable: '/tmp/bin/dsh', version: '0.2.3-alpha.2', state: 'detected', source: 'https://github.com/deepseek-ai/deepseek-harness' },
    applied_state: 'applied',
    pending_native_changes: [],
    blockers: [],
  }
}

function gatewayStatusData() {
  return {
    daemon: { discovery_state: 'unidentified', reason_code: 'daemon_identity_unavailable', caller_version: '0.1.0' },
    gateway: { ready: false, listen: null, key_ref: null, config_revision: null },
    supervision: { state: 'unavailable', reason_code: 'service_manager_unavailable' },
    autostart: { saved_intent: true, registration: 'unavailable', unit_matches: false },
    shared_key: { loaded_reference: null, loaded_state: 'absent' },
  }
}

function tmaxStatusData() {
  return {
    config_path: '/tmp/tmax/tmax.toml',
    file_revision: 'rev-1',
    configuration_state: 'valid',
    daemon: gatewayStatusData(),
    failures: null,
  }
}

function providerListData() {
  return {
    revision: 'rev-1',
    providers: [
      { id: 'kimi', name: 'Kimi Coding Plan', enabled: true, authentication: 'api-key', affected_targets: 1, observations: { billing_state: 'observed', observations: [{ scope: 'quota', state: 'observed', last_attempt_outcome: 'completed', last_attempt_reason_code: null }] }, quota_suspension: { state: 'unsupported' } },
      { id: 'cpa-us2', name: null, enabled: true, authentication: 'api-key', affected_targets: 1, observations: { billing_state: 'unknown', observations: [{ scope: 'quota', state: 'unknown', last_attempt_outcome: 'unavailable', last_attempt_reason_code: 'unsupported_source' }] }, quota_suspension: { state: 'unsupported' } },
    ],
  }
}

function kimiInspectData() {
  return {
    id: 'kimi',
    source: 'upstream',
    provider: { enabled: true, integration: 'kimi-code-plan-cn', overrides: { name: 'Kimi Coding Plan' }, auth: { method: 'api-key' }, subscription: null },
    observations: {
      account_identity: null,
      billing: {
        kind: 'plan_allowance',
        state: 'observed',
        dashboard: 'https://www.kimi.com/code/console',
        freshness: { state: 'unqualified' },
        observation: { observed_at: '2026-11-23T06:26:05Z' },
        display: { notes: [], rows: [] },
      },
    },
    quota_suspension: { state: 'unsupported' },
  }
}

function cpaInspectData() {
  return {
    id: 'cpa-us2',
    source: 'upstream',
    provider: { enabled: true, integration: 'custom', overrides: {}, auth: { method: 'api-key' }, subscription: null },
    observations: {
      account_identity: null,
      billing: { kind: 'unknown', state: 'unknown', dashboard: '', freshness: { state: 'missing' }, observation: null, display: null },
    },
    quota_suspension: { state: 'unsupported' },
    failure_state: { state: 'not_configured', consecutive_failures: 0, active_priority: null, expires_at: null, reason_code: null },
  }
}

function stateExplainData() {
  return {
    selection: { qualified_model: 'p/public-model', provider: 'p', model: 'public-model', priority: 0 },
    priority_evaluations: [{ qualified_model: 'p/public-model', priority_evaluation: { effective_priority: 0 } }],
    excluded_candidates: [],
  }
}

// ---------------------------------------------------------------------------
// Fake tmax envelopes. A dispatch entry is keyed by the exact argv it answers,
// so an action that builds a different argv gets a typed unknown_resource
// refusal instead of a silent pass.

const success = data => ({ code: 'ok', status: 'success', data })
const dryReady = data => ({ code: 'dry_run_ready', status: 'dry_run', data })

function refreshKey(provider, scope, dryRun) {
  return `provider refresh ${provider} --scope ${scope}${dryRun ? ' --dry-run' : ''}`
}

function refreshEnvelope(provider, scope, dryRun) {
  return dryRun
    ? dryReady({ provider, checks: [{ scope, outcome: 'not_executed', reason_code: null, publication: 'not_performed' }] })
    : success({ provider, checks: [{ scope, outcome: 'completed', reason_code: null, published: true }] })
}

function quotaResumeEnvelope(provider, dryRun) {
  const suspension = dryRun
    ? { state: 'suspended', reason_code: 'quota_exhausted', reset_at: null }
    : { state: 'clear', reason_code: null, reset_at: null }
  return dryRun
    ? dryReady({ provider, requested: 'provider quota-resume', resumed: false, quota_suspension: suspension })
    : success({ provider, requested: 'provider quota-resume', resumed: true, quota_suspension: suspension })
}

function failureResetEnvelope(dryRun) {
  const failure = dryRun
    ? { state: 'active', consecutive_failures: 2, active_priority: 1, expires_at: null, reason_code: 'upstream_rejection' }
    : { state: 'not_configured', consecutive_failures: 0, active_priority: null, expires_at: null, reason_code: null }
  return dryRun
    ? dryReady({ provider: 'kimi', requested: 'provider failure-reset', reset: false, failure_state: failure })
    : success({ provider: 'kimi', requested: 'provider failure-reset', reset: true, failure_state: failure })
}

function webStatusEnvelope() {
  return success({
    daemon: { discovery_state: 'unidentified', reason_code: 'daemon_identity_unavailable' },
    gateway: { ready: false, listen: null },
  })
}

function webOpenEnvelope() {
  return success({ ready: true, url: 'http://127.0.0.1:17841/', daemon: { discovery_state: 'compatible' } })
}

function gatewayLifecycleEnvelope(verb, dryRun) {
  if (dryRun) {
    return dryReady({
      operation: `gateway ${verb}`,
      dry_run: true,
      planned_action: 'planned',
      daemon: { discovery_state: 'unidentified', reason_code: 'daemon_identity_unavailable' },
      blockers: [],
    })
  }
  return success({
    operation: `gateway ${verb}`,
    daemon: { discovery_state: 'compatible', inference: { ready: true } },
    supervision: { state: 'active' },
  })
}

function connectEnvelope(dryRun) {
  if (dryRun) {
    return dryReady({
      target: 'dsh-main',
      agent: 'dsh',
      path: '/tmp/dsh',
      dry_run: true,
      files: [{ path: '/tmp/dsh/cordis.patch.yml', revision: 'a' }],
      credentials: { resolved: false },
      effects: { locks_created: false, native_writes: false, processes_started: false, secret_reads: false },
    })
  }
  // A completed apply reports `awaiting_native_reload` (status success, exit 0).
  return {
    code: 'awaiting_native_reload',
    status: 'success',
    data: {
      target: 'dsh-main',
      agent: 'dsh',
      path: '/tmp/dsh',
      state: 'applied_reload_pending',
      written: true,
      changed_files: ['/tmp/dsh/cordis.patch.yml'],
      unchanged_files: [],
      recovery: 'recovery-1',
      fact_gaps: [],
    },
  }
}

/** `tmax model list`: all catalog rows the add action validates against. */
function modelListEnvelope() {
  return success({
    models: [
      { id: 'kimi/k3.1-pro', kind: 'concrete', enabled: true, availability: 'unverified', published_protocols: ['chat-completions', 'messages', 'responses'] },
      { id: 'cpa-us2/gpt-6-astra', kind: 'concrete', enabled: true, availability: 'unverified', published_protocols: ['chat-completions', 'responses'] },
      { id: 'public-model', kind: 'alias', enabled: true, availability: 'unverified', protocols: ['responses'], published_protocols: ['responses'] },
    ],
  })
}

function baseScenario() {
  const dispatch = {}
  for (const provider of ['kimi', 'cpa-us2']) {
    for (const scope of OBSERVE_SCOPES) {
      dispatch[refreshKey(provider, scope, false)] = refreshEnvelope(provider, scope, false)
      dispatch[refreshKey(provider, scope, true)] = refreshEnvelope(provider, scope, true)
    }
  }
  dispatch['provider quota-resume kimi'] = quotaResumeEnvelope('kimi', false)
  dispatch['provider quota-resume kimi --dry-run'] = quotaResumeEnvelope('kimi', true)
  dispatch['provider quota-resume cpa-us2'] = quotaResumeEnvelope('cpa-us2', false)
  dispatch['provider quota-resume cpa-us2 --dry-run'] = quotaResumeEnvelope('cpa-us2', true)
  dispatch['provider failure-reset kimi'] = failureResetEnvelope(false)
  dispatch['provider failure-reset kimi --dry-run'] = failureResetEnvelope(true)
  dispatch['web status'] = webStatusEnvelope()
  dispatch['web open --no-browser'] = webOpenEnvelope()
  for (const verb of ['start', 'stop', 'restart']) {
    dispatch[`gateway ${verb}`] = gatewayLifecycleEnvelope(verb, false)
    dispatch[`gateway ${verb} --dry-run`] = gatewayLifecycleEnvelope(verb, true)
  }
  // The state service composes the inventory plus provider/config/explain reads.
  dispatch['agent list'] = success({ agents: [{ target: 'dsh-main', agent: 'dsh', path: '__HOME__' }] })
  dispatch['agent inspect dsh-main'] = success(agentInspectData())
  dispatch['gateway status'] = success(gatewayStatusData())
  dispatch['status'] = success(tmaxStatusData())
  dispatch['provider list'] = success(providerListData())
  dispatch['model list'] = modelListEnvelope()
  dispatch['provider inspect kimi'] = success(kimiInspectData())
  dispatch['provider inspect cpa-us2'] = success(cpaInspectData())
  dispatch['config validate'] = success({ valid: true, revision: 'rev-1', snapshot_state: 'available' })
  dispatch['model explain public-model --protocol responses'] = success(stateExplainData())
  dispatch['agent provider add dsh-main kimi --provider kimi'] = success({ target: 'dsh-main', name: 'kimi', provider: 'kimi' })
  dispatch['agent model add dsh-main kimi k3.1-pro --protocol responses'] = success({ target: 'dsh-main', name: 'kimi', model: 'k3.1-pro', protocol: 'responses' })
  dispatch['agent apply dsh-main --dry-run'] = connectEnvelope(true)
  dispatch['agent apply dsh-main'] = connectEnvelope(false)
  return {
    connect: { dryRun: connectEnvelope(true), exec: connectEnvelope(false) },
    dispatch,
  }
}

// ---------------------------------------------------------------------------
// A fake tmax executable that dispatches on argv and records every call.

const FAKE_SCRIPT = `#!/usr/bin/env node
const fs = require('node:fs')
const scenarioPath = __SCENARIO__
const logPath = __LOG__
const argv = process.argv.slice(2)
fs.appendFileSync(logPath, JSON.stringify(argv) + "\\n")
const scenario = JSON.parse(fs.readFileSync(scenarioPath, 'utf8'))
function pick() {
  const key = argv.join(' ')
  const dispatch = scenario.dispatch || {}
  if (Object.prototype.hasOwnProperty.call(dispatch, key)) return dispatch[key]
  for (const prefix of Object.keys(dispatch).sort((a, b) => b.length - a.length)) {
    if (key.startsWith(prefix + ' ')) return dispatch[prefix]
  }
  return undefined
}
const body = pick() || {
  code: 'unknown_resource',
  status: 'error',
  data: null,
  blockers: [{ code: 'unknown_resource', message: 'fake tmax has no response for: ' + argv.join(' ') }],
  __exit: 2,
}
if (typeof body.__raw === 'string') {
  process.stdout.write(body.__raw)
  process.exit(body.__exit === undefined ? 4 : body.__exit)
}
const writes = (scenario.writeFiles || {})[argv.join(' ')]
if (Array.isArray(writes)) {
  const path = require('node:path')
  for (const file of writes) {
    fs.mkdirSync(path.dirname(file.path), { recursive: true })
    fs.writeFileSync(file.path, file.content)
  }
}
const exitCode = body.__exit === undefined ? (body.status === 'success' || body.status === 'dry_run' ? 0 : 4) : body.__exit
const envelope = { ...body }
delete envelope.__exit
const delay = scenario.delayFor === undefined || scenario.delayFor === argv.slice(0, 2).join(' ')
  ? (scenario.delayMs || 0)
  : 0
const emit = () => {
  process.stdout.write(JSON.stringify(envelope))
  process.exit(exitCode)
}
if (delay > 0) setTimeout(emit, delay)
else emit()
`

async function fakeTmax(scenario, home = '') {
  const directory = await mkdtemp(join(tmpdir(), 'tmax-action-cli-'))
  const scenarioPath = join(directory, 'scenario.json')
  const logPath = join(directory, 'calls.jsonl')
  const scriptPath = join(directory, 'tmax')
  const serialize = value => JSON.stringify(value).replaceAll('__HOME__', home)
  await writeFile(scenarioPath, serialize(scenario))
  await writeFile(logPath, '')
  await writeFile(scriptPath, FAKE_SCRIPT.replace('__SCENARIO__', JSON.stringify(scenarioPath)).replace('__LOG__', JSON.stringify(logPath)))
  await chmod(scriptPath, 0o755)
  return {
    path: scriptPath,
    async update(next) {
      await writeFile(scenarioPath, serialize(next))
    },
    async patch(mutator) {
      const current = JSON.parse(await readFile(scenarioPath, 'utf8'))
      mutator(current)
      await writeFile(scenarioPath, serialize(current))
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

// ---------------------------------------------------------------------------
// A fake plugin context exposing only the services the host half touches.

function fakeContext() {
  const state = { routes: [], httpRoutes: [] }
  const modules = {
    '@deepseek-ai/cordis-plugin-include': { parseEntryYaml: parseBlocks },
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
    effect: callback => callback(),
    // Optional services are acquired through injection; the fake resolves them
    // immediately (they are all present here) with a service-bearing child.
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

async function postJson(state, path, payload, headers = {}) {
  return callRoute(routeOf(state, path), 'POST', {
    body: JSON.stringify(payload),
    headers: { 'content-type': 'application/json', ...headers },
  })
}

async function getJson(state, path) {
  return callRoute(routeOf(state, path), 'GET', {})
}

async function applied({ scenario = baseScenario(), config = {} } = {}) {
  const home = await mkdtemp(join(tmpdir(), 'tmax-action-home-'))
  const fake = await fakeTmax(scenario, home)
  const { ctx, state } = fakeContext()
  // Pin the target: these tests exercise actions, not home-based resolution.
  await apply(ctx, { target: 'dsh-main', tmaxPath: fake.path, home, refreshSeconds: 3600, ...config })
  return { fake, state, home }
}

function assertInvalid(result, status, code) {
  assert.equal(result.ok, false)
  assert.equal(result.status, status)
  assert.equal(result.code, code)
  assert.equal(typeof result.message, 'string')
  assert.ok(result.message.length > 0)
}

// ---------------------------------------------------------------------------
// Pure helpers

test('the action contract is a closed allowlist with closed operands and deadlines', () => {
  assert.deepEqual([...ACTIONS], [
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
  assert.deepEqual([...GATEWAY_ACTIONS], ['gateway_start', 'gateway_stop', 'gateway_restart'])
  assert.deepEqual([...OBSERVE_SCOPES], ['quota', 'auth', 'models', 'all'])
  // One scope uses tmax's 45 s observation deadline; a full observation runs
  // three checks and gets a longer budget.
  assert.equal(ACTION_TIMEOUTS.observe_provider, 45_000)
  assert.equal(ACTION_TIMEOUTS.observeAll, 120_000)
  assert.equal(actionTimeoutMs('observe_provider'), 45_000)
  assert.equal(actionTimeoutMs('observe_provider', { scope: 'quota' }), 45_000)
  assert.equal(actionTimeoutMs('observe_provider', { scope: 'auth' }), 45_000)
  assert.equal(actionTimeoutMs('observe_provider', { scope: 'all' }), 120_000)
  assert.equal(actionTimeoutMs('resume_quota'), 45_000)
  assert.equal(actionTimeoutMs('reset_failure'), 45_000)
  assert.equal(actionTimeoutMs('connect'), 45_000)
  assert.equal(actionTimeoutMs('add_model'), 120_000)
  assert.equal(actionTimeoutMs('open_management', { dryRun: true }), 20_000)
  assert.equal(actionTimeoutMs('open_management', { dryRun: false }), 60_000)
  // Gateway lifecycle waits on supervision, so every verb gets the 90 s budget.
  assert.equal(ACTION_TIMEOUTS.gateway, 90_000)
  for (const action of GATEWAY_ACTIONS) {
    assert.equal(actionTimeoutMs(action), 90_000)
    assert.equal(actionTimeoutMs(action, { dryRun: true }), 90_000)
  }

  // Canonical lock keys are shared with the legacy routes.
  assert.equal(actionLockKey('connect'), 'connect')
  assert.equal(actionLockKey('add_model'), 'connect')
  assert.equal(actionLockKey('observe_provider', 'kimi'), 'observe')
  assert.equal(actionLockKey('resume_quota', 'kimi'), 'resume_quota:kimi')
  assert.equal(actionLockKey('reset_failure', 'kimi'), 'reset_failure:kimi')
  assert.equal(actionLockKey('open_management'), 'open_management')
  assert.equal(actionLockKey('gateway_start'), 'gateway:start')
  assert.equal(actionLockKey('gateway_stop'), 'gateway:stop')
  assert.equal(actionLockKey('gateway_restart'), 'gateway:restart')
  assert.equal(actionLockKey('delete_everything'), null)
})

test('every action builds its exact argv from validated operands only', () => {
  const config = { home: '/tmp/dsh-home' }
  for (const scope of OBSERVE_SCOPES) {
    assert.deepEqual(
      actionArgv('observe_provider', { provider: 'kimi', scope, config }),
      ['provider', 'refresh', 'kimi', '--scope', scope],
    )
    assert.deepEqual(
      actionArgv('observe_provider', { provider: 'kimi', scope, config, dryRun: true }),
      ['provider', 'refresh', 'kimi', '--scope', scope, '--dry-run'],
    )
  }
  assert.deepEqual(actionArgv('resume_quota', { provider: 'kimi', config }), ['provider', 'quota-resume', 'kimi'])
  assert.deepEqual(actionArgv('resume_quota', { provider: 'kimi', config, dryRun: true }), ['provider', 'quota-resume', 'kimi', '--dry-run'])
  assert.deepEqual(actionArgv('reset_failure', { provider: 'kimi', config }), ['provider', 'failure-reset', 'kimi'])
  assert.deepEqual(actionArgv('reset_failure', { provider: 'kimi', config, dryRun: true }), ['provider', 'failure-reset', 'kimi', '--dry-run'])
  assert.deepEqual(actionArgv('open_management', { config }), ['web', 'open', '--no-browser'])
  assert.deepEqual(actionArgv('open_management', { config, dryRun: true }), ['web', 'status'])
  for (const verb of ['start', 'stop', 'restart']) {
    assert.deepEqual(actionArgv(`gateway_${verb}`, { config }), ['gateway', verb])
    assert.deepEqual(actionArgv(`gateway_${verb}`, { config, dryRun: true }), ['gateway', verb, '--dry-run'])
  }
  // Connect is the native application: `agent apply TARGET`, never `dsh connect`.
  assert.deepEqual(actionArgv('connect', { config: { target: 'dsh-main' } }), ['agent', 'apply', 'dsh-main'])
  assert.deepEqual(actionArgv('connect', { config: { target: 'dsh-main' }, dryRun: true }), ['agent', 'apply', 'dsh-main', '--dry-run'])
  assert.deepEqual(actionArgv('connect', { target: 'resolved-target', dryRun: true }), ['agent', 'apply', 'resolved-target', '--dry-run'])
  // `agent apply` has no --home selector: an unresolved target has no argv.
  assert.equal(actionArgv('connect', { config }), null)
  assert.equal(actionArgv('connect', { config, dryRun: true }), null)
  // No fallback form: an unknown action, a missing provider operand, or a
  // non-enum scope has none.
  assert.equal(actionArgv('delete_everything', { provider: 'kimi' }), null)
  assert.equal(actionArgv('resume_quota', { provider: '' }), null)
  assert.equal(actionArgv('observe_provider', { provider: 'kimi', scope: 'galaxy' }), null)
})

test('request validation rejects malformed actions, operands, and scopes', () => {
  assertInvalid(validateActionRequest(null), 400, 'invalid_body')
  assertInvalid(validateActionRequest([]), 400, 'invalid_body')
  assertInvalid(validateActionRequest({}), 400, 'invalid_action')
  assertInvalid(validateActionRequest({ action: 'delete_everything' }), 400, 'invalid_action')
  assertInvalid(validateActionRequest({ action: 'resume_quota' }), 400, 'missing_provider')
  assertInvalid(validateActionRequest({ action: 'resume_quota', provider: 7 }), 400, 'invalid_provider')
  assertInvalid(validateActionRequest({ action: 'reset_failure', provider: '' }), 400, 'invalid_provider')
  assertInvalid(validateActionRequest({ action: 'observe_provider', provider: 'kimi', scope: 'galaxy' }), 400, 'invalid_scope')
  assertInvalid(validateActionRequest({ action: 'observe_provider', provider: 'kimi', dryRun: 'yes' }), 400, 'invalid_dry_run')

  assert.deepEqual(
    validateActionRequest({ action: 'observe_provider', provider: 'kimi' }),
    { ok: true, request: { action: 'observe_provider', provider: 'kimi', scope: 'quota', dryRun: false } },
  )
  assert.deepEqual(
    validateActionRequest({ action: 'open_management' }),
    { ok: true, request: { action: 'open_management', provider: null, scope: null, dryRun: false } },
  )
  // A provider sent for an action that names none is ignored, never argv.
  assert.equal(validateActionRequest({ action: 'open_management', provider: 'kimi' }).request.provider, null)

  const planned = planAction(
    { action: 'observe_provider', provider: 'kimi' },
    { providerIds: ['kimi', 'cpa-us2'], config: { home: '/tmp/dsh-home' } },
  )
  assert.equal(planned.ok, true)
  assert.deepEqual(planned.plan.argv, ['provider', 'refresh', 'kimi', '--scope', 'quota'])
  assert.equal(planned.plan.timeoutMs, 45_000)
  // The full-observation scope gets the longer budget.
  const allScopes = planAction(
    { action: 'observe_provider', provider: 'kimi', scope: 'all' },
    { providerIds: ['kimi'], config: { home: '/tmp/dsh-home' } },
  )
  assert.equal(allScopes.ok, true)
  assert.equal(allScopes.plan.timeoutMs, 120_000)
  // Only exact snapshot IDs are addressable; raw strings never survive planning.
  assertInvalid(planAction({ action: 'observe_provider', provider: 'kimi --dry-run' }, { providerIds: ['kimi'] }), 409, 'unknown_provider')
  assertInvalid(planAction({ action: 'resume_quota', provider: 'KIMI' }, { providerIds: ['kimi'] }), 409, 'unknown_provider')
  assertInvalid(planAction({ action: 'reset_failure', provider: 'kimi; reset' }, { providerIds: ['kimi'] }), 409, 'unknown_provider')

  assert.deepEqual(
    providerIdsFromState({ providers: { tmax: [{ id: 'kimi' }, { id: null }, {}, { id: 'cpa-us2' }], agent: [{ provider: 'glm' }] } }),
    ['kimi', 'cpa-us2'],
  )
  assert.deepEqual(providerIdsFromState(undefined), [])

  // A connect plans the native application for the resolved target.
  const plannedConnect = planAction({ action: 'connect', dryRun: true }, { target: 'dsh-main' })
  assert.equal(plannedConnect.ok, true)
  assert.deepEqual(plannedConnect.plan.argv, ['agent', 'apply', 'dsh-main', '--dry-run'])
  assert.equal(plannedConnect.plan.timeoutMs, 45_000)
  assertInvalid(planAction({ action: 'connect' }, { config: { home: '/tmp/dsh-home' } }), 409, 'target_unresolved')
})

test('result projection preserves tmax evidence and strips key material', () => {
  const result = resultFromEnvelope({
    code: 'ok',
    status: 'success',
    operation: 'provider quota-resume',
    schema_version: 1,
    dry_run: false,
    data: { provider: 'kimi', resumed: true, token: 'sk-secret', nested: { value: 'sk-secret' } },
    blockers: [{ code: 'quota_exhausted', message: 'quota is exhausted' }],
    next_actions: [{ code: 'inspect_runtime', operation: 'status', arguments: [] }],
    effects: { secret_reads: false },
  })
  assert.deepEqual(Object.keys(result).sort(), ['blockers', 'code', 'data', 'effects', 'next_actions', 'status'])
  assert.equal(result.code, 'ok')
  assert.equal(result.status, 'success')
  assert.equal(result.data.provider, 'kimi')
  assert.equal(JSON.stringify(result).includes('sk-secret'), false)
  assert.equal('token' in result.data, false)
  assert.equal('value' in result.data.nested, false)
  assert.equal(result.blockers[0].code, 'quota_exhausted')
  assert.equal(result.next_actions[0].code, 'inspect_runtime')
  assert.equal(result.effects.secret_reads, false)

  assert.equal(isActionSuccess({ code: 'ok', status: 'success' }), true)
  assert.equal(isActionSuccess({ code: 'awaiting_native_reload', status: 'success' }), true, 'a nested success code is a completed apply')
  assert.equal(isActionSuccess({ code: 'dry_run_ready', status: 'dry_run' }), false)
  assert.equal(isActionSuccess({ code: 'dry_run_ready', status: 'dry_run' }, true), true)
  assert.equal(isActionSuccess({ code: 'blocked', status: 'blocked' }, true), false)
  assert.equal(isActionSuccess(undefined), false)
})

// ---------------------------------------------------------------------------
// HTTP action route

test('observe_provider previews with --dry-run, then executes, and returns fresh state', async () => {
  const { fake, state } = await applied()
  // Warm the snapshot cache so the pre-action state read is observable.
  assert.equal((await getJson(state, '/_tmax/state')).statusCode, 200)

  const preview = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi', dryRun: true })
  assert.equal(preview.statusCode, 200)
  assert.equal(preview.body.ok, true)
  assert.equal(preview.body.action, 'observe_provider')
  assert.equal(preview.body.dryRun, true)
  assert.deepEqual(Object.keys(preview.body).sort(), ['action', 'dryRun', 'ok', 'result', 'state'])
  assert.equal(preview.body.result.code, 'dry_run_ready')
  assert.equal(preview.body.result.status, 'dry_run')
  assert.equal(preview.body.result.data.checks[0].outcome, 'not_executed')
  assert.ok(preview.body.state, 'preview carries state')
  let calls = await fake.calls()
  assert.deepEqual(
    calls.filter(argv => argv[0] === 'provider' && argv[1] === 'refresh'),
    [['provider', 'refresh', 'kimi', '--scope', 'quota', '--dry-run']],
  )
  assert.ok(calls.some(argv => argv[0] === 'agent' && argv[1] === 'inspect'), 'a fresh state read follows the action')

  await fake.clearCalls()
  const executed = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  assert.equal(executed.statusCode, 200)
  assert.equal(executed.body.ok, true)
  assert.equal(executed.body.dryRun, false)
  assert.equal(executed.body.result.code, 'ok')
  assert.equal(executed.body.result.data.checks[0].published, true)
  calls = await fake.calls()
  assert.deepEqual(
    calls.filter(argv => argv[0] === 'provider' && argv[1] === 'refresh'),
    [['provider', 'refresh', 'kimi', '--scope', 'quota']],
  )
  assert.ok(calls.some(argv => argv[0] === 'agent' && argv[1] === 'inspect'), 'a fresh state read follows the action')
})

test('every action, scope, and preview reaches tmax through one exact argv', async () => {
  const { fake, state } = await applied()
  const actionCalls = async () => (await fake.calls()).filter(argv =>
    (argv[0] === 'provider' && ['refresh', 'quota-resume', 'failure-reset'].includes(argv[1]))
    || argv[0] === 'web'
    || (argv[0] === 'agent' && argv[1] === 'apply'))

  for (const scope of OBSERVE_SCOPES) {
    await fake.clearCalls()
    const result = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi', scope })
    assert.equal(result.statusCode, 200, `${scope}: ${JSON.stringify(result.body)}`)
    assert.deepEqual(await actionCalls(), [['provider', 'refresh', 'kimi', '--scope', scope]])
  }

  await fake.clearCalls()
  let result = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi', dryRun: true })
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.result.code, 'dry_run_ready')
  assert.deepEqual(await actionCalls(), [['provider', 'quota-resume', 'kimi', '--dry-run']])

  await fake.clearCalls()
  result = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi' })
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.result.code, 'ok')
  assert.deepEqual(await actionCalls(), [['provider', 'quota-resume', 'kimi']])

  await fake.clearCalls()
  result = await postJson(state, '/_tmax/action', { action: 'reset_failure', provider: 'kimi', dryRun: true })
  assert.equal(result.statusCode, 200)
  assert.deepEqual(await actionCalls(), [['provider', 'failure-reset', 'kimi', '--dry-run']])

  await fake.clearCalls()
  result = await postJson(state, '/_tmax/action', { action: 'reset_failure', provider: 'kimi' })
  assert.equal(result.statusCode, 200)
  assert.deepEqual(await actionCalls(), [['provider', 'failure-reset', 'kimi']])

  await fake.clearCalls()
  result = await postJson(state, '/_tmax/action', { action: 'connect', dryRun: true })
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.result.code, 'dry_run_ready')
  assert.equal(result.body.result.data.effects.secret_reads, false)
  assert.deepEqual(await actionCalls(), [['agent', 'apply', 'dsh-main', '--dry-run']])

  await fake.clearCalls()
  result = await postJson(state, '/_tmax/action', { action: 'connect' })
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.result.data.state, 'applied_reload_pending')
  assert.deepEqual(await actionCalls(), [['agent', 'apply', 'dsh-main']])
})

test('open_management previews management readiness and only execute starts the listener', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/action', { action: 'open_management', dryRun: true })
  assert.equal(preview.statusCode, 200)
  assert.equal(preview.body.ok, true)
  assert.equal(preview.body.action, 'open_management')
  assert.equal(preview.body.dryRun, true)
  assert.equal(preview.body.result.code, 'ok')
  assert.equal(preview.body.result.data.gateway.ready, false)
  let calls = await fake.calls()
  assert.deepEqual(calls.filter(argv => argv[0] === 'web'), [['web', 'status']])
  assert.equal(calls.some(argv => argv[0] === 'web' && argv[1] === 'open'), false, 'preview never starts a listener')

  await fake.clearCalls()
  const opened = await postJson(state, '/_tmax/action', { action: 'open_management' })
  assert.equal(opened.statusCode, 200)
  assert.equal(opened.body.ok, true)
  assert.equal(opened.body.dryRun, false)
  assert.equal(opened.body.result.data.ready, true)
  assert.equal(opened.body.result.data.url, 'http://127.0.0.1:17841/')
  calls = await fake.calls()
  assert.deepEqual(calls.filter(argv => argv[0] === 'web'), [['web', 'open', '--no-browser']])

  // A manager prerequisite is a typed refusal, not a transport failure.
  await fake.patch(current => {
    current.dispatch['web open --no-browser'] = {
      code: 'runtime_blocked',
      status: 'blocked',
      data: null,
      blockers: [{ code: 'daemon_identity_unavailable', message: 'No compatible daemon identity is available.' }],
      __exit: 3,
    }
  })
  const blocked = await postJson(state, '/_tmax/action', { action: 'open_management' })
  assert.equal(blocked.statusCode, 409)
  assert.equal(blocked.body.code, 'runtime_blocked')
  assert.equal(blocked.body.result.blockers[0].code, 'daemon_identity_unavailable')
  await fake.update(baseScenario())
})

test('gateway lifecycle actions preview then execute with exact argv and fresh state', async () => {
  const { fake, state } = await applied()
  await getJson(state, '/_tmax/state') // warm; the fixture gateway is configured
  for (const verb of ['start', 'stop', 'restart']) {
    const action = `gateway_${verb}`
    await fake.clearCalls()
    const preview = await postJson(state, '/_tmax/action', { action, dryRun: true })
    assert.equal(preview.statusCode, 200, `${action}: ${JSON.stringify(preview.body)}`)
    assert.equal(preview.body.ok, true)
    assert.equal(preview.body.action, action)
    assert.equal(preview.body.dryRun, true)
    assert.equal(preview.body.result.code, 'dry_run_ready')
    assert.ok(preview.body.state, 'preview carries state')
    let calls = await fake.calls()
    assert.deepEqual(
      calls.filter(argv => argv[0] === 'gateway' && ['start', 'stop', 'restart'].includes(argv[1])),
      [['gateway', verb, '--dry-run']],
    )

    await fake.clearCalls()
    const executed = await postJson(state, '/_tmax/action', { action })
    assert.equal(executed.statusCode, 200, `${action}: ${JSON.stringify(executed.body)}`)
    assert.equal(executed.body.ok, true)
    assert.equal(executed.body.dryRun, false)
    assert.equal(executed.body.result.code, 'ok')
    calls = await fake.calls()
    assert.deepEqual(
      calls.filter(argv => argv[0] === 'gateway' && ['start', 'stop', 'restart'].includes(argv[1])),
      [['gateway', verb]],
    )
    assert.ok(calls.some(argv => argv[0] === 'agent' && argv[1] === 'inspect'), 'a fresh state read follows the action')
  }
})

test('a gateway action response carries state adopted after the action', async () => {
  const { fake, state } = await applied()
  await getJson(state, '/_tmax/state')
  await fake.patch(current => {
    current.dispatch['gateway status'].data.daemon.discovery_state = 'compatible'
    current.dispatch['gateway status'].data.daemon.reason_code = null
  })
  const executed = await postJson(state, '/_tmax/action', { action: 'gateway_start' })
  assert.equal(executed.statusCode, 200)
  assert.equal(executed.body.state.gateway.daemonDiscoveryState, 'identified', 'compatible maps to the page contract token')
})

test('gateway actions require a configured gateway and a readable state', async () => {
  // A gateway reported as not configured is a typed refusal, never an argv.
  const notConfigured = await applied()
  await notConfigured.fake.patch(current => {
    current.dispatch['agent inspect dsh-main'].data.providers = []
    current.dispatch['agent inspect dsh-main'].data.native_projection = []
  })
  const refused = await postJson(notConfigured.state, '/_tmax/action', { action: 'gateway_start', dryRun: true })
  assert.equal(refused.statusCode, 409)
  assert.equal(refused.body.ok, false)
  assert.equal(refused.body.action, 'gateway_start')
  assert.equal(refused.body.code, 'gateway_not_configured')
  assert.equal(
    (await notConfigured.fake.calls()).some(argv => argv[0] === 'gateway' && ['start', 'stop', 'restart'].includes(argv[1])),
    false,
    'no lifecycle argv runs',
  )

  // Missing gateway evidence is unavailable, not "not configured".
  const service = createActionService({
    config: { target: 'dsh-main' },
    snapshot: {
      state: async () => ({ ok: true, target: { id: 'dsh-main' }, gateway: null, providers: { tmax: [] } }),
      invalidate: () => {},
    },
    log: () => {},
  })
  const unknown = await service.run({ action: 'gateway_restart' })
  assert.equal(unknown.status, 409)
  assert.equal(unknown.body.code, 'gateway_unavailable')

  // A failed inventory leaves gateway facts unknown: unavailable, not "not
  // configured", and still a typed conflict rather than a transport failure.
  const scenario = baseScenario()
  scenario.dispatch['agent inspect dsh-main'] = {
    code: 'unknown_target',
    status: 'error',
    data: null,
    blockers: [{ code: 'unknown_target', message: 'Unknown DSH target "missing".' }],
    __exit: 2,
  }
  const degraded = await applied({ scenario })
  const degradedGateway = await postJson(degraded.state, '/_tmax/action', { action: 'gateway_stop' })
  assert.equal(degradedGateway.statusCode, 409)
  assert.equal(degradedGateway.body.code, 'gateway_unavailable')
  assert.equal(
    (await degraded.fake.calls()).some(argv => argv[0] === 'gateway' && ['start', 'stop', 'restart'].includes(argv[1])),
    false,
    'no lifecycle argv runs',
  )

  // A fully unreadable state is a 502, not a gateway conflict.
  const unreadable = baseScenario()
  for (const key of ['agent inspect dsh-main', 'provider list', 'config validate']) {
    unreadable.dispatch[key] = { __raw: 'not a tmax envelope', __exit: 4 }
  }
  const broken = await applied({ scenario: unreadable })
  const stateUnavailable = await postJson(broken.state, '/_tmax/action', { action: 'gateway_stop' })
  assert.equal(stateUnavailable.statusCode, 502)
  assert.equal(stateUnavailable.body.code, 'state_unavailable')
})

test('gateway lifecycle locks are per verb', async () => {
  const { fake, state } = await applied()
  await fake.update({ ...baseScenario(), delayFor: 'gateway start', delayMs: 150 })
  const first = postJson(state, '/_tmax/action', { action: 'gateway_start' })
  await sleep(60)
  const duplicate = await postJson(state, '/_tmax/action', { action: 'gateway_start' })
  assert.equal(duplicate.statusCode, 409)
  assert.equal(duplicate.body.code, 'busy')
  // A different verb is a different lock.
  const stop = await postJson(state, '/_tmax/action', { action: 'gateway_stop' })
  assert.equal(stop.statusCode, 200)
  assert.equal((await first).statusCode, 200)
  await fake.update(baseScenario())
})

test('a gateway lifecycle refusal is a 409 with blockers and transport is a 502', async () => {
  const { fake, state } = await applied()
  await fake.patch(current => {
    current.dispatch['gateway restart'] = {
      code: 'runtime_blocked',
      status: 'blocked',
      data: null,
      blockers: [{ code: 'daemon_identity_unavailable', message: 'Daemon identity challenge, authority, or process did not match' }],
      __exit: 3,
    }
  })
  const refused = await postJson(state, '/_tmax/action', { action: 'gateway_restart' })
  assert.equal(refused.statusCode, 409)
  assert.equal(refused.body.code, 'runtime_blocked')
  assert.equal(refused.body.result.blockers[0].code, 'daemon_identity_unavailable')
  assert.ok(refused.body.state)

  await fake.patch(current => {
    current.dispatch['gateway restart'] = { __raw: 'not a tmax envelope', __exit: 4 }
  })
  const transport = await postJson(state, '/_tmax/action', { action: 'gateway_restart' })
  assert.equal(transport.statusCode, 502)
  assert.equal(transport.body.code, 'tmax_unavailable')
  assert.ok(transport.body.state)
  await fake.update(baseScenario())
})

test('unknown providers and non-enum scopes are refused before any action argv', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()

  const unknown = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'nope' })
  assert.equal(unknown.statusCode, 409)
  assert.equal(unknown.body.ok, false)
  assert.equal(unknown.body.code, 'unknown_provider')
  assert.equal(unknown.body.action, 'observe_provider')

  const injection = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi --dry-run' })
  assert.equal(injection.statusCode, 409)
  assert.equal(injection.body.code, 'unknown_provider')

  const spaced = await postJson(state, '/_tmax/action', { action: 'reset_failure', provider: ' kimi' })
  assert.equal(spaced.statusCode, 409)
  assert.equal(spaced.body.code, 'unknown_provider')

  const badScope = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi', scope: 'everything' })
  assert.equal(badScope.statusCode, 400)
  assert.equal(badScope.body.code, 'invalid_scope')

  const badAction = await postJson(state, '/_tmax/action', { action: 'delete_everything', provider: 'kimi' })
  assert.equal(badAction.statusCode, 400)
  assert.equal(badAction.body.code, 'invalid_action')

  const calls = await fake.calls()
  const actionArgvs = calls.filter(argv =>
    (argv[0] === 'provider' && ['refresh', 'quota-resume', 'failure-reset'].includes(argv[1]))
    || (argv[0] === 'web')
    || (argv[0] === 'agent' && argv[1] === 'apply'))
  assert.deepEqual(actionArgvs, [], 'no refused request reached an action argv')
})

test('a typed tmax refusal is a 409 and a missing envelope is a 502', async () => {
  const { fake, state } = await applied()

  await fake.patch(current => {
    current.dispatch['provider quota-resume kimi'] = {
      code: 'unknown_provider',
      status: 'error',
      data: null,
      blockers: [{ code: 'unknown_provider', message: 'Unknown provider "kimi".' }],
      __exit: 2,
    }
  })
  const refused = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi' })
  assert.equal(refused.statusCode, 409)
  assert.equal(refused.body.ok, false)
  assert.equal(refused.body.action, 'resume_quota')
  assert.equal(refused.body.dryRun, false)
  assert.equal(refused.body.code, 'unknown_provider')
  assert.equal(refused.body.message, 'Unknown provider "kimi".')
  assert.equal(refused.body.result.code, 'unknown_provider')
  assert.equal(refused.body.result.status, 'error')
  assert.equal(refused.body.result.message, 'Unknown provider "kimi".', 'the refusal message survives on the result')
  assert.equal(refused.body.result.blockers[0].code, 'unknown_provider')
  assert.ok(refused.body.state, 'a typed refusal still carries fresh state')

  // A refused preview is a refusal too, not a success.
  await fake.patch(current => {
    current.dispatch['provider refresh kimi --scope quota --dry-run'] = {
      code: 'provider_unqualified',
      status: 'blocked',
      data: null,
      blockers: [{ code: 'provider_unqualified', message: 'Kimi needs sign-in before quota can be observed.' }],
      __exit: 3,
    }
  })
  const blockedPreview = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi', dryRun: true })
  assert.equal(blockedPreview.statusCode, 409)
  assert.equal(blockedPreview.body.dryRun, true)
  assert.equal(blockedPreview.body.result.code, 'provider_unqualified')
  assert.equal(blockedPreview.body.result.blockers[0].code, 'provider_unqualified')

  await fake.patch(current => {
    current.dispatch['provider quota-resume kimi'] = { __raw: 'not a tmax envelope', __exit: 4 }
  })
  const transport = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi' })
  assert.equal(transport.statusCode, 502)
  assert.equal(transport.body.ok, false)
  assert.equal(transport.body.code, 'tmax_unavailable')
  assert.equal(transport.body.result.code, 'tmax_unavailable')
  assert.equal(transport.body.result.status, 'error')
  assert.ok(transport.body.state, 'a transport failure still carries state')
  await fake.update(baseScenario())
})

test('observations share one lock and reject duplicates as busy', async () => {
  const { fake, state } = await applied()
  await fake.update({ ...baseScenario(), delayFor: 'provider refresh', delayMs: 150 })

  const first = postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  await sleep(60)
  const duplicate = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  assert.equal(duplicate.statusCode, 409)
  assert.equal(duplicate.body.ok, false)
  assert.equal(duplicate.body.code, 'busy')
  assert.equal(duplicate.body.action, 'observe_provider')

  // Every observation is one operation: another provider is not a new lock.
  const otherProvider = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'cpa-us2' })
  assert.equal(otherProvider.statusCode, 409)
  assert.equal(otherProvider.body.code, 'busy')
  const firstResult = await first
  assert.equal(firstResult.statusCode, 200)
  await fake.update(baseScenario())
})

test('persisted repairs are independent per provider and per action', async () => {
  const { fake, state } = await applied()
  await fake.update({ ...baseScenario(), delayFor: 'provider quota-resume', delayMs: 150 })

  const resume = postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi' })
  await sleep(60)
  // Another provider is a different lock...
  const otherProvider = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'cpa-us2' })
  assert.equal(otherProvider.statusCode, 200)
  // ...and so is another repair for the same provider.
  const sameProviderOtherAction = await postJson(state, '/_tmax/action', { action: 'reset_failure', provider: 'kimi' })
  assert.equal(sameProviderOtherAction.statusCode, 200)
  const resumed = await resume
  assert.equal(resumed.statusCode, 200)
  await fake.update(baseScenario())
})

test('legacy routes and actions share the connect and observation locks', async () => {
  const { fake, state } = await applied()
  await getJson(state, '/_tmax/state')

  // A legacy Connect holds the shared lock; the action connect is busy.
  await fake.update({ ...baseScenario(), delayFor: 'agent apply', delayMs: 150 })
  const legacyConnect = postJson(state, '/_tmax/connect', { dryRun: true })
  await sleep(60)
  const actionConnect = await postJson(state, '/_tmax/action', { action: 'connect', dryRun: true })
  assert.equal(actionConnect.statusCode, 409)
  assert.equal(actionConnect.body.code, 'busy')
  assert.equal((await legacyConnect).statusCode, 200)

  // The reverse direction: an action connect blocks the legacy route.
  const actionFirst = postJson(state, '/_tmax/action', { action: 'connect', dryRun: true })
  await sleep(60)
  const legacySecond = await postJson(state, '/_tmax/connect', { dryRun: true })
  assert.equal(legacySecond.statusCode, 409)
  assert.equal(legacySecond.body.code, 'busy')
  assert.equal((await actionFirst).statusCode, 200)

  // Legacy observe and action observe_provider are one observation lock.
  await fake.update({ ...baseScenario(), delayFor: 'provider list', delayMs: 150 })
  const legacyObserve = postJson(state, '/_tmax/observe', {})
  await sleep(60)
  const actionObserve = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  assert.equal(actionObserve.statusCode, 409)
  assert.equal(actionObserve.body.code, 'busy')
  const legacyResult = await legacyObserve
  assert.ok(legacyResult.statusCode === 200 || legacyResult.statusCode === 207)

  // And an action observation blocks the legacy observe route.
  await fake.update({ ...baseScenario(), delayFor: 'provider refresh', delayMs: 150 })
  const actionObserveFirst = postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  await sleep(60)
  const legacyObserveSecond = await postJson(state, '/_tmax/observe', {})
  assert.equal(legacyObserveSecond.statusCode, 409)
  assert.equal(legacyObserveSecond.body.code, 'busy')
  assert.equal((await actionObserveFirst).statusCode, 200)
  await fake.update(baseScenario())
})

test('an action response carries state adopted after the action', async () => {
  const { fake, state } = await applied()
  await getJson(state, '/_tmax/state')
  // The cached state is stale; only a read after the action may show this.
  await fake.patch(current => {
    current.dispatch['provider inspect kimi'].data.observations.billing.observation.observed_at = '2026-11-23T12:34:56Z'
  })
  const executed = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  assert.equal(executed.statusCode, 200)
  assert.equal(executed.body.state.providers.tmax[0].billing.observedAt, '2026-11-23T12:34:56Z')
  const inspects = (await fake.calls()).filter(argv => argv[0] === 'agent' && argv[1] === 'inspect')
  assert.ok(inspects.length >= 2, 'the post-action state read is a fresh inventory read')
})

test('action results never carry key material', async () => {
  const { fake, state } = await applied()
  const secret = 'sk-live-ACTION-SECRET'
  await fake.patch(current => {
    current.dispatch['provider quota-resume kimi'] = {
      code: 'ok',
      status: 'success',
      data: {
        provider: 'kimi',
        requested: 'provider quota-resume',
        resumed: true,
        key: secret,
        token: { value: secret },
        nested: { password: secret },
        reference: 'TMAX_TEST',
      },
    }
  })
  const result = await postJson(state, '/_tmax/action', { action: 'resume_quota', provider: 'kimi' })
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.result.data.reference, 'TMAX_TEST')
  assert.equal('key' in result.body.result.data, false)
  assert.equal('token' in result.body.result.data, false)
  assert.equal('password' in result.body.result.data.nested, false)
  assert.equal(JSON.stringify(result.body).includes(secret), false)
  await fake.update(baseScenario())
})

test('a provider action is refused when the current state is unavailable', async () => {
  const service = createActionService({
    config: { home: '/tmp/tmax-actions-home' },
    snapshot: {
      state: async () => { throw new Error('state unavailable') },
      invalidate: () => {},
    },
    log: () => {},
  })
  const outcome = await service.run({ action: 'observe_provider', provider: 'kimi' })
  assert.equal(outcome.status, 502)
  assert.equal(outcome.body.ok, false)
  assert.equal(outcome.body.code, 'state_unavailable')
  assert.equal(outcome.body.action, 'observe_provider')
})

test('a provider action uses readable provider evidence and refuses only an unreadable state', async () => {
  // A failed inventory still leaves the provider list readable: the action is
  // about a provider, so it proceeds and returns state with the target unknown.
  const scenario = baseScenario()
  scenario.dispatch['agent inspect dsh-main'] = {
    code: 'unknown_target',
    status: 'error',
    data: null,
    blockers: [{ code: 'unknown_target', message: 'Unknown DSH target "missing".' }],
    __exit: 2,
  }
  const { fake, state } = await applied({ scenario })
  await fake.clearCalls()
  const unavailable = await getJson(state, '/_tmax/state')
  assert.equal(unavailable.body.ok, true, 'the provider section answered')
  assert.equal(unavailable.body.target, null)
  const action = await postJson(state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  assert.equal(action.statusCode, 200)
  assert.ok(
    (await fake.calls()).some(argv => argv[0] === 'provider' && argv[1] === 'refresh'),
    'a readable provider is addressable even when the target read failed',
  )

  // A fully unreadable state cannot prove which providers exist: a 502.
  const unreadable = baseScenario()
  for (const key of ['agent inspect dsh-main', 'provider list', 'config validate']) {
    unreadable.dispatch[key] = { __raw: 'not a tmax envelope', __exit: 4 }
  }
  const broken = await applied({ scenario: unreadable })
  await broken.fake.clearCalls()
  const refused = await postJson(broken.state, '/_tmax/action', { action: 'observe_provider', provider: 'kimi' })
  assert.equal(refused.statusCode, 502)
  assert.equal(refused.body.ok, false)
  assert.equal(refused.body.code, 'state_unavailable')
  assert.equal(refused.body.action, 'observe_provider')
  const calls = await broken.fake.calls()
  assert.equal(
    calls.some(argv => argv[0] === 'provider' && argv[1] === 'refresh'),
    false,
    'no observation runs against an unreadable state',
  )
})

test('a completed action-form connect never claims settings the write did not make', async () => {
  const { state } = await applied()
  const before = (await getJson(state, '/_tmax/state')).body
  assert.equal(before.ownership.owner, 'unapplied')

  const result = await postJson(state, '/_tmax/action', { action: 'connect', dryRun: false })
  assert.equal(result.statusCode, 200)
  assert.equal(
    result.body.state.ownership.owner,
    'unapplied',
    'the page reports the settings it can see, not the ones it asked for',
  )
})

test('the action-form connect resolves the target from the DSH home when none is configured', async () => {
  // No config.target: the state service resolves the single matching target,
  // and the action applies that same target.
  const { fake, state } = await applied({ config: { target: undefined } })
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/action', { action: 'connect', dryRun: true })
  assert.equal(preview.statusCode, 200, JSON.stringify(preview.body))
  const applyCall = (await fake.calls()).find(argv => argv[0] === 'agent' && argv[1] === 'apply')
  assert.deepEqual(applyCall, ['agent', 'apply', 'dsh-main', '--dry-run'])

  await fake.clearCalls()
  const executed = await postJson(state, '/_tmax/action', { action: 'connect' })
  assert.equal(executed.statusCode, 200, JSON.stringify(executed.body))
  assert.deepEqual(
    (await fake.calls()).find(argv => argv[0] === 'agent' && argv[1] === 'apply'),
    ['agent', 'apply', 'dsh-main'],
  )
})

test('the action route keeps the shared POST guards and works beside the legacy routes', async () => {
  const { state } = await applied()
  const route = routeOf(state, '/_tmax/action')
  assert.equal((await callRoute(route, 'GET', {})).statusCode, 405)
  assert.equal((await callRoute(route, 'POST', { body: '{}' })).statusCode, 415)
  const crossOrigin = await postJson(state, '/_tmax/action', { action: 'open_management' }, { origin: 'http://evil.test' })
  assert.equal(crossOrigin.statusCode, 403)

  // Legacy Connect and the action route are one table; neither replaces the other.
  assert.ok(routeOf(state, '/_tmax/connect'))
  assert.ok(routeOf(state, '/_tmax/observe'))
  const connect = await postJson(state, '/_tmax/connect', { dryRun: true })
  assert.equal(connect.statusCode, 200)
  assert.equal(connect.body.ok, true)
})

// ---------------------------------------------------------------------------
// Target-selection add: one previewed composite, exact argv, stop and report.

test('add_model previews a catalog-derived plan without writing anything', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: true,
  })
  assert.equal(preview.statusCode, 200)
  assert.equal(preview.body.ok, true)
  assert.equal(preview.body.dryRun, true)
  assert.equal(preview.body.result.status, 'dry_run')
  assert.deepEqual(preview.body.result.data, {
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    target: 'dsh-main',
    entry: { name: 'kimi', mode: 'create' },
    steps: ['provider_add', 'model_add', 'apply'],
  })
  // A preview reads state only; it never writes desired or native configuration.
  const calls = await fake.calls()
  assert.equal(calls.some(argv => argv[0] === 'agent' && ['provider', 'model', 'apply'].includes(argv[1])), false)
  assert.equal(preview.body.state.schemaVersion, 3)
  assert.equal(preview.body.state.catalog.state, 'available')
})

test('add_model executes the fixed provider, model, apply sequence and reports every step', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const executed = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: false,
  })
  assert.equal(executed.statusCode, 200)
  assert.equal(executed.body.ok, true)
  assert.equal(executed.body.result.status, 'success')
  assert.equal(executed.body.result.code, 'awaiting_native_reload')
  assert.deepEqual(executed.body.result.data, {
    target: 'dsh-main',
    agent: 'dsh',
    path: '/tmp/dsh',
    state: 'applied_reload_pending',
    written: true,
    changed_files: ['/tmp/dsh/cordis.patch.yml'],
    unchanged_files: [],
    recovery: 'recovery-1',
    fact_gaps: [],
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    entry: { name: 'kimi', mode: 'create' },
    steps: [
      { name: 'provider_add', code: 'ok', status: 'success' },
      { name: 'model_add', code: 'ok', status: 'success' },
      { name: 'apply', code: 'awaiting_native_reload', status: 'success' },
    ],
  })
  assert.equal(JSON.stringify(executed.body.result).includes('sk-live-CONNECT-SECRET'), false, 'apply evidence is secret-free')
  const writes = (await fake.calls()).filter(argv => argv[0] === 'agent' && ['provider', 'model', 'apply'].includes(argv[1]))
  assert.deepEqual(writes, [
    ['agent', 'provider', 'add', 'dsh-main', 'kimi', '--provider', 'kimi'],
    ['agent', 'model', 'add', 'dsh-main', 'kimi', 'k3.1-pro', '--protocol', 'responses'],
    ['agent', 'apply', 'dsh-main'],
  ])
  assert.equal(executed.body.state.schemaVersion, 3)
})

test('add_model reuses an existing entry whose protocol matches', async () => {
  const scenario = baseScenario()
  scenario.dispatch['agent inspect dsh-main'] = success({
    ...agentInspectData(),
    providers: [
      ...agentInspectData().providers,
      {
        name: 'kimi-entry',
        source: { provider: 'kimi' },
        relayed: false,
        base_url: null,
        credential_ref: null,
        applied_delivery: null,
        models: [{ id: 'k3.1-pro', protocol: 'responses' }],
      },
    ],
  })
  const listed = modelListEnvelope()
  listed.data.models.push({ id: 'kimi/kimi-other', kind: 'concrete', enabled: true, availability: 'unverified', published_protocols: ['responses'] })
  scenario.dispatch['model list'] = listed
  const { state } = await applied({ scenario })
  const already = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: true,
  })
  assert.equal(already.statusCode, 409)
  assert.equal(already.body.code, 'already_joined')
  const preview = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'kimi-other',
    protocol: 'responses',
    dryRun: true,
  })
  assert.equal(preview.statusCode, 200)
  assert.deepEqual(preview.body.result.data.entry, { name: 'kimi-entry', mode: 'existing' })
  assert.deepEqual(preview.body.result.data.steps, ['model_add', 'apply'])
})

test('add_model refuses unknown, unsupported, and already-selected requests before argv', async () => {
  const { state } = await applied()
  for (const [body, code] of [
    [{ provider: 'missing', model: 'k3.1-pro', protocol: 'responses' }, 'unknown_provider'],
    [{ provider: 'kimi', model: 'missing', protocol: 'responses' }, 'unknown_model'],
    [{ provider: 'cpa-us2', model: 'gpt-6-astra', protocol: 'messages' }, 'unsupported_protocol'],
    [{ provider: 'kimi', model: 'k3.1-pro', protocol: 'bogus' }, 'invalid_protocol'],
  ]) {
    const result = await postJson(state, '/_tmax/action', { action: 'add_model', ...body, dryRun: false })
    assert.equal(result.body.ok, false)
    assert.equal(result.body.code, code)
    assert.equal(result.body.state, undefined)
  }
})

test('add_model stops at a refused step and reports the completed prefix', async () => {
  const scenario = baseScenario()
  delete scenario.dispatch['agent model add dsh-main kimi k3.1-pro --protocol responses']
  const { state } = await applied({ scenario })
  const executed = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: false,
  })
  assert.equal(executed.statusCode, 409)
  assert.equal(executed.body.ok, false)
  assert.equal(executed.body.result.data.failedStep, 'model_add')
  assert.deepEqual(executed.body.result.data.steps, [
    { name: 'provider_add', code: 'ok', status: 'success' },
    { name: 'model_add', code: 'unknown_resource', status: 'error' },
  ])
  assert.ok(executed.body.state, 'the fresh state accompanies a partial write')
})

test('add_model refuses a disabled provider before any argv', () => {
  const state = {
    catalog: {
      state: 'available',
      entriesState: 'available',
      providers: [{
        id: 'disabled',
        name: 'disabled',
        source: 'upstream',
        enabled: false,
        entry: null,
        entries: [],
        joinedCount: 0,
        totalCount: 1,
        models: [{ id: 'm1', kind: 'concrete', enabled: true, protocols: ['responses'], joined: false, joinedProtocols: [] }],
      }],
    },
    gateway: { configured: true },
    target: { pendingNativeChanges: [] },
  }
  const planned = planAddModel({ action: 'add_model', provider: 'disabled', model: 'm1', protocol: 'responses' }, { state, target: 'dsh-main' })
  assert.equal(planned.ok, false)
  assert.equal(planned.code, 'provider_disabled')
})

test('add_model refuses a target that already has pending native changes', async () => {
  const scenario = baseScenario()
  scenario.dispatch['agent inspect dsh-main'] = success({
    ...agentInspectData(),
    applied_state: 'pending_native_changes',
    pending_native_changes: [{ kind: 'default', id: 'unrelated' }],
  })
  const { fake, state } = await applied({ scenario })
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: true,
  })
  assert.equal(preview.statusCode, 409)
  assert.equal(preview.body.code, 'pending_native_changes')
  assert.equal('result' in preview.body, false, 'a planned refusal has no tmax result envelope')
  assert.equal((await fake.calls()).some(argv => argv[0] === 'agent' && argv[1] === 'provider'), false)
})

test('add_model preserves the apply refusal evidence and reports the failed apply step', async () => {
  const { fake, state } = await applied()
  await fake.patch(current => {
    current.dispatch['agent apply dsh-main'] = {
      code: 'partial_native_apply',
      status: 'error',
      data: {
        state: 'partially_applied',
        changed_files: ['/tmp/dsh/cordis.patch.yml'],
        recovery: 'recovery-2',
      },
      effects: { native_writes: true, locks_created: false },
      blockers: [{ code: 'partial_native_apply', message: 'one file failed' }],
    }
  })
  await fake.clearCalls()
  const executed = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: false,
  })
  assert.equal(executed.statusCode, 409)
  assert.equal(executed.body.code, 'partial_native_apply')
  assert.equal(executed.body.result.data.state, 'partially_applied')
  assert.deepEqual(executed.body.result.data.changed_files, ['/tmp/dsh/cordis.patch.yml'])
  assert.equal(executed.body.result.data.recovery, 'recovery-2')
  assert.equal(executed.body.result.data.failedStep, 'apply')
  assert.equal(executed.body.result.data.entry.name, 'kimi')
  assert.deepEqual(executed.body.result.effects, { native_writes: true, locks_created: false })
})

test('add_model refuses an unreadable pending target state separately from known pending changes', async () => {
  const scenario = baseScenario()
  scenario.dispatch['agent inspect dsh-main'] = success({
    ...agentInspectData(),
    applied_state: 'unknown',
    pending_native_changes: null,
  })
  const { fake, state } = await applied({ scenario })
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/action', {
    action: 'add_model',
    provider: 'kimi',
    model: 'k3.1-pro',
    protocol: 'responses',
    dryRun: true,
  })
  assert.equal(preview.statusCode, 409)
  assert.equal(preview.body.code, 'target_uninspectable')
  assert.equal((await fake.calls()).some(argv => argv[0] === 'agent' && argv[1] === 'provider'), false)
})
