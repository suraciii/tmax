import { parseBlocks } from './fake-yaml.mjs'
import assert from 'node:assert/strict'
import { chmod, mkdir, mkdtemp, readFile, symlink, writeFile } from 'node:fs/promises'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { apply } from '../index.js'
import { CONNECT_TIMEOUT_MS, INVENTORY_TIMEOUT_MS, REFRESH_TIMEOUT_MS, resolveHome } from '../host/cli.js'
import { composeInventory, readInventory } from '../host/inventory.js'
import { providerIds } from '../host/ownership.js'
import { normalizeDaemonDiscovery, normalizeSharedKeyState } from '../host/snapshot.js'

// ---------------------------------------------------------------------------
// Fixtures: the reviewed state contract and the latest CLI envelopes behind it.

const fixture = JSON.parse(await readFile(new URL('./fixtures/state.json', import.meta.url), 'utf8'))
const fixtureCase = fixture.cases.connectedHomeOwned

/** The one target this DSH home declares. `__HOME__` is substituted per test. */
function agentList() {
  return {
    code: 'ok',
    status: 'success',
    data: {
      agents: [{
        target: 'dsh-main',
        agent: 'dsh',
        path: '__HOME__',
        provider_count: 1,
        model_count: 3,
        default_configured: true,
      }],
    },
  }
}

/** `tmax agent inspect TARGET`: native entries, delivery facts, capabilities. */
function agentInspect(overrides = {}) {
  return {
    code: 'ok',
    status: 'success',
    data: {
      target: 'dsh-main',
      agent: 'dsh',
      path: '/tmp/dsh',
      scope_state: 'available',
      scope_reason_code: null,
      configuration_revision: 'rev-1',
      providers: [{
        name: 'tmax',
        source: { provider: 'tmax' },
        relayed: true,
        base_url: 'http://127.0.0.1:17840',
        credential_ref: 'TMAX_TEST',
        applied_delivery: null,
        models: [
          { id: 'public-model', protocol: 'responses' },
          { id: 'text-only', protocol: 'responses' },
          { id: 'no-context', protocol: 'responses' },
        ],
      }],
      default: { provider: 'tmax', id: 'deepseek-v4.1-flash', protocol: 'responses', effort: 'max' },
      roles: {},
      native_projection: [{
        name: 'tmax',
        source: { provider: 'tmax' },
        models: [
          {
            id: 'public-model',
            protocol: 'responses',
            capabilities: { contextWindow: 200000, input: ['text', 'image'], reasoningEfforts: { max: 'max' } },
            efforts: ['max'],
            projection: { fields: {} },
          },
          { id: 'text-only', protocol: 'responses', capabilities: { contextWindow: 1000 }, efforts: [], projection: { fields: {} } },
          { id: 'no-context', protocol: 'responses', capabilities: {}, efforts: [], projection: { fields: {} } },
        ],
      }],
      native_default: {
        state: 'available',
        fields: { provider: 'tmax', model: 'deepseek-v4.1-flash', effort: 'max', present: true },
      },
      installation: {
        executable: '/tmp/bin/dsh',
        version: '0.2.3-alpha.2',
        state: 'detected',
        source: 'https://github.com/deepseek-ai/deepseek-harness',
      },
      applied_state: 'unknown',
      pending_native_changes: null,
      blockers: [{
        code: 'gateway_not_ready',
        field: null,
        message: 'Explicitly start the configured gateway and activate the matching complete snapshot and shared key before native application',
      }],
      ...overrides,
    },
  }
}

/** `tmax gateway status`: daemon identity, gateway readiness, supervision. */
function gatewayStatus(overrides = {}) {
  return {
    code: 'ok',
    status: 'success',
    data: {
      daemon: {
        discovery_state: 'unidentified',
        reason_code: 'daemon_identity_unavailable',
        identity: null,
        inference: null,
        web: null,
        caller_version: '0.1.0',
      },
      gateway: { ready: false, listen: null, key_ref: null, config_revision: null },
      supervision: { state: 'unavailable', reason_code: 'service_manager_unavailable' },
      autostart: { saved_intent: true, registration: 'unavailable', unit_matches: false },
      shared_key: { loaded_reference: null, loaded_state: 'absent' },
      publication: { snapshot_state: 'absent', model_count: null },
      ...overrides,
    },
  }
}

/** `tmax status`: authority (data directory) and the answering version. */
function tmaxStatus(overrides = {}) {
  return {
    code: 'ok',
    status: 'success',
    data: {
      config_path: '/tmp/tmax/tmax.toml',
      file_revision: 'rev-1',
      configuration_state: 'valid',
      providers: 2,
      targets: 1,
      daemon: gatewayStatus().data,
      failures: null,
      ...overrides,
    },
  }
}

function providerList() {
  return {
    code: 'ok',
    status: 'success',
    data: {
      revision: 'rev-1',
      providers: [
        {
          id: 'kimi',
          name: 'Kimi Coding Plan',
          enabled: true,
          authentication: 'api-key',
          source: 'upstream',
          affected_targets: 3,
          observations: {
            billing_kind: 'plan_allowance',
            billing_state: 'observed',
            observations: [{ scope: 'quota', state: 'observed', last_attempt_outcome: 'completed', last_attempt_reason_code: null }],
          },
          quota_suspension: { state: 'unsupported' },
        },
        {
          id: 'cpa-us2',
          name: null,
          enabled: true,
          authentication: 'api-key',
          source: 'upstream',
          affected_targets: 3,
          observations: {
            billing_kind: 'unknown',
            billing_state: 'unknown',
            observations: [{ scope: 'quota', state: 'unknown', last_attempt_outcome: 'unavailable', last_attempt_reason_code: 'unsupported_source' }],
          },
          quota_suspension: { state: 'unsupported' },
        },
        {
          id: 'tmax',
          name: 'tmax gateway',
          enabled: true,
          authentication: 'shared-key',
          source: 'gateway',
          affected_targets: 1,
          observations: {
            billing_kind: 'unknown',
            billing_state: 'unknown',
            observations: [{ scope: 'quota', state: 'unknown', last_attempt_outcome: 'unavailable', last_attempt_reason_code: 'unsupported_source' }],
          },
          quota_suspension: { state: 'unsupported' },
        },
      ],
    },
  }
}

function kimiInspect() {
  return {
    code: 'ok',
    status: 'success',
    data: {
      id: 'kimi',
      source: 'upstream',
      provider: {
        enabled: true,
        integration: 'kimi-code-plan-cn',
        overrides: { name: 'Kimi Coding Plan' },
        auth: { method: 'api-key' },
        subscription: null,
      },
      observations: {
        account_identity: null,
        billing: {
          kind: 'plan_allowance',
          state: 'observed',
          dashboard: 'https://www.kimi.com/code/console',
          freshness: { state: 'unqualified' },
          observation: { observed_at: '2026-11-23T06:26:05Z' },
          display: {
            notes: ['counters_unit_unqualified', 'usage_and_limits_may_overlap'],
            rows: [{
              label: { key: 'weekly_allowance' },
              basis: 'counters',
              counters: { limit: '100', remaining: '100' },
              reset: { kind: 'provider_timestamp', at: '2026-11-26T15:42:16Z' },
            }],
          },
        },
      },
      quota_suspension: { state: 'unsupported' },
    },
  }
}

function cpaInspect() {
  return {
    code: 'ok',
    status: 'success',
    data: {
      id: 'cpa-us2',
      source: 'upstream',
      provider: { enabled: true, integration: 'custom', overrides: {}, auth: { method: 'api-key' }, subscription: null },
      observations: {
        account_identity: null,
        billing: { kind: 'unknown', state: 'unknown', dashboard: '', freshness: { state: 'missing' }, observation: null, display: null },
      },
      quota_suspension: { state: 'unsupported' },
      failure_state: { state: 'not_configured', consecutive_failures: 0, active_priority: null, expires_at: null, reason_code: null },
    },
  }
}

function configValidate() {
  return { code: 'ok', status: 'success', data: { valid: true, revision: 'rev-1', snapshot_state: 'available' } }
}

function explain(id) {
  if (id === 'public-model') {
    return {
      code: 'ok',
      status: 'success',
      data: {
        selection: { qualified_model: 'opencode-go-2/public-model', provider: 'opencode-go-2', model: 'public-model', priority: 1 },
        priority_evaluations: [
          { qualified_model: 'opencode-go/public-model', priority_evaluation: { effective_priority: 0 } },
          { qualified_model: 'opencode-go-2/public-model', priority_evaluation: { effective_priority: 1 } },
        ],
        excluded_candidates: [],
      },
    }
  }
  if (id === 'text-only') {
    return {
      code: 'ok',
      status: 'success',
      data: {
        selection: null,
        priority_evaluations: [],
        excluded_candidates: [{ qualified_model: 'elsewhere/text-only', reason_code: 'compatibility_unavailable' }],
      },
    }
  }
  return { code: 'ok', status: 'success', data: { selection: null, priority_evaluations: [], excluded_candidates: [] } }
}

function refreshOk(id) {
  return {
    code: 'ok',
    status: 'success',
    data: { provider: id, checks: [{ scope: 'quota', outcome: 'completed', reason_code: null, published: true }] },
  }
}

function refreshDryRun(id) {
  return {
    code: 'dry_run_ready',
    status: 'dry_run',
    data: { provider: id, checks: [{ scope: 'quota', outcome: 'not_executed', reason_code: null, publication: 'not_performed' }] },
  }
}

function refreshFailed(id) {
  return {
    code: 'observation_failed',
    status: 'incomplete',
    data: { provider: id, checks: [{ scope: 'quota', outcome: 'failed', reason_code: 'transport_error', published: false }] },
    __exit: 4,
  }
}

function refreshUnsupported(id) {
  return {
    code: 'observation_failed',
    status: 'incomplete',
    data: { provider: id, checks: [{ scope: 'quota', outcome: 'unavailable', reason_code: 'unsupported_source', published: false }] },
    __exit: 4,
  }
}

/** `tmax agent apply TARGET --dry-run`: a completed preview, never a write. */
function connectDryRun() {
  return {
    code: 'dry_run_ready',
    status: 'dry_run',
    data: {
      target: 'dsh-main',
      agent: 'dsh',
      path: '/tmp/dsh',
      dry_run: true,
      files: [{ path: '/tmp/dsh/cordis.patch.yml', revision: 'a' }],
      credentials: { resolved: false },
      effects: { native_writes: false, secret_reads: false, processes_started: false, locks_created: false },
      key: 'sk-live-CONNECT-SECRET',
      token: { value: 'sk-live-CONNECT-SECRET' },
      nested: { password: 'sk-live-CONNECT-SECRET' },
      reference: 'TMAX_TEST',
    },
  }
}

/** `tmax agent apply TARGET`: a completed write reports `awaiting_native_reload`. */
function connectExec() {
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
      key: 'sk-live-CONNECT-SECRET',
      token: { value: 'sk-live-CONNECT-SECRET' },
      nested: { password: 'sk-live-CONNECT-SECRET' },
      reference: 'TMAX_TEST',
    },
  }
}

/** `tmax model list`: every configured concrete model and gateway alias. */
function modelList() {
  const concrete = (provider, id, protocols) => ({
    id: `${provider}/${id}`,
    kind: 'concrete',
    enabled: true,
    availability: 'unverified',
    published_protocols: protocols,
  })
  const alias = (id, protocols) => ({
    id,
    kind: 'alias',
    enabled: true,
    availability: 'unverified',
    protocols,
    published_protocols: protocols,
  })
  return {
    code: 'ok',
    status: 'success',
    data: {
      models: [
        concrete('kimi', 'k3.1-pro', ['chat-completions', 'responses']),
        concrete('cpa-us2', 'gpt-6-astra', ['chat-completions', 'responses']),
        alias('public-model', ['chat-completions', 'responses']),
        alias('text-only', ['responses']),
        alias('no-context', ['responses']),
        alias('deepseek-v4.1-flash', ['chat-completions', 'responses']),
      ],
    },
  }
}

function baseScenario() {
  return {
    agentList: agentList(),
    agentInspect: agentInspect(),
    gatewayStatus: gatewayStatus(),
    tmaxStatus: tmaxStatus(),
    providerList: providerList(),
    modelList: modelList(),
    inspect: { kimi: kimiInspect(), 'cpa-us2': cpaInspect() },
    configValidate: configValidate(),
    explain: {
      'public-model': explain('public-model'),
      'text-only': explain('text-only'),
      'no-context': explain('no-context'),
    },
    refresh: {
      kimi: { exec: refreshOk('kimi'), dryRun: refreshDryRun('kimi') },
      'cpa-us2': { exec: refreshOk('cpa-us2'), dryRun: refreshDryRun('cpa-us2') },
    },
    connect: { dryRun: connectDryRun(), exec: connectExec() },
  }
}

/** A second relayed route, for multi-route ownership and credential tests. */
function withSecondRoute(scenario, { name = 'other', credentialRef = 'TMAX_OTHER', contextWindow = 1000 } = {}) {
  const data = scenario.agentInspect.data
  data.providers.push({
    name,
    source: { provider: 'tmax' },
    relayed: true,
    base_url: 'http://127.0.0.1:17840',
    credential_ref: credentialRef,
    applied_delivery: null,
    models: [{ id: `${name}-model`, protocol: 'responses' }],
  })
  data.native_projection.push({
    name,
    source: { provider: 'tmax' },
    models: [{
      id: `${name}-model`,
      protocol: 'responses',
      capabilities: { contextWindow },
      efforts: [],
      projection: { fields: {} },
    }],
  })
  scenario.explain[`${name}-model`] = explain('public-model')
  return scenario
}

// ---------------------------------------------------------------------------
// A fake tmax executable that dispatches on the latest command surface and
// records every call. `__HOME__` is replaced with the test's DSH home.

const FAKE_SCRIPT = `#!/usr/bin/env node
const fs = require('node:fs')
const scenarioPath = __SCENARIO__
const logPath = __LOG__
const argv = process.argv.slice(2)
fs.appendFileSync(logPath, JSON.stringify(argv) + "\\n")
const scenario = JSON.parse(fs.readFileSync(scenarioPath, 'utf8'))
function pick() {
  const a = argv
  const dry = a.includes('--dry-run')
  if (a[0] === 'agent' && a[1] === 'list') return scenario.agentList
  if (a[0] === 'agent' && a[1] === 'inspect') {
    if (a.includes('--home')) {
      return { code: 'invalid_arguments', status: 'error', data: null, message: 'Unknown option --home.', __exit: 2 }
    }
    return scenario.agentInspect
  }
  if (a[0] === 'agent' && a[1] === 'apply') return scenario.connect && scenario.connect[dry ? 'dryRun' : 'exec']
  if (a[0] === 'gateway' && a[1] === 'status') return scenario.gatewayStatus
  if (a[0] === 'status') return scenario.tmaxStatus
  if (a[0] === 'provider' && a[1] === 'list') return scenario.providerList
  if (a[0] === 'model' && a[1] === 'list') return scenario.modelList
  if (a[0] === 'provider' && a[1] === 'inspect') return scenario.inspect && scenario.inspect[a[2]]
  if (a[0] === 'provider' && a[1] === 'refresh') {
    const entry = scenario.refresh && scenario.refresh[a[2]]
    return entry && entry[dry ? 'dryRun' : 'exec']
  }
  if (a[0] === 'provider' && a[1] === 'quota-resume') return scenario.quotaResume && scenario.quotaResume[dry ? 'dryRun' : 'exec']
  if (a[0] === 'provider' && a[1] === 'failure-reset') return scenario.failureReset && scenario.failureReset[dry ? 'dryRun' : 'exec']
  if (a[0] === 'config' && a[1] === 'validate') return scenario.configValidate
  if (a[0] === 'model' && a[1] === 'explain') return scenario.explain && scenario.explain[a[2]]
  if (a[0] === 'history' && a[1] === 'list') return scenario.history
  if (a[0] === 'gateway' && ['start', 'stop', 'restart'].includes(a[1])) {
    const entry = scenario.gateway && scenario.gateway[a[1]]
    return entry && entry[dry ? 'dryRun' : 'exec']
  }
  if (a[0] === 'web' && a[1] === 'status') return scenario.web && scenario.web.status
  if (a[0] === 'web' && a[1] === 'open') return scenario.web && scenario.web.open
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
  const directory = await mkdtemp(join(tmpdir(), 'tmax-host-'))
  const scenarioPath = join(directory, 'scenario.json')
  const logPath = join(directory, 'calls.jsonl')
  const scriptPath = join(directory, 'tmax')
  const serialize = value => JSON.stringify(value).replaceAll('__HOME__', home)
  await writeFile(scenarioPath, serialize(scenario))
  await writeFile(logPath, '')
  await writeFile(scriptPath, FAKE_SCRIPT.replace('__SCENARIO__', JSON.stringify(scenarioPath)).replace('__LOG__', JSON.stringify(logPath)))
  await chmod(scriptPath, 0o755)
  const api = {
    path: scriptPath,
    async update(next) {
      await writeFile(scenarioPath, serialize(next))
    },
    async calls() {
      const text = await readFile(logPath, 'utf8')
      return text.split('\n').filter(line => line !== '').map(line => JSON.parse(line))
    },
    async clearCalls() {
      await writeFile(logPath, '')
    },
    /** Record further substitutes on top of a live scenario. */
    async patch(mutator) {
      const current = JSON.parse(await readFile(scenarioPath, 'utf8'))
      mutator(current)
      await writeFile(scenarioPath, serialize(current))
    },
  }
  return api
}

// ---------------------------------------------------------------------------
// A fake plugin context exposing the services the host half touches. Optional
// services are provided through `inject`, exactly as the plugin acquires them.

function fakeContext({
  credential = 'test-key',
  parserUnavailable = false,
  resolveCredential,
  configuredRefs = ['TMAX_TEST'],
  rejectCredentialRefs = [],
  liveNames = [],
  agentDefault = { provider: 'tmax', model: 'gpt-6.1-sol' },
  settingsDescribe = () => [],
  configurableProviders = () => [],
  describeThrows = false,
  throwDuplicate = false,
  throwOnReplace = false,
  settingsUnavailable = false,
  deferred = [],
} = {}) {
  const state = {
    routes: [],
    registrations: [],
    replaces: 0,
    tools: [],
    skills: [],
    commands: [],
    events: [],
    effects: [],
    listeners: [],
    adapterOptions: undefined,
    httpRoutes: [],
    credentialRefCalls: [],
    resolveCalls: 0,
    injections: [],
    duplicate: throwDuplicate,
    replaceThrows: throwOnReplace,
  }
  const modules = {
    // DSH's entry-list parser, as a small block-YAML reader: a list of mappings
    // whose nested mappings and sequences follow indentation, which is every
    // shape these fixtures write.
    '@deepseek-ai/cordis-plugin-include': { parseEntryYaml: parseBlocks },
    '@deepseek-ai/dsh-llm-pi-ai': {
      PiAiAdapter: class {
        constructor(options) {
          this.options = options
          state.adapterOptions = options
        }
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
      credentialRef: ref => {
        state.credentialRefCalls.push(ref)
        if (ref === null || ref === undefined) throw new Error('credentialRef received a null reference')
        // A real DSH credentials service throws for a name outside its grammar.
        if (rejectCredentialRefs.includes(ref)) throw new TypeError(`credential ref "${ref}" must match /^[A-Za-z_][A-Za-z0-9_]*$/`)
        return ref
      },
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
  if (parserUnavailable === true) delete modules['@deepseek-ai/cordis-plugin-include']
  const credentialsService = {
    credentialRef(ref) {
      state.credentialRefCalls.push(ref)
      if (ref === null || ref === undefined) throw new Error('credentialRef received a null reference')
      // A real DSH credentials service throws for a name outside its grammar.
      if (rejectCredentialRefs.includes(ref)) throw new TypeError(`credential ref "${ref}" must match /^[A-Za-z_][A-Za-z0-9_]*$/`)
      return ref
    },
    resolve: async () => {
      state.resolveCalls += 1
      if (resolveCredential !== undefined) return resolveCredential()
      return credential === null ? undefined : { value: credential }
    },
    describe: async ref => {
      if (describeThrows) throw new Error('credential store unavailable')
      return { configured: configuredRefs.includes(ref) }
    },
  }
  const services = {
    credentials: credentialsService,
    settings: settingsUnavailable ? undefined : { describe: settingsDescribe },
    agentDefaultModel: { currentSelection: () => agentDefault },
    tools: { register(definition) { state.tools.push(definition); return () => {} } },
    skills: { register(definition) { state.skills.push(definition); return () => {} } },
    commands: { register(definition) { state.commands.push(definition); return () => {} } },
    webServer: {
      register(route) {
        state.httpRoutes.push(route)
        return () => {
          state.httpRoutes = state.httpRoutes.filter(candidate => candidate !== route)
        }
      },
    },
  }
  for (const name of deferred) delete services[name]

  function activate(record) {
    if (record.active) return
    if (!record.deps.every(dep => services[dep] !== undefined)) return
    record.active = true
    record.effects = []
    const child = Object.create(ctx)
    child.effect = callback => {
      const dispose = callback()
      record.effects.push(dispose)
      return dispose
    }
    // Cordis exposes injected services as properties on the child context.
    for (const name of record.deps) {
      Object.defineProperty(child, name, { get: () => services[name], configurable: true })
    }
    record.callback(child)
  }

  const ctx = {
    fiber: { entry: { parent: { tree: { import: async specifier => modules[specifier] } } } },
    logger: { info() {}, warn() {} },
    get(name) {
      return services[name]
    },
    llm: {
      // Real DSH returns {id, name} records, including routes this plugin just
      // registered; a plain string list stays accepted where a test supplies it.
      listProviders: () => [...(state.liveNames ?? liveNames), ...state.routes].map(entry =>
        typeof entry === 'string' ? { id: entry, name: entry } : entry),
      listConfigurableProviders: () => configurableProviders(),
      registerAdapter(routes, adapter) {
        state.registrations.push([...routes])
        if (state.duplicate) {
          const error = new Error('DUPLICATE_ADAPTER: provider route is already registered')
          error.code = 'DUPLICATE_ADAPTER'
          throw error
        }
        state.routes = [...routes]
        state.adapterOptions = adapter.options
        const handle = () => {
          state.routes = []
        }
        handle.replace = next => {
          state.replaces += 1
          if (state.replaceThrows) {
            const error = new Error('DUPLICATE_ADAPTER: provider route is already registered')
            error.code = 'DUPLICATE_ADAPTER'
            throw error
          }
          state.routes = [...next]
        }
        return handle
      },
    },
    on(name, callback) {
      const entry = { name, callback }
      state.listeners.push(entry)
      return () => {
        state.listeners = state.listeners.filter(candidate => candidate !== entry)
      }
    },
    emit(event, payload) { state.events.push([event, payload]) },
    effect(callback) { state.effects.push(callback()) },
    inject(deps, callback) {
      const record = { deps: [...deps], callback, active: false, effects: [] }
      state.injections.push(record)
      activate(record)
      return {
        dispose() {
          record.active = false
          for (const dispose of record.effects) {
            try { dispose?.() } catch {}
          }
          record.effects = []
        },
      }
    },
  }
  /** Provide, replace, or remove one service; injected callbacks re-run. */
  state.provide = (name, value) => {
    if (value === undefined) delete services[name]
    else services[name] = value
    for (const record of state.injections) {
      if (!record.deps.includes(name)) continue
      if (record.active) {
        record.active = false
        for (const dispose of record.effects) {
          try { dispose?.() } catch {}
        }
        record.effects = []
      }
      activate(record)
    }
  }
  state.liveNames = [...liveNames]
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

function makeReq(method, { body, headers = {}, url = '/' } = {}) {
  const chunks = body === undefined ? [] : [Buffer.from(body)]
  return {
    method,
    url,
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

function emptyHome() {
  return mkdtemp(join(tmpdir(), 'tmax-home-'))
}

const defaultConfig = home => ({ tmaxPath: undefined, home, refreshSeconds: 3600 })

async function applied({ scenario = baseScenario(), config = {}, context = {} } = {}) {
  const home = config.home ?? await emptyHome()
  const fake = await fakeTmax(scenario, home)
  const { ctx, state } = fakeContext(context)
  const merged = { ...defaultConfig(home), tmaxPath: fake.path, ...config }
  if (merged.home === undefined) merged.home = home
  await apply(ctx, merged)
  return { fake, ctx, state, home }
}

/** Wait until one asynchronous refresh settles. */
async function settle(ms = 300) {
  await new Promise(resolve => setTimeout(resolve, ms))
}

// ---------------------------------------------------------------------------
// Shape comparison: every object must carry exactly the fixture's keys; every
// non-empty array is compared by its first element. `null` in the fixture is a
// contract wildcard (the value is unknown/unavailable in that case).

function assertShape(actual, expected, path = 'state') {
  if (expected === null || expected === undefined) return
  if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${path} must be an array`)
    if (expected.length === 0) return
    assert.ok(actual.length > 0, `${path} must not be empty`)
    assertShape(actual[0], expected[0], `${path}[0]`)
    return
  }
  if (typeof expected === 'object') {
    assert.ok(actual !== null && typeof actual === 'object' && !Array.isArray(actual), `${path} must be an object`)
    assert.deepEqual(Object.keys(actual).sort(), Object.keys(expected).sort(), `${path} keys`)
    for (const key of Object.keys(expected)) {
      assertShape(actual[key], expected[key], `${path}.${key}`)
    }
    return
  }
  assert.ok(actual === null || typeof actual === typeof expected, `${path} has type ${typeof actual}, expected ${typeof expected}`)
}

// ---------------------------------------------------------------------------
// Tests: the inventory read path and the composed state contract.


test('the inventory composes from agent list/inspect, gateway status, and tmax status', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const body = (await getJson(state, '/_tmax/state')).body
  const calls = await fake.calls()
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['agent', 'list'])), 'the home resolves the target')
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['agent', 'inspect', 'dsh-main'])))
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['gateway', 'status'])))
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['status'])))
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['provider', 'list'])))
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['provider', 'inspect', 'kimi'])))
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['config', 'validate'])))
  assert.ok(calls.some(argv => JSON.stringify(argv) === JSON.stringify(['model', 'explain', 'public-model', '--protocol', 'responses'])))
  assert.equal(calls.some(argv => argv[0] === 'dsh'), false)
  assert.equal(body.diagnostics.readModel, 'inventory')
  assert.equal(body.ok, true)
})

test('an explicit target skips agent list and is the agent inspect operand', async () => {
  const { fake, state } = await applied({ config: { target: 'custom-target' } })
  await fake.clearCalls()
  await getJson(state, '/_tmax/state')
  const calls = await fake.calls()
  assert.equal(calls.some(argv => argv[0] === 'agent' && argv[1] === 'list'), false)
  assert.deepEqual(
    calls.find(argv => argv[0] === 'agent' && argv[1] === 'inspect'),
    ['agent', 'inspect', 'custom-target'],
  )
  assert.equal(calls.some(argv => argv.includes('--home')), false, 'agent inspect has no --home selector')
})

test('no declared target matching the DSH home is a visible typed error', async () => {
  const scenario = baseScenario()
  scenario.agentList = {
    code: 'ok',
    status: 'success',
    data: { agents: [{ target: 'pi-main', agent: 'pi', path: '/tmp/pi' }] },
  }
  const { state } = await applied({ scenario })
  assert.deepEqual(state.routes, [])
  assert.equal(state.registrations.length, 0)
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ok, true, 'providers and configuration still answered')
  assert.equal(body.target, null)
  assert.deepEqual(body.routes, [])
  assert.equal(body.errors.length, 1)
  assert.equal(body.errors[0].section, 'projection')
  assert.equal(body.errors[0].code, 'unknown_resource')
  assert.match(body.errors[0].message, /No declared DSH target matches the DSH home/)
  assert.equal(body.providers.tmax.length, 3, 'readable sections are not erased by the failed target read')
  assert.equal(body.catalog.state, 'available', 'provider and model reads still compose a catalog')
  assert.equal(body.catalog.entriesState, 'unknown', 'the failed target read owns only the entry side')
  assert.equal(body.diagnostics.readModel, 'inventory')
  const status = (await getJson(state, '/_tmax/status')).body
  assert.ok(status.blockers.some(blocker => blocker.source === 'projection' && blocker.code === 'unknown_resource'))
})

test('target resolution compares the DSH home across spellings and refuses another dsh path', async () => {
  // A trailing slash on the declared target path is the same directory.
  const home = await emptyHome()
  const slashScenario = baseScenario()
  slashScenario.agentList = {
    code: 'ok',
    status: 'success',
    data: { agents: [{ target: 'dsh-slash', agent: 'dsh', path: `${home}/` }] },
  }
  slashScenario.agentInspect.data.target = 'dsh-slash'
  const slash = await applied({ scenario: slashScenario, config: { home } })
  const slashBody = (await getJson(slash.state, '/_tmax/state')).body
  assert.deepEqual(slashBody.errors, [])
  assert.equal(slashBody.target.id, 'dsh-slash')

  // A symlinked DSH home resolves to the declared real path.
  const real = await mkdtemp(join(tmpdir(), 'tmax-real-home-'))
  const link = `${real}-link`
  await symlink(real, link, 'dir')
  const symScenario = baseScenario()
  symScenario.agentList = {
    code: 'ok',
    status: 'success',
    data: { agents: [{ target: 'dsh-real', agent: 'dsh', path: real }] },
  }
  symScenario.agentInspect.data.target = 'dsh-real'
  const symlinked = await applied({ scenario: symScenario, config: { home: link } })
  const symBody = (await getJson(symlinked.state, '/_tmax/state')).body
  assert.deepEqual(symBody.errors, [])
  assert.equal(symBody.target.id, 'dsh-real')

  // One dsh target with a different path is not this home.
  const other = baseScenario()
  other.agentList = {
    code: 'ok',
    status: 'success',
    data: { agents: [{ target: 'dsh-other', agent: 'dsh', path: '/tmp/some-other-dsh-home' }] },
  }
  const refused = await applied({ scenario: other })
  assert.deepEqual(refused.state.routes, [])
  const refusedBody = (await getJson(refused.state, '/_tmax/state')).body
  assert.equal(refusedBody.target, null)
  assert.deepEqual(refusedBody.errors.map(error => error.code), ['unknown_resource'])
  assert.match(refusedBody.errors[0].message, /No declared DSH target matches/)
})

test('multiple targets matching the DSH home are an invalid-arguments error', async () => {
  const scenario = baseScenario()
  scenario.agentList = {
    code: 'ok',
    status: 'success',
    data: {
      agents: [
        { target: 'dsh-a', agent: 'dsh', path: '__HOME__' },
        { target: 'dsh-b', agent: 'dsh', path: '__HOME__' },
      ],
    },
  }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.errors[0].section, 'projection')
  assert.equal(body.errors[0].code, 'invalid_arguments')
  assert.match(body.errors[0].message, /Multiple DSH targets match/)
})

test('a selected target that is not a DSH target is refused', async () => {
  const scenario = baseScenario()
  scenario.agentInspect = agentInspect({ agent: 'pi' })
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.errors[0].section, 'projection')
  assert.equal(body.errors[0].code, 'invalid_arguments')
  assert.match(body.errors[0].message, /not a DSH target/)
})

test('an agent inspect failure is a visible projection error, not a crash', async () => {
  const scenario = baseScenario()
  scenario.agentInspect = {
    code: 'unknown_resource',
    status: 'error',
    data: null,
    blockers: [{ code: 'unknown_resource', message: 'Unknown DSH target "dsh-main".' }],
    __exit: 2,
  }
  const { state } = await applied({ scenario })
  assert.deepEqual(state.routes, [])
  assert.equal(state.registrations.length, 0)
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ok, true, 'the provider and config reads still answered')
  assert.equal(body.target, null)
  assert.deepEqual(body.errors, [{
    section: 'projection',
    code: 'unknown_resource',
    message: 'tmax agent inspect failed: unknown_resource (Unknown DSH target "dsh-main".)',
  }])
  assert.match(body.diagnostics.lastError, /Unknown DSH target/)
  const status = (await getJson(state, '/_tmax/status')).body
  const projectionBlockers = status.blockers.filter(blocker => blocker.source === 'projection')
  assert.equal(projectionBlockers.length, 1)
  assert.equal(projectionBlockers[0].code, 'unknown_resource')
  assert.equal(status.blockers.some(blocker => blocker.code === 'tmax_unavailable'), false)
})

test('an older tmax without relayed/capability inspection is an unsupported_tmax error', async () => {
  const scenario = baseScenario()
  delete scenario.agentInspect.data.providers[0].relayed
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.errors[0].section, 'projection')
  assert.equal(body.errors[0].code, 'unsupported_tmax')
  assert.match(body.errors[0].message, /update tmax/)
})

test('a missing tmax CLI does not fail activation and is reported as a blocker', async () => {
  const home = await emptyHome()
  const { ctx, state } = fakeContext()
  await apply(ctx, { tmaxPath: '/definitely/missing/tmax', home, refreshSeconds: 3600 })
  assert.deepEqual(state.routes, [])
  assert.equal(state.registrations.length, 0)
  const status = await getJson(state, '/_tmax/status')
  assert.equal(status.statusCode, 200)
  assert.equal(status.body.ok, false)
  assert.match(status.body.lastError, /ENOENT|could not be executed/)
  // The configured executable is a path, not evidence: it never reaches an
  // HTTP body or the command output.
  assert.equal(status.body.lastError.includes('/definitely/missing/tmax'), false)
  assert.equal(status.body.lastError.includes('Command failed'), false)
  assert.ok(status.body.blockers.some(blocker => blocker.source === 'projection' && blocker.code === 'tmax_unavailable'))
  const stateBody = (await getJson(state, '/_tmax/state')).body
  // Nothing answered: the target, provider, and configuration reads each name
  // their own failure instead of one generic claim.
  assert.deepEqual([...new Set(stateBody.errors.map(error => error.section))].sort(), ['catalog', 'config', 'projection', 'providers'])
  assert.ok(stateBody.errors.every(error => typeof error.code === 'string' && error.code !== '' && typeof error.message === 'string'))
  assert.equal(JSON.stringify(stateBody).includes('/definitely/missing/tmax'), false, 'no section error echoes the executable path')
  assert.equal(stateBody.diagnostics.readModel, 'inventory')
  const tool = state.tools.find(candidate => candidate.name === 'tmax_status')
  const value = await tool.execute()
  assert.equal(value.ok, false)
  assert.deepEqual(value.routes, [])
  assert.ok(value.errors.length > 0)
})

test('a gateway status failure leaves gateway facts unknown without erasing routes', async () => {
  const scenario = baseScenario()
  scenario.gatewayStatus = {
    code: 'daemon_incompatible',
    status: 'error',
    data: null,
    blockers: [{ code: 'daemon_incompatible', message: 'The daemon management protocol is incompatible.' }],
    __exit: 4,
  }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ok, true, 'the inventory still describes the target')
  assert.deepEqual(body.routes.map(route => route.name), ['tmax'])
  assert.equal(body.gateway.ready, null)
  assert.equal(body.gateway.daemonDiscoveryState, 'unknown')
  assert.equal(body.gateway.reasonCode, 'daemon_identity_unavailable')
  assert.equal(body.gateway.listen, '127.0.0.1:17840')
  assert.ok(body.errors.some(error => error.section === 'gateway' && error.code === 'daemon_incompatible'))
  assert.deepEqual(body.diagnostics.history, { state: 'unavailable', reasonCode: 'daemon_identity_unavailable' })
})

test('a tmax status failure keeps authority unknown and names it, never inventing a path', async () => {
  const scenario = baseScenario()
  scenario.tmaxStatus = {
    code: 'configuration_invalid',
    status: 'error',
    data: null,
    message: 'Configuration format version 1 is unsupported',
    __exit: 4,
  }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.authority, null)
  assert.equal(body.tmaxVersion, '0.1.0', 'the gateway status still names the answering version')
  assert.ok(body.errors.some(error => error.section === 'authority' && error.code === 'configuration_invalid'))
})

test('an unavailable target sub-read stays visible in the target section', async () => {
  const scenario = baseScenario()
  scenario.agentInspect.data.native_default = { state: 'unavailable', reason_code: 'dsh_target_inspect_failed' }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.deepEqual(body.target.nativeDefault, { state: 'unavailable', reason_code: 'dsh_target_inspect_failed' })
  assert.equal(body.target.id, 'dsh-main')
})

test('gateway daemon and shared-key tokens use the page vocabulary', async () => {
  assert.equal(normalizeDaemonDiscovery('compatible'), 'identified')
  assert.equal(normalizeDaemonDiscovery('identified'), 'identified')
  assert.equal(normalizeDaemonDiscovery('unidentified'), 'unidentified')
  assert.equal(normalizeDaemonDiscovery('absent'), 'unidentified')
  assert.equal(normalizeDaemonDiscovery('incompatible'), 'unknown')
  assert.equal(normalizeDaemonDiscovery(undefined), 'unknown')
  assert.equal(normalizeSharedKeyState('ready'), 'loaded')
  assert.equal(normalizeSharedKeyState('loaded'), 'loaded')
  assert.equal(normalizeSharedKeyState('absent'), 'absent')
  assert.equal(normalizeSharedKeyState(undefined), 'unknown')
  assert.equal(normalizeSharedKeyState('rotating'), 'unknown')

  const scenario = baseScenario()
  scenario.gatewayStatus.data.daemon.discovery_state = 'compatible'
  scenario.gatewayStatus.data.daemon.reason_code = null
  scenario.gatewayStatus.data.gateway = { ready: true, listen: '127.0.0.1:17840', key_ref: 'k1', config_revision: 'rev-1' }
  scenario.gatewayStatus.data.shared_key = { loaded_reference: 'k1', loaded_state: 'ready' }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.gateway.daemonDiscoveryState, 'identified')
  assert.equal(body.gateway.sharedKey.loadedState, 'loaded')
  assert.equal(body.gateway.ready, true)
  assert.equal(body.gateway.reasonCode, null)
  assert.deepEqual(body.diagnostics.history, { state: 'available', reasonCode: null })
})

test('a model whose native reasoning is disabled reports a known false', async () => {
  const scenario = baseScenario()
  scenario.agentInspect.data.native_projection[0].models[0].capabilities.reasoningEfforts = false
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.routes[0].models[0].id, 'public-model')
  assert.equal(body.routes[0].models[0].capabilities.reasoningEfforts, false)
})

test('a configured gateway with no running daemon is not ready, not unknown', async () => {
  const scenario = baseScenario()
  scenario.gatewayStatus.data.daemon.discovery_state = 'absent'
  scenario.gatewayStatus.data.daemon.reason_code = null
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.gateway.ready, false)
  assert.equal(body.gateway.reasonCode, 'gateway_not_ready')
})

test('an unconfigured gateway reports its blocker and no invented routes', async () => {
  const scenario = baseScenario()
  scenario.agentInspect.data.providers = []
  scenario.agentInspect.data.native_projection = []
  scenario.agentInspect.data.default = null
  scenario.agentInspect.data.blockers = [{ code: 'target_incomplete', message: 'Select providers and models before applying' }]
  const { state } = await applied({ scenario })
  assert.deepEqual(state.routes, [])
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.gateway.configured, false)
  assert.equal(body.gateway.ready, null)
  assert.equal(body.gateway.reasonCode, 'gateway_unconfigured')
  assert.deepEqual(body.routes, [])
  assert.deepEqual(
    body.target.blockers.map(blocker => blocker.code).sort(),
    ['gateway_unconfigured', 'target_incomplete'],
  )
  const status = (await getJson(state, '/_tmax/status')).body
  assert.ok(status.blockers.some(blocker => blocker.source === 'target' && blocker.code === 'gateway_unconfigured'))
})

test('routes without a credential reference or base URL are skipped with declared blockers', async () => {
  const scenario = withSecondRoute(baseScenario(), { name: 'no-cred', credentialRef: null })
  withSecondRoute(scenario, { name: 'no-base', credentialRef: 'TMAX_OTHER' })
  scenario.agentInspect.data.providers.find(entry => entry.name === 'no-base').base_url = null
  // Keep the base route fully servable so the declared blockers below are the
  // only route-build blockers in play.
  scenario.agentInspect.data.providers[0].models = scenario.agentInspect.data.providers[0].models
    .filter(model => model.id !== 'no-context')
  scenario.agentInspect.data.native_projection[0].models =
    scenario.agentInspect.data.native_projection[0].models.filter(model => model.id !== 'no-context')
  const { state } = await applied({ scenario })

  // Every declared route is listed; the two unusable ones carry blockers below.
  assert.deepEqual((await getJson(state, '/_tmax/state')).body.routes.map(route => route.name), ['tmax', 'no-cred', 'no-base'])
  assert.equal(state.credentialRefCalls.includes(null), false)
  assert.equal(state.credentialRefCalls.includes(undefined), false)
  assert.deepEqual(state.credentialRefCalls, ['TMAX_TEST'])

  const status = (await getJson(state, '/_tmax/status')).body
  const declared = status.blockers.filter(blocker => blocker.source === 'route')
  assert.deepEqual(declared.map(blocker => blocker.code), ['route_credential_missing', 'route_base_url_missing'])
  assert.deepEqual(declared[0].models, ['no-cred-model'])
  assert.deepEqual(declared[1].models, ['no-base-model'])
  assert.match(declared[0].message, /tmax agent apply dsh-main/)

  const body = (await getJson(state, '/_tmax/state')).body
  assert.deepEqual(body.routes.map(route => route.name), ['tmax', 'no-cred', 'no-base'])
  assert.equal(body.routes[1].credentialRef, null)
  assert.equal(body.routes[1].credentialConfigured, null)
  assert.deepEqual(body.routes[1].models.map(model => model.id), ['no-cred-model'])
})

test('route credential readiness distinguishes configured, missing, and unknown', async () => {
  const configured = await applied()
  const configuredBody = (await getJson(configured.state, '/_tmax/state')).body
  assert.equal(configuredBody.routes[0].credentialConfigured, true)

  // A readable store that reports the reference missing is a known false; the
  // route is still registered, it is just not usable.
  const missing = await applied({ context: { configuredRefs: [] } })
  assert.deepEqual((await getJson(missing.state, '/_tmax/state')).body.routes.map(route => route.name), ['tmax'])
  const missingBody = (await getJson(missing.state, '/_tmax/state')).body
  assert.equal(missingBody.routes[0].credentialConfigured, false)

  // A store that cannot answer is unknown, never missing.
  const unknown = await applied({ context: { describeThrows: true } })
  const unknownBody = (await getJson(unknown.state, '/_tmax/state')).body
  assert.equal(unknownBody.routes[0].credentialConfigured, null)
})

test('a malformed credential reference skips only that route with a declared blocker', async () => {
  const scenario = withSecondRoute(baseScenario(), { name: 'bad-ref', credentialRef: 'bad-ref' })
  const { state } = await applied({ scenario, context: { rejectCredentialRefs: ['bad-ref'] } })

  // One bad reference never drops the working route.
  assert.deepEqual((await getJson(state, '/_tmax/state')).body.routes.map(route => route.name), ['tmax', 'bad-ref'])
  const status = (await getJson(state, '/_tmax/status')).body
  const invalid = status.blockers.filter(blocker => blocker.code === 'route_credential_invalid')
  assert.equal(invalid.length, 1)
  assert.equal(invalid[0].source, 'route')
  assert.match(invalid[0].message, /bad-ref/)
  assert.deepEqual(invalid[0].models, ['bad-ref-model'])

  const body = (await getJson(state, '/_tmax/state')).body
  const bad = body.routes.find(route => route.name === 'bad-ref')
  assert.ok(bad)
  assert.deepEqual(bad.blockers.map(blocker => blocker.code), ['route_credential_invalid'])
})

test('state routes carry liveness and the declared route blockers', async () => {
  const scenario = withSecondRoute(baseScenario(), { name: 'partial', contextWindow: 1000 })
  // A second model with no context capacity on the same route is hidden.
  scenario.agentInspect.data.providers.find(entry => entry.name === 'partial').models.push({ id: 'hidden', protocol: 'responses' })
  scenario.agentInspect.data.native_projection.find(entry => entry.name === 'partial').models.push({
    id: 'hidden',
    protocol: 'responses',
    capabilities: {},
    efforts: [],
    projection: { fields: {} },
  })
  scenario.explain.hidden = explain('public-model')
  const { state } = await applied({ scenario, context: { liveNames: ['tmax', 'partial'] } })
  assert.deepEqual((await getJson(state, '/_tmax/state')).body.routes.map(route => route.name), ['tmax', 'partial'], 'every declared route is listed')

  const body = (await getJson(state, '/_tmax/state')).body
  const partial = body.routes.find(route => route.name === 'partial')
  assert.equal(partial.active, true)
  assert.deepEqual(partial.blockers.map(blocker => blocker.code), ['route_model_context_window_unknown'])
  assert.deepEqual(partial.blockers[0].models, ['hidden'])

  const status = (await getJson(state, '/_tmax/status')).body
  const routeCodes = status.blockers.filter(blocker => blocker.source === 'route').map(blocker => blocker.code)
  assert.deepEqual(routeCodes, ['route_model_context_window_unknown', 'route_model_context_window_unknown'])
})

test('a provider inspect failure stays a per-provider error record', async () => {
  const scenario = baseScenario()
  delete scenario.inspect['cpa-us2']
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ok, true)
  assert.deepEqual(body.providers.tmax.map(provider => provider.id), ['kimi', 'cpa-us2', 'tmax'])
  assert.equal(body.providers.tmax[0].error, null)
  assert.equal(body.providers.tmax[1].error.code, 'unknown_resource')
  assert.equal(typeof body.providers.tmax[1].error.message, 'string')
  assert.deepEqual(body.errors, [], 'a provider row error is not a section failure')
  const status = (await getJson(state, '/_tmax/status')).body
  assert.ok(status.blockers.some(blocker => blocker.source === 'provider' && blocker.code === 'unknown_resource'))
})

test('a provider-list failure is a visible section error, not zero providers', async () => {
  const scenario = baseScenario()
  scenario.providerList = {
    code: 'storage_unavailable',
    status: 'error',
    data: null,
    blockers: [{ code: 'storage_unavailable', message: 'storage unavailable' }],
    __exit: 4,
  }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ok, true, 'the inventory itself was readable')
  assert.deepEqual(body.providers.tmax, [])
  const providersError = body.errors.find(error => error.section === 'providers')
  assert.equal(providersError.code, 'storage_unavailable')
  assert.equal(body.routes.length, 1)
})

test('an invalid config is reported without making the whole state unreadable', async () => {
  const scenario = baseScenario()
  scenario.configValidate = {
    code: 'invalid_config',
    status: 'error',
    data: { valid: false, revision: 'rev-bad' },
    blockers: [{ code: 'invalid_config', message: 'Unknown field "bogus" in the saved configuration' }],
    __exit: 4,
  }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ok, true)
  assert.equal(body.config.state, 'invalid')
  assert.equal(body.config.revision, 'rev-bad')
  assert.deepEqual(body.config.errors, [{
    code: 'invalid_config',
    message: 'Unknown field "bogus" in the saved configuration',
  }])
  assert.equal(body.diagnostics.lastError, null)
  assert.ok(body.errors.some(error => error.section === 'config' && error.code === 'invalid_config'))
  const status = (await getJson(state, '/_tmax/status')).body
  assert.ok(status.blockers.some(blocker => blocker.source === 'config' && blocker.code === 'invalid_config'))
  assert.ok(status.blockers.some(blocker => blocker.source === 'section' && blocker.code === 'invalid_config'))
})

test('an absent saved configuration is valid evidence, not unavailable', async () => {
  const scenario = baseScenario()
  scenario.configValidate = { code: 'ok', status: 'success', data: { valid: true, revision: null, snapshot_state: 'absent' } }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.deepEqual(body.config, { state: 'valid', revision: null, errors: [] })
  assert.equal(body.diagnostics.lastError, null)
})

test('routing candidates keep tmax evaluation order behind the selected row', async () => {
  const scenario = baseScenario()
  scenario.agentInspect.data.providers[0].models = [{ id: 'order-model', protocol: 'responses' }]
  scenario.agentInspect.data.native_projection[0].models = [{
    id: 'order-model',
    protocol: 'responses',
    capabilities: { contextWindow: 1000 },
    efforts: [],
    projection: { fields: {} },
  }]
  scenario.explain = {
    'order-model': {
      code: 'ok',
      status: 'success',
      data: {
        selection: { qualified_model: 'b/order-model', provider: 'b', model: 'order-model', priority: 5 },
        priority_evaluations: [
          { qualified_model: 'a/order-model', priority_evaluation: { effective_priority: 0 } },
          { qualified_model: 'b/order-model', priority_evaluation: { effective_priority: 5 } },
          { qualified_model: 'c/order-model', priority_evaluation: { effective_priority: 1 } },
        ],
        excluded_candidates: [],
      },
    },
  }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.deepEqual(
    body.routing[0].candidates.map(candidate => candidate.qualifiedModel),
    ['b/order-model', 'a/order-model', 'c/order-model'],
    'selected leads, the rest keep evaluation order rather than priority order',
  )
})

test('a model explain failure stays a per-row routing error', async () => {
  const scenario = baseScenario()
  delete scenario.explain['public-model']
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  const row = body.routing.find(entry => entry.model === 'public-model')
  assert.equal(row.error.code, 'unknown_resource')
  assert.equal(row.selection, null)
  assert.deepEqual(body.errors, [], 'a routing row error is not a section failure')
})

test('the state cache serves repeated reads and single-flights cold ones', async () => {
  const warm = await applied()
  await warm.fake.clearCalls()
  const first = await getJson(warm.state, '/_tmax/state')
  const inspectsAfterFirst = (await warm.fake.calls()).filter(argv => argv[0] === 'agent' && argv[1] === 'inspect').length
  const second = await getJson(warm.state, '/_tmax/state')
  assert.deepEqual(second.body, first.body)
  assert.equal(
    (await warm.fake.calls()).filter(argv => argv[0] === 'agent' && argv[1] === 'inspect').length,
    inspectsAfterFirst,
    'a warm read spawns no new composition',
  )

  // Two cold readers share one composition (single-flight).
  const cold = await applied()
  await cold.fake.clearCalls()
  await cold.fake.update({ ...baseScenario(), delayFor: 'agent inspect', delayMs: 120 })
  const [left, right] = await Promise.all([
    callRoute(routeOf(cold.state, '/_tmax/state'), 'GET', {}),
    callRoute(routeOf(cold.state, '/_tmax/state'), 'GET', {}),
  ])
  assert.deepEqual(left.body, right.body)
  const inspects = (await cold.fake.calls()).filter(argv => argv[0] === 'agent' && argv[1] === 'inspect').length
  assert.equal(inspects, 1, 'one cold composition serves both readers')
  await cold.fake.update(baseScenario())
})







// ---------------------------------------------------------------------------
// Tests: optional services acquired by injection (S4).


// ---------------------------------------------------------------------------
// Tests: HTTP route surface, POST hardening, and explicit operations.

test('POST actions are hardened: method, media type, origin, size, JSON, and busy', async () => {
  const { state } = await applied()
  const route = routeOf(state, '/_tmax/observe')

  const wrongMethod = await callRoute(route, 'GET', {})
  assert.equal(wrongMethod.statusCode, 405)
  assert.equal(wrongMethod.headers.allow, 'POST')

  const noType = await callRoute(route, 'POST', { body: '{}' })
  assert.equal(noType.statusCode, 415)

  const wrongType = await callRoute(route, 'POST', { body: '{}', headers: { 'content-type': 'text/plain' } })
  assert.equal(wrongType.statusCode, 415)

  const jsonWithCharset = await callRoute(route, 'POST', { body: '{}', headers: { 'content-type': 'application/json; charset=utf-8' } })
  assert.equal(jsonWithCharset.statusCode, 200)

  const crossOrigin = await postJson(state, '/_tmax/observe', {}, { origin: 'http://evil.test' })
  assert.equal(crossOrigin.statusCode, 403)

  const crossSite = await postJson(state, '/_tmax/observe', {}, { 'sec-fetch-site': 'cross-site' })
  assert.equal(crossSite.statusCode, 403)

  const sameOrigin = await postJson(state, '/_tmax/observe', {}, { origin: 'http://localhost:3080' })
  assert.equal(sameOrigin.statusCode, 200)

  const tooLarge = await callRoute(route, 'POST', { body: '{}', headers: { 'content-type': 'application/json', 'content-length': String(20 * 1024) } })
  assert.equal(tooLarge.statusCode, 413)

  const notJson = await callRoute(route, 'POST', { body: '{not json', headers: { 'content-type': 'application/json' } })
  assert.equal(notJson.statusCode, 400)

  const invalidDryRun = await postJson(state, '/_tmax/observe', { dryRun: 'yes' })
  assert.equal(invalidDryRun.statusCode, 400)
  assert.equal(invalidDryRun.body.code, 'invalid_dry_run')
})

test('concurrent observe requests are rejected with busy', async () => {
  const { fake, state } = await applied()
  await fake.update({ ...baseScenario(), delayFor: 'provider refresh', delayMs: 150 })
  const first = postJson(state, '/_tmax/observe', {})
  await new Promise(resolve => setTimeout(resolve, 40))
  const second = await postJson(state, '/_tmax/observe', {})
  assert.equal(second.statusCode, 409)
  assert.equal(second.body.code, 'busy')
  const firstResult = await first
  assert.ok(firstResult.statusCode === 200 || firstResult.statusCode === 207)
  await fake.update(baseScenario())
})

test('a provider with no quota source is never refreshed', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const result = await postJson(state, '/_tmax/observe', {})
  assert.equal(result.statusCode, 200)
  assert.equal(result.body.ok, true)
  // The list reports cpa-us2 with billing kind unknown: no endpoint exists, so
  // the observation does not attempt it and reports no item for it.
  assert.deepEqual(result.body.items.map(item => item.id), ['kimi'])
  const refreshed = (await fake.calls())
    .filter(argv => argv[0] === 'provider' && argv[1] === 'refresh')
    .map(argv => argv[2])
  assert.deepEqual(refreshed, ['kimi'], 'no source means no request')
})

test('a dry-run observation previews with --dry-run and publishes nothing', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/observe', { dryRun: true })
  assert.equal(preview.statusCode, 200)
  assert.equal(preview.body.ok, true)
  assert.equal(preview.body.dryRun, true)
  // One quota-scope refresh per provider with a quota source, every one a preview.
  const refreshes = (await fake.calls()).filter(argv => argv[0] === 'provider' && argv[1] === 'refresh')
  assert.equal(refreshes.length, 1)
  assert.ok(refreshes[0].includes('kimi'))
  for (const argv of refreshes) {
    assert.ok(argv.includes('--dry-run'), `dry run passes --dry-run: ${argv.join(' ')}`)
    assert.ok(argv.includes('--scope') && argv.includes('quota'), `one quota observation: ${argv.join(' ')}`)
  }
  assert.deepEqual(preview.body.items.map(item => item.status), ['unchanged'])
  assert.ok(preview.body.items.every(item => item.code === 'dry_run'))
  // The preview still returns a fresh read model, under the current schema.
  assert.equal(preview.body.state.schemaVersion, 3)
})

test('explicit observe reports all-failed and partial outcomes without claiming success', async () => {
  const scenario = baseScenario()
  // A second observable provider, so one item can fail while another completes.
  scenario.providerList.data.providers.find(provider => provider.id === 'cpa-us2')
    .observations.billing_kind = 'plan_allowance'
  scenario.refresh = {
    kimi: { exec: refreshFailed('kimi'), dryRun: refreshDryRun('kimi') },
    'cpa-us2': { exec: refreshFailed('cpa-us2'), dryRun: refreshDryRun('cpa-us2') },
  }
  const { fake, state } = await applied({ scenario })
  await fake.clearCalls()
  const failed = await postJson(state, '/_tmax/observe', {})
  assert.equal(failed.statusCode, 207)
  assert.equal(failed.body.ok, false)
  assert.deepEqual(failed.body.items.map(item => item.status), ['failed', 'failed'])
  assert.ok(failed.body.items.every(item => item.code === 'observation_failed'))
  assert.ok(failed.body.state, 'state accompanies the outcome')
  assert.equal(failed.body.dryRun, false)

  // Partial: one completed, one blocked-by-configuration is not a transport failure.
  await fake.patch(current => {
    current.refresh.kimi.exec = refreshOk('kimi')
    current.refresh['cpa-us2'].exec = refreshUnsupported('cpa-us2')
  })
  const partial = await postJson(state, '/_tmax/observe', {})
  assert.equal(partial.statusCode, 200)
  assert.equal(partial.body.ok, true)
  assert.deepEqual(partial.body.items.map(item => item.status), ['refreshed', 'blocked'])
  assert.equal(partial.body.items[1].code, 'unsupported_source')

  // Mixed with a real failure: not success.
  await fake.patch(current => {
    current.refresh['cpa-us2'].exec = refreshFailed('cpa-us2')
  })
  const mixed = await postJson(state, '/_tmax/observe', {})
  assert.equal(mixed.statusCode, 207)
  assert.equal(mixed.body.ok, false)
  assert.deepEqual(mixed.body.items.map(item => item.status), ['refreshed', 'failed'])
})

test('the legacy usage refresh route observes explicitly and reports per-item results', async () => {
  const { state } = await applied()
  const body = await postJson(state, '/_tmax/usage/refresh', {})
  assert.equal(body.statusCode, 200)
  assert.equal(body.body.ok, true)
  assert.equal(body.body.dryRun, false)
  assert.equal(body.body.providers.length, 3)
  assert.deepEqual(body.body.items.map(item => item.status), ['refreshed'])
})

// ---------------------------------------------------------------------------
// Tests: S1 — the usage refresh route honors preview requests.

test('POST /_tmax/usage/refresh honors dryRun/preview and never observes on preview', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()

  const preview = await postJson(state, '/_tmax/usage/refresh', { dryRun: true })
  assert.equal(preview.statusCode, 200)
  assert.equal(preview.body.ok, true)
  assert.equal(preview.body.dryRun, true)
  assert.deepEqual(preview.body.items.map(item => item.status), ['unchanged'])
  let refreshes = (await fake.calls()).filter(argv => argv[0] === 'provider' && argv[1] === 'refresh')
  assert.equal(refreshes.length, 1)
  assert.ok(refreshes.every(argv => argv.includes('--dry-run')), 'a preview passes --dry-run to every refresh')

  // The body `preview` alias and the query form are the same request.
  await fake.clearCalls()
  const aliased = await postJson(state, '/_tmax/usage/refresh', { preview: true })
  assert.equal(aliased.statusCode, 200)
  assert.equal(aliased.body.dryRun, true)
  await fake.clearCalls()
  const query = await callRoute(routeOf(state, '/_tmax/usage/refresh'), 'POST', {
    body: '{}',
    headers: { 'content-type': 'application/json' },
    url: '/_tmax/usage/refresh?dry-run=1',
  })
  assert.equal(query.statusCode, 200)
  assert.equal(query.body.dryRun, true)
  refreshes = (await fake.calls()).filter(argv => argv[0] === 'provider' && argv[1] === 'refresh')
  assert.ok(refreshes.every(argv => argv.includes('--dry-run')), 'the query preview never observes for real')

  // A real request still observes.
  await fake.clearCalls()
  const executed = await postJson(state, '/_tmax/usage/refresh', { dryRun: false })
  assert.equal(executed.statusCode, 200)
  assert.equal(executed.body.dryRun, false)
  assert.deepEqual(executed.body.items.map(item => item.status), ['refreshed'])
  refreshes = (await fake.calls()).filter(argv => argv[0] === 'provider' && argv[1] === 'refresh')
  assert.ok(refreshes.every(argv => argv.includes('--dry-run') === false), 'a real refresh never previews')
})

// ---------------------------------------------------------------------------
// Tests: Connect through `tmax agent apply TARGET [--dry-run]`.

test('connect previews with --dry-run, executes without it, and returns state without key material', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const preview = await postJson(state, '/_tmax/connect', { dryRun: true })
  assert.equal(preview.statusCode, 200)
  assert.equal(preview.body.ok, true)
  assert.equal(preview.body.dryRun, true)
  assert.deepEqual(preview.body.connect.effects, { native_writes: false, secret_reads: false, processes_started: false, locks_created: false })
  assert.equal(preview.body.connect.reference, 'TMAX_TEST', 'a credential reference name is not a secret')
  for (const key of ['key', 'token']) assert.equal(key in preview.body.connect, false, `${key} is stripped from a preview`)
  assert.equal('password' in preview.body.connect.nested, false, 'nested secret material is stripped')
  assert.equal(JSON.stringify(preview.body).includes('sk-live-CONNECT-SECRET'), false, 'no key material in a preview response')
  assert.ok(preview.body.state)
  let calls = await fake.calls()
  assert.deepEqual(
    calls.find(argv => argv[0] === 'agent' && argv[1] === 'apply'),
    ['agent', 'apply', 'dsh-main', '--dry-run'],
  )
  assert.equal(calls.some(argv => argv[0] === 'dsh'), false)

  await fake.clearCalls()
  const executed = await postJson(state, '/_tmax/connect', { dryRun: false })
  assert.equal(executed.statusCode, 200, 'a completed apply may report awaiting_native_reload')
  assert.equal(executed.body.ok, true)
  assert.equal(executed.body.connect.state, 'applied_reload_pending')
  assert.equal(executed.body.connect.written, true)
  assert.deepEqual(executed.body.connect.changed_files, ['/tmp/dsh/cordis.patch.yml'])
  assert.deepEqual(executed.body.connect.effects, undefined, 'a real apply reports files, not preview effects')
  calls = await fake.calls()
  assert.deepEqual(
    calls.find(argv => argv[0] === 'agent' && argv[1] === 'apply'),
    ['agent', 'apply', 'dsh-main'],
  )
  assert.equal(executed.body.connect.reference, 'TMAX_TEST')
  for (const key of ['key', 'token']) assert.equal(key in executed.body.connect, false, `${key} is stripped from an applied result`)
  assert.equal('password' in executed.body.connect.nested, false)
  assert.equal(JSON.stringify(executed.body).includes('sk-live-CONNECT-SECRET'), false, 'no key material in an applied response')

  // A refused connect is a 409 with the tmax blocker, still state-bearing.
  await fake.patch(current => {
    current.connect.exec = {
      code: 'target_incomplete',
      status: 'blocked',
      data: null,
      blockers: [{
        code: 'target_incomplete',
        message: 'This DSH target has no gateway route to connect',
        key: 'sk-live-CONNECT-SECRET',
        token: { value: 'sk-live-CONNECT-SECRET' },
      }],
      __exit: 3,
    }
  })
  const refused = await postJson(state, '/_tmax/connect', { dryRun: false })
  assert.equal(refused.statusCode, 409)
  assert.equal(refused.body.ok, false)
  assert.equal(refused.body.code, 'target_incomplete')
  assert.equal(refused.body.message, 'This DSH target has no gateway route to connect')
  assert.equal(refused.body.connect.blockers[0].code, 'target_incomplete')
  assert.equal('key' in refused.body.connect.blockers[0], false, 'a refusal strips key material')
  assert.equal('token' in refused.body.connect.blockers[0], false)
  assert.equal(JSON.stringify(refused.body).includes('sk-live-CONNECT-SECRET'), false, 'no key material in a refusal')
  assert.ok(refused.body.state)
  await fake.update(baseScenario())
})

test('an unresolvable connect target is a typed 409 refusal, not a 502', async () => {
  const scenario = baseScenario()
  scenario.agentList = { code: 'ok', status: 'success', data: { agents: [] } }
  const { state } = await applied({ scenario })
  const result = await postJson(state, '/_tmax/connect', { dryRun: true })
  assert.equal(result.statusCode, 409)
  assert.equal(result.body.ok, false)
  assert.equal(result.body.code, 'unknown_resource')
  assert.match(result.body.message, /No declared DSH target matches/)
})

test('a connect transport failure is a 502, not a typed refusal', async () => {
  const { fake, state } = await applied()
  await fake.patch(current => {
    current.connect.exec = { __raw: 'not a tmax envelope', __exit: 4 }
  })
  const result = await postJson(state, '/_tmax/connect', { dryRun: false })
  assert.equal(result.statusCode, 502)
  assert.equal(result.body.ok, false)
  assert.equal(result.body.code, 'connect_unavailable')
  await fake.update(baseScenario())
})

test('connect preview aliases can never execute a native write and execution needs an explicit false', async () => {
  const { fake, state } = await applied()
  const route = routeOf(state, '/_tmax/connect')
  const applyCalls = async () => (await fake.calls()).filter(argv => argv[0] === 'agent' && argv[1] === 'apply')
  const post = async (payload, url = '/_tmax/connect') => callRoute(route, 'POST', {
    body: JSON.stringify(payload),
    url,
    headers: { 'content-type': 'application/json' },
  })

  // The documented first-generation preview body form.
  await fake.clearCalls()
  const bodyPreview = await post({ preview: true })
  assert.equal(bodyPreview.statusCode, 200)
  assert.equal(bodyPreview.body.dryRun, true)
  assert.deepEqual(await applyCalls(), [['agent', 'apply', 'dsh-main', '--dry-run']])

  // Query preview forms are previews too.
  for (const query of ['preview=1', 'dry-run=1']) {
    await fake.clearCalls()
    const response = await post({}, `/_tmax/connect?${query}`)
    assert.equal(response.statusCode, 200, query)
    assert.equal(response.body.dryRun, true, query)
    assert.deepEqual(await applyCalls(), [['agent', 'apply', 'dsh-main', '--dry-run']], query)
  }

  // A request that does not state its intent is refused before any tmax call.
  for (const payload of [{}, { dryRun: 'yes' }, { preview: 1 }]) {
    await fake.clearCalls()
    const refused = await post(payload)
    assert.equal(refused.statusCode, 400, JSON.stringify(payload))
    assert.equal(refused.body.code, 'invalid_dry_run')
    assert.deepEqual(await applyCalls(), [], 'no native write is attempted')
  }

  // A preview statement wins over an execute statement: a query can only preview.
  await fake.clearCalls()
  const queryOverBody = await post({ dryRun: false }, '/_tmax/connect?preview=1')
  assert.equal(queryOverBody.statusCode, 200)
  assert.equal(queryOverBody.body.dryRun, true)
  assert.deepEqual(await applyCalls(), [['agent', 'apply', 'dsh-main', '--dry-run']])

  // Conflicting body spellings resolve to the safe preview.
  await fake.clearCalls()
  const conflicting = await post({ dryRun: true, preview: false })
  assert.equal(conflicting.statusCode, 200)
  assert.equal(conflicting.body.dryRun, true)
  assert.deepEqual(await applyCalls(), [['agent', 'apply', 'dsh-main', '--dry-run']])

  // Execution happens only with an explicit false in either spelling.
  for (const payload of [{ dryRun: false }, { preview: false }]) {
    await fake.clearCalls()
    const executed = await post(payload)
    assert.equal(executed.statusCode, 200, JSON.stringify(payload))
    assert.equal(executed.body.dryRun, false)
    assert.deepEqual(await applyCalls(), [['agent', 'apply', 'dsh-main']], JSON.stringify(payload))
  }
})


// ---------------------------------------------------------------------------
// Tests: home ownership and the S2 comment fix.







test('live provider records are reduced to ids for ownership and liveness', async () => {
  assert.deepEqual(providerIds(['a', { id: 'b', name: 'B' }, { name: 'no-id' }, null, 7, '']), ['a', 'b'])
  const { state } = await applied({ context: { liveNames: [{ id: 'tmax', name: 'Home tmax' }] } })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ownership.owner, 'other')
  assert.equal(body.ownership.conflict.code, 'route_owner_conflict')
  assert.equal(body.routes[0].active, true, 'the projected route is live under another owner')
})




// ---------------------------------------------------------------------------
// Tests: tools, skills, commands, history, and CLI helpers.

test('tmax_status returns compact sections and triggers no observation', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const tool = state.tools.find(candidate => candidate.name === 'tmax_status')
  assert.ok(tool)
  const value = await tool.execute()
  for (const key of ['ok', 'target', 'ownership', 'gateway', 'routes', 'providers', 'routing', 'diagnostics', 'blockers']) {
    assert.ok(key in value, `tmax_status exposes ${key}`)
  }
  assert.deepEqual(value.routes.map(route => route.provider), ['tmax'])
  assert.equal(value.providers.tmax.length, 3)
  assert.equal(value.routing[0].selection.qualifiedModel, 'opencode-go-2/public-model')
  const calls = await fake.calls()
  assert.equal(calls.some(argv => argv[0] === 'provider' && argv[1] === 'refresh'), false)
  assert.equal(calls.some(argv => argv[0] === 'agent' && argv[1] === 'apply'), false)
  assert.equal(calls.some(argv => argv[0] === 'history'), false)
})

test('the tmax skill states explicit operations, unknowns, and no secret handling', async () => {
  const { state } = await applied()
  const skill = state.skills.find(candidate => candidate.name === 'tmax')
  assert.ok(skill)
  const content = skill.content
  assert.match(content, /never contacts a provider/i)
  assert.match(content, /unknown/i)
  assert.match(content, /Never put keys or tokens/i)
  assert.match(content, /tmax agent apply/)
  assert.equal(/dsh connect/i.test(content), false)
  assert.equal(/always.{0,40}dry-run/i.test(content), false)
  assert.equal(/dry-run before mutations/i.test(content), false)
})

test('the /tmax command registers once and reads state or history only', async () => {
  const scenario = baseScenario()
  scenario.history = {
    code: 'ok',
    status: 'success',
    data: {
      matched_retained_count: 1,
      coverage: { process_instance: 'proc-1', retained_count: 1 },
      records: [{ request: 'r1', time: '2026-11-23T09:00:00Z', time_basis: 'completed', outcome: 'ok', model: 'public-model', protocol: 'responses', provider: 'p' }],
    },
  }
  const { fake, state } = await applied({ scenario })
  assert.equal(state.commands.length, 1)
  const command = state.commands[0]
  assert.equal(command.name, 'tmax')

  await fake.clearCalls()
  const status = await command.handler({ rawInput: 'status' })
  assert.equal(status.kind, 'success')
  assert.match(status.text, /tmax state: readable/)
  let calls = await fake.calls()
  assert.equal(calls.some(argv => argv[0] === 'history'), false)

  await fake.clearCalls()
  const history = await command.handler({ rawInput: 'history' })
  assert.equal(history.kind, 'success')
  assert.match(history.text, /records: 1 matched/)
  calls = await fake.calls()
  assert.deepEqual(calls, [['history', 'list']], 'the history command is one bounded read')

  const unknown = await command.handler({ rawInput: 'galaxy' })
  assert.equal(unknown.kind, 'error')
  assert.match(unknown.text, /unknown \/tmax subcommand/)
})

test('GET /_tmax/history maps a retained-history read', async () => {
  const scenario = baseScenario()
  scenario.history = {
    code: 'ok',
    status: 'success',
    data: {
      coverage: { process_instance: 'proc-1', retained_count: 2, oldest: '2026-11-23T09:00:00Z' },
      records: [
        { request: 'r1', time: '2026-11-23T09:00:00Z', time_basis: 'completed', outcome: 'ok', model: 'public-model', protocol: 'responses', provider: 'p' },
        { request: 'r2', time: '2026-11-23T09:01:00Z', time_basis: 'completed', outcome: 'failed', model: 'public-model', protocol: 'responses', provider: 'p', reason_code: 'upstream_rejection' },
      ],
    },
  }
  const { state } = await applied({ scenario })
  const body = await getJson(state, '/_tmax/history')
  assert.equal(body.statusCode, 200)
  assert.equal(body.body.ok, true)
  assert.equal(body.body.records.length, 2)
  assert.equal(body.body.records[0].model, 'public-model')
  assert.equal(body.body.coverage.processInstance, 'proc-1')
})

test('host CLI helpers trim DSH_HOME and keep the documented deadlines', () => {
  assert.equal(REFRESH_TIMEOUT_MS, 45_000)
  assert.equal(CONNECT_TIMEOUT_MS, 45_000)
  assert.equal(INVENTORY_TIMEOUT_MS, 45_000)
  assert.equal(resolveHome({}, { DSH_HOME: '  /tmp/dsh-home  ' }), '/tmp/dsh-home')
  assert.equal(resolveHome({}, { DSH_HOME: '   ' }), join(homedir(), '.dsh'))
  assert.equal(resolveHome({ home: '/explicit' }, { DSH_HOME: '/env' }), '/explicit')
})

test('the inventory module composes a projection-shaped object', async () => {
  const home = await emptyHome()
  const fake = await fakeTmax(baseScenario(), home)
  const inventory = await readInventory({ target: 'dsh-main', tmaxPath: fake.path })
  assert.equal(inventory.schema_version, 1)
  assert.equal(typeof inventory.generated_at, 'string')
  assert.equal(inventory.configuration_revision, 'rev-1')
  assert.equal(inventory.authority, '/tmp/tmax')
  assert.equal(inventory.tmax_version, '0.1.0')
  assert.equal(inventory.target.id, 'dsh-main')
  assert.deepEqual(inventory.target.routes, ['tmax'])
  assert.deepEqual(inventory.routes.map(route => route.name), ['tmax'])
  assert.deepEqual(inventory.routes[0].models.map(model => model.id), ['public-model', 'text-only', 'no-context'])
  assert.deepEqual(inventory.blockers, [])

  // The pure composer never invents a target: an empty dossier is empty.
  const empty = composeInventory({ inspect: { agent: 'dsh', providers: [], native_projection: [] } })
  assert.deepEqual(empty.routes, [])
  assert.deepEqual(empty.target.routes, [])
  assert.deepEqual(empty.blockers.map(blocker => blocker.code), ['gateway_unconfigured'])
})

test('the module exports the DSH plugin contract', async () => {
  const module = await import('../index.js')
  assert.equal(module.name, 'tmax')
  assert.deepEqual(module.inject, ['llm', 'settings', 'credentials'])
  assert.equal(typeof module.apply, 'function')
})

test('applied profile settings own the routes, so the page registers nothing', async () => {
  // A home whose profile settings already carry tmax's provider, as apply
  // writes them: DSH's own adapter serves that route, so a second registration
  // would offer the same upstream twice.
  const home = await emptyHome()
  await mkdir(join(home, 'profiles/desktop'), { recursive: true })
  // The provider carries its entry's name, so this is the route the target
  // declares as well.
  await writeFile(
    join(home, 'profiles/desktop/cordis.patch.yml'),
    '- id: llm-pi-ai\n  config:\n    providers:\n      tmax:\n        api: openai-completions\n        baseURL: https://example.invalid/v1\n        models:\n          - id: first\n',
  )
  const { state } = await applied({ config: { home } })
  assert.deepEqual(state.routes, [])
  assert.equal(state.registrations.length, 0)
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ownership.owner, 'settings')
  assert.equal(body.ownership.scope, 'profile_scoped')
})

test('a comment or another row\'s field is never read as an applied provider', async () => {
  const home = await emptyHome()
  await mkdir(join(home, 'profiles/desktop'), { recursive: true })
  // The target's route is `tmax`; nothing here declares it under `llm-pi-ai`.
  await writeFile(
    join(home, 'profiles/desktop/cordis.patch.yml'),
    '# tmax: applied elsewhere\n- id: other\n  config:\n    tmax: true\n    nested:\n      tmax: 1\n',
  )
  const { state } = await applied({ config: { home } })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ownership.owner, 'unapplied')
  assert.equal(body.ownership.conflict, null)
})

test('conflicts are decided per route against the settings keys the registry spells', async () => {
  // Route `tmax` is applied and served by the settings; route `other` is live
  // under a foreign owner. One applied route must not hide the other's conflict.
  const home = await emptyHome()
  await mkdir(join(home, 'profiles/desktop'), { recursive: true })
  await writeFile(
    join(home, 'profiles/desktop/cordis.patch.yml'),
    '- id: llm-pi-ai\n  config:\n    providers:\n      tmax:\n        api: openai-completions\n',
  )
  const scenario = withSecondRoute(baseScenario(), { name: 'other', credentialRef: 'TMAX_OTHER' })
  const { state } = await applied({
    scenario,
    config: { home },
    // The registry names providers exactly as the settings keys spell them.
    context: { liveNames: ['tmax', 'other'] },
  })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ownership.owner, 'unapplied', 'one of two routes applied is not fully applied')
  assert.equal(body.ownership.reason, null, 'something was applied, so it is not "never applied"')
  assert.deepEqual(body.ownership.conflict.routes, ['other'])
})

test('a settings document that will not parse leaves ownership unreadable, not unapplied', async () => {
  const home = await emptyHome()
  await mkdir(join(home, 'profiles/desktop'), { recursive: true })
  await writeFile(
    join(home, 'profiles/desktop/cordis.patch.yml'),
    '- id: llm-pi-ai\n  config:\n    providers:\n      tmax:\n        api: openai-completions\n',
  )
  await mkdir(join(home, 'profiles/broken'), { recursive: true })
  // A tab indentation is not YAML: the reader throws.
  await writeFile(join(home, 'profiles/broken/cordis.patch.yml'), '- id: other\n\tconfig: {}\n')
  const { state } = await applied({ config: { home } })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ownership.owner, 'unknown')
  assert.equal(body.ownership.reason, 'settings_unreadable')
})

test('without the entry-list parser, ownership is unreadable rather than guessed', async () => {
  const home = await emptyHome()
  await mkdir(join(home, 'profiles/desktop'), { recursive: true })
  await writeFile(
    join(home, 'profiles/desktop/cordis.patch.yml'),
    '- id: llm-pi-ai\n  config:\n    providers:\n      tmax:\n        api: openai-completions\n',
  )
  const { state } = await applied({ config: { home }, context: { parserUnavailable: true } })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.ownership.owner, 'unknown')
  assert.equal(body.ownership.reason, 'settings_unreadable')
  assert.equal(body.ownership.conflict, null)
})

test('the catalog projects provider source, aliases, joined state, default, and protocols', async () => {
  const scenario = baseScenario()
  delete scenario.providerList.data.providers[0].source
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  const catalog = body.catalog
  assert.equal(catalog.state, 'available')
  assert.equal(catalog.entriesState, 'available')
  const tmax = catalog.providers.find(provider => provider.id === 'tmax')
  assert.equal(tmax.source, 'gateway')
  assert.equal(tmax.entry, 'tmax')
  assert.deepEqual(tmax.entries.map(entry => entry.name), ['tmax'])
  assert.equal(tmax.totalCount, 4)
  assert.equal(tmax.joinedCount, 3)
  assert.equal(tmax.models.find(model => model.id === 'deepseek-v4.1-flash').default, true)
  assert.equal(tmax.models.find(model => model.id === 'public-model').joined, true)
  assert.deepEqual(tmax.models.find(model => model.id === 'public-model').joinedProtocols, ['responses'])
  const kimi = catalog.providers.find(provider => provider.id === 'kimi')
  assert.equal(kimi.source, 'upstream', 'provider-inspect data.source is the fallback')
  assert.equal(kimi.entry, null)
  assert.equal(kimi.totalCount, 1)
  assert.equal(kimi.joinedCount, 0)
  assert.deepEqual(kimi.models[0].protocols, ['chat-completions', 'responses'])
  assert.equal(kimi.models[0].declaredProtocols, null, 'unreported is not a readable empty list')
  assert.equal(catalog.default.provider, 'tmax')
  assert.equal(catalog.default.model, 'deepseek-v4.1-flash')
})

test('the catalog default maps a direct entry name back to its provider', async () => {
  const scenario = baseScenario()
  const inspect = JSON.parse(JSON.stringify(scenario.agentInspect.data))
  inspect.providers.push({
    name: 'kimi-entry',
    source: { provider: 'kimi' },
    relayed: false,
    models: [{ id: 'k3.1-pro', protocol: 'responses' }],
  })
  inspect.default = { provider: 'kimi-entry', id: 'k3.1-pro', protocol: 'responses' }
  scenario.agentInspect = { code: 'ok', status: 'success', data: inspect }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  const kimi = body.catalog.providers.find(provider => provider.id === 'kimi')
  assert.equal(body.catalog.default.provider, 'kimi')
  assert.equal(kimi.models[0].default, true)
  assert.equal(kimi.models[0].joined, true)
})

test('an unreadable catalog source reports unavailable, not an empty catalog', async () => {
  const scenario = baseScenario()
  scenario.modelList = { code: 'unknown_resource', status: 'error', data: null }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.catalog.state, 'unavailable')
  assert.equal(body.catalog.providers.find(provider => provider.id === 'kimi').totalCount, null)
  assert.equal(body.catalog.providers.find(provider => provider.id === 'kimi').joinedCount, null)
  assert.ok(body.errors.some(error => error.section === 'catalog' && error.code === 'unknown_resource'))
})

test('a provider-list failure makes the catalog unavailable even when models are readable', async () => {
  const scenario = baseScenario()
  scenario.providerList = { code: 'unknown_resource', status: 'error', data: null }
  const { state } = await applied({ scenario })
  const body = (await getJson(state, '/_tmax/state')).body
  assert.equal(body.catalog.state, 'unavailable')
  assert.deepEqual(body.catalog.providers, [])
  assert.ok(body.errors.some(error => error.section === 'providers' && error.code === 'unknown_resource'))
})
