import assert from 'node:assert/strict'
import { chmod, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import {
  COMMAND_SUBCOMMANDS,
  HISTORY_READ_TIMEOUT_MS,
  HISTORY_RECORD_LIMIT,
  apply,
  formatTmaxGateway,
  formatTmaxHistory,
  formatTmaxProviders,
  formatTmaxRouting,
  formatTmaxSection,
  parseTmaxCommand,
} from '../index.js'

// ---------------------------------------------------------------------------
// Fixtures: the target inventory behind the state service.

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
      models: [
        { id: 'deepseek-v4.1-flash', protocol: 'responses' },
        { id: 'glm-5.4', protocol: 'responses' },
      ],
    }],
    default: { provider: 'tmax', id: 'deepseek-v4.1-flash', protocol: 'responses' },
    native_projection: [{
      name: 'tmax',
      source: { provider: 'tmax' },
      models: [
        { id: 'deepseek-v4.1-flash', protocol: 'responses', capabilities: { contextWindow: 500000 }, efforts: [], projection: { fields: {} } },
        { id: 'glm-5.4', protocol: 'responses', capabilities: { contextWindow: 200000 }, efforts: [], projection: { fields: {} } },
      ],
    }],
    native_default: { state: 'available', fields: { provider: 'tmax', model: 'deepseek-v4.1-flash', present: true } },
    installation: { executable: '/tmp/bin/dsh', version: '0.2.3-alpha.2', state: 'detected', source: 'https://github.com/deepseek-ai/deepseek-harness' },
    applied_state: 'unknown',
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

function providerListData() {
  return {
    revision: 'rev-1',
    providers: [
      {
        id: 'kimi',
        name: 'Kimi Coding Plan',
        enabled: true,
        authentication: 'api-key',
        affected_targets: 1,
        observations: { billing_state: 'observed', observations: [{ scope: 'quota', state: 'observed', last_attempt_outcome: 'completed', last_attempt_reason_code: null }] },
        quota_suspension: { state: 'unsupported' },
      },
      {
        id: 'cpa-us2',
        name: null,
        enabled: true,
        authentication: 'api-key',
        affected_targets: 1,
        observations: { billing_state: 'unknown', observations: [{ scope: 'quota', state: 'unknown', last_attempt_outcome: 'unavailable', last_attempt_reason_code: 'unsupported_source' }] },
        quota_suspension: { state: 'unsupported' },
      },
    ],
  }
}

function kimiInspectData() {
  return {
    id: 'kimi',
    provider: { enabled: true, integration: 'kimi-code-plan-cn', overrides: { name: 'Kimi Coding Plan' }, auth: { method: 'api-key' }, subscription: null },
    observations: {
      account_identity: null,
      billing: {
        kind: 'plan_allowance',
        state: 'observed',
        dashboard: 'https://www.kimi.com/code/console',
        freshness: { state: 'unqualified' },
        observation: { observed_at: '2026-11-23T06:26:05Z' },
        display: {
          notes: [],
          rows: [{ label: { key: 'weekly_allowance' }, basis: 'counters', counters: { limit: '100', remaining: '100' } }],
        },
      },
    },
    quota_suspension: { state: 'unsupported' },
  }
}

function cpaInspectData() {
  return {
    id: 'cpa-us2',
    provider: { enabled: true, integration: 'custom', overrides: {}, auth: { method: 'api-key' }, subscription: null },
    observations: {
      account_identity: null,
      billing: { kind: 'unknown', state: 'unknown', dashboard: '', freshness: { state: 'missing' }, observation: null, display: null },
    },
    quota_suspension: { state: 'unsupported' },
    failure_state: { state: 'not_configured', consecutive_failures: 0, active_priority: null, expires_at: null, reason_code: null },
  }
}

function explainData() {
  return {
    selection: { qualified_model: 'opencode-go-2/deepseek-v4.1-flash', provider: 'opencode-go-2', model: 'deepseek-v4.1-flash', priority: 1 },
    priority_evaluations: [
      { qualified_model: 'opencode-go/deepseek-v4.1-flash', priority_evaluation: { effective_priority: 0 } },
      { qualified_model: 'opencode-go-2/deepseek-v4.1-flash', priority_evaluation: { effective_priority: 1 } },
    ],
    excluded_candidates: [],
  }
}

const success = data => ({ code: 'ok', status: 'success', data })

function historyRecord(index) {
  return {
    request: `req-${index}`,
    time: `2026-11-23T10:${String(index).padStart(2, '0')}:00Z`,
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

function historyCoverage(count) {
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

function historyEnvelope(recordCount = 3) {
  return success({
    coverage: historyCoverage(recordCount),
    requested_range: { since: null, until: null },
    includes_in_progress: true,
    matched_retained_count: recordCount,
    omitted_by_limit: 0,
    records: Array.from({ length: recordCount }, (_, index) => historyRecord(index)),
  })
}

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
  const directory = await mkdtemp(join(tmpdir(), 'tmax-command-cli-'))
  const scenarioPath = join(directory, 'scenario.json')
  const logPath = join(directory, 'calls.jsonl')
  const scriptPath = join(directory, 'tmax')
  await writeFile(scenarioPath, JSON.stringify(scenario))
  await writeFile(logPath, '')
  await writeFile(scriptPath, FAKE_SCRIPT.replace('__SCENARIO__', JSON.stringify(scenarioPath)).replace('__LOG__', JSON.stringify(logPath)))
  await chmod(scriptPath, 0o755)
  return {
    path: scriptPath,
    async update(next) {
      await writeFile(scenarioPath, JSON.stringify(next))
    },
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

// ---------------------------------------------------------------------------
// Fake plugin context: the command registry is the surface under test.

function fakeContext({ withCommands = true } = {}) {
  const state = {
    commands: [],
    httpRoutes: [],
    effects: [],
  }
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
      if (name === 'commands' && withCommands) {
        return {
          register(definition) {
            state.commands.push(definition)
            return () => {
              state.commands = state.commands.filter(candidate => candidate !== definition)
            }
          },
        }
      }
      if (name === 'credentials') {
        return {
          credentialRef: ref => ref,
          resolve: async () => undefined,
          describe: async () => ({ configured: false }),
        }
      }
      if (name === 'settings') return { describe: () => [] }
      if (name === 'agentDefaultModel') return { currentSelection: () => ({ provider: 'tmax', model: 'gpt-6.1-sol' }) }
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
      if (name === 'commands' && !withCommands) return undefined
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
      // Cordis runs the callback only while every injected service exists.
      if (!deps.every(name => ctx.get(name) !== undefined)) return { dispose() {} }
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

function baseScenario() {
  return {
    dispatch: {
      'agent inspect dsh-main': success(agentInspectData()),
      'gateway status': success(gatewayStatusData()),
      status: success({ config_path: '/tmp/tmax/tmax.toml', file_revision: 'rev-1', configuration_state: 'valid', daemon: gatewayStatusData() }),
      'provider list': success(providerListData()),
      'model list': success({ models: [
        { id: 'kimi/k3.1-pro', kind: 'concrete', enabled: true, availability: 'unverified', published_protocols: ['chat-completions', 'responses'] },
        { id: 'cpa-us2/gpt-6-astra', kind: 'concrete', enabled: true, availability: 'unverified', published_protocols: ['chat-completions', 'responses'] },
        { id: 'deepseek-v4.1-flash', kind: 'alias', enabled: true, availability: 'unverified', protocols: ['responses'], published_protocols: ['responses'] },
        { id: 'glm-5.4', kind: 'alias', enabled: true, availability: 'unverified', protocols: ['responses'], published_protocols: ['responses'] },
      ] }),
      'provider inspect kimi': success(kimiInspectData()),
      'provider inspect cpa-us2': success(cpaInspectData()),
      'config validate': success({ valid: true, revision: 'rev-1', snapshot_state: 'available' }),
      'model explain deepseek-v4.1-flash --protocol responses': success(explainData()),
      'model explain glm-5.4 --protocol responses': success({ selection: null, priority_evaluations: [], excluded_candidates: [] }),
    },
    history: historyEnvelope(3),
  }
}

async function applied({ scenario = baseScenario(), withCommands = true } = {}) {
  const fake = await fakeTmax(scenario)
  const home = await mkdtemp(join(tmpdir(), 'tmax-command-home-'))
  const { ctx, state } = fakeContext({ withCommands })
  await apply(ctx, { target: 'dsh-main', tmaxPath: fake.path, home, refreshSeconds: 3600 })
  return { fake, state, home }
}

function commandOf(state) {
  const definition = state.commands[0]
  assert.ok(definition, 'the /tmax command is registered')
  return definition
}

async function runCommand(state, rawInput) {
  return commandOf(state).handler({ rawInput })
}

// ---------------------------------------------------------------------------
// Pure helpers

test('the command surface parses only a closed subcommand set', () => {
  assert.deepEqual([...COMMAND_SUBCOMMANDS], ['status', 'providers', 'routing', 'gateway', 'diagnostics', 'history'])
  // A retained-history read can wait on daemon identity, the control RPC, and
  // an owned process start, so it keeps startup/process margin.
  assert.equal(HISTORY_READ_TIMEOUT_MS, 45_000)
  assert.deepEqual(parseTmaxCommand(undefined), { ok: true, subcommand: 'status' })
  assert.deepEqual(parseTmaxCommand(''), { ok: true, subcommand: 'status' })
  assert.deepEqual(parseTmaxCommand('  '), { ok: true, subcommand: 'status' })
  assert.deepEqual(parseTmaxCommand('status'), { ok: true, subcommand: 'status' })
  assert.deepEqual(parseTmaxCommand(' providers '), { ok: true, subcommand: 'providers' })
  const unknown = parseTmaxCommand('bogus')
  assert.equal(unknown.ok, false)
  assert.equal(unknown.code, 'unknown_subcommand')
  assert.match(unknown.message, /usage: \/tmax \[status\|providers\|routing\|gateway\|diagnostics\|history\]/)
  assert.equal(parseTmaxCommand('status extra').ok, false)
  // Unbounded client text is bounded in the error.
  const long = parseTmaxCommand('x'.repeat(500))
  assert.equal(long.ok, false)
  assert.ok(long.message.length < 300)
})

test('section formatters state unknown and unavailable evidence instead of inventing values', () => {
  assert.match(formatTmaxSection('history', {}), /unsupported \/tmax subcommand/)
  assert.equal(formatTmaxGateway({ gateway: null }), 'gateway: unavailable (no gateway section was reported)')

  // A section the state reports as failed is unavailable, never a zero count.
  const providersError = { section: 'providers', code: 'provider_list_unavailable', message: 'tmax snapshot providers are unavailable' }
  const failedProviders = formatTmaxProviders({
    ok: true,
    errors: [providersError],
    providers: { tmax: [], agent: [] },
  })
  assert.match(failedProviders, /^tmax providers: unavailable \(provider_list_unavailable: tmax snapshot providers are unavailable\)/)
  assert.equal(/tmax providers: 0/.test(failedProviders), false)

  const failedRouting = formatTmaxRouting({
    ok: true,
    errors: [{ section: 'routing', code: 'routing_unavailable', message: 'tmax snapshot routing is unavailable' }],
    routing: [],
  })
  assert.match(failedRouting, /^routing: unavailable \(routing_unavailable: tmax snapshot routing is unavailable\)/)
  assert.equal(/routing: 0/.test(failedRouting), false)

  // An unavailable object in place of the list is the same failure line.
  assert.match(
    formatTmaxProviders({ providers: { tmax: { state: 'unavailable', reasonCode: 'x' }, agent: [] } }),
    /^tmax providers: unavailable \(providers_unavailable: the providers section was not reported\)/,
  )
  assert.match(
    formatTmaxRouting({ routing: { state: 'unavailable', reason_code: 'x' } }),
    /^routing: unavailable \(routing_unavailable: the routing section was not reported\)/,
  )

  const zeroQuota = formatTmaxProviders({
    providers: {
      tmax: [{
        id: 'zero',
        name: 'Zero',
        enabled: true,
        authentication: 'api-key',
        integration: 'custom',
        billing: { kind: 'plan_allowance', state: 'observed', freshness: 'current', observedAt: '2026-11-23T10:00:00Z', rows: [{ label: { key: 'weekly_allowance' }, percent: 0 }] },
        suspension: { state: 'unsupported' },
        failure: { state: 'not_configured', consecutiveFailures: 0 },
        error: null,
      }],
      agent: [],
    },
  })
  assert.match(zeroQuota, /quota reading: 0% \(weekly_allowance\)/, 'a zero share stays visible')
  assert.match(zeroQuota, /failure: not_configured/)

  const errorRouting = formatTmaxRouting({
    routing: [{
      model: 'm',
      protocol: 'responses',
      kind: 'concrete',
      availability: 'unverified',
      candidateCount: 1,
      selection: null,
      candidates: [],
      excluded: [],
      error: { code: 'model_explain_unavailable', message: 'model explain did not complete' },
    }],
  })
  assert.match(errorRouting, /selection: none reported/)
  assert.match(errorRouting, /error: model_explain_unavailable: model explain did not complete/)
})

test('history formatting states unavailability with a reason, never an empty history', () => {
  const blocked = formatTmaxHistory({
    ok: false,
    code: 'daemon_identity_unavailable',
    message: 'Daemon identity challenge, authority, or process did not match',
    envelope: { code: 'daemon_identity_unavailable', status: 'blocked', data: null },
    data: null,
  })
  assert.match(blocked, /history unavailable: daemon_identity_unavailable/)
  assert.match(blocked, /Daemon identity challenge/)
  assert.match(blocked, /coverage: not reported/)

  const noDaemon = formatTmaxHistory({
    ok: true,
    envelope: { code: 'ok', status: 'success' },
    data: { snapshot_state: 'absent', coverage: { prior_process_coverage: 'unavailable' }, records: [], reason_code: 'no_daemon_history' },
  })
  assert.match(noDaemon, /history unavailable: no retained daemon history/)
  assert.match(noDaemon, /reason: no_daemon_history/)
  assert.match(noDaemon, /prior process coverage: unavailable/)

  const transport = formatTmaxHistory({ ok: false, envelope: undefined, message: 'tmax did not answer before the timeout' })
  assert.match(transport, /history unavailable: tmax did not answer before the timeout/)

  // A successful envelope without history evidence is not an empty history.
  const noData = formatTmaxHistory({ ok: true, envelope: { code: 'ok', status: 'success' }, data: {} })
  assert.equal(noData, 'history unavailable: tmax returned no history data')
})

// ---------------------------------------------------------------------------
// Registration and handler behavior

test('the /tmax command registers once with a closed hint and disposes cleanly', async () => {
  const { state } = await applied()
  assert.equal(state.commands.length, 1)
  const definition = commandOf(state)
  assert.equal(definition.name, 'tmax')
  assert.equal(typeof definition.description, 'string')
  assert.ok(definition.description.length > 0)
  assert.deepEqual(definition.input, { hint: '[status|providers|routing|gateway|diagnostics|history]' })
  assert.equal(typeof definition.handler, 'function')

  for (const dispose of state.effects) {
    if (typeof dispose === 'function') dispose()
  }
  assert.equal(state.commands.length, 0, 'the command is disposed with the plugin')
})

test('an absent commands service registers nothing and does not fail activation', async () => {
  const { state } = await applied({ withCommands: false })
  assert.deepEqual(state.commands, [])
  assert.ok(state.httpRoutes.length > 0, 'the rest of the plugin still activates')
})

test('empty input and status summarize the read model without observing or writing', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const summary = await runCommand(state, '')
  assert.equal(summary.kind, 'success')
  assert.match(summary.text, /tmax state: readable/)
  assert.match(summary.text, /target: dsh-main/)
  assert.match(summary.text, /effective default: tmax\/gpt-6\.1-sol/)
  assert.match(summary.text, /routes: 1/)
  assert.match(summary.text, /  - tmax: not active; credential missing; 2 model\(s\)/)
  assert.match(summary.text, /gateway: configured yes; ready unknown/)
  assert.match(summary.text, /providers: 2/)
  assert.match(summary.text, /blockers: /)

  const explicit = await runCommand(state, 'status')
  assert.equal(explicit.kind, 'success')
  // Each read stamps its own generation time; every other line is identical.
  const stable = text => text.replace(/generated: [^;]+;/u, 'generated: <t>;')
  assert.equal(stable(explicit.text), stable(summary.text), 'status is the default subcommand')

  const calls = await fake.calls()
  assert.ok(calls.length >= 2)
  const readOnly = argv => ['agent', 'gateway', 'status', 'provider', 'config', 'model'].includes(argv[0])
    || (argv[0] === 'history' && argv[1] === 'list')
  assert.equal(calls.every(readOnly), true, 'read-only inventory reads only')
  const credentialRef = 'TMAX_TEST'
  assert.equal(summary.text.includes(credentialRef), false, 'a credential reference never enters command text')
})

test('providers distinguishes unknown quota from zero and reports repair state', async () => {
  const { state } = await applied()
  const result = await runCommand(state, 'providers')
  assert.equal(result.kind, 'success')
  assert.match(result.text, /tmax providers: 2/)
  assert.match(result.text, /- kimi \(Kimi Coding Plan\)/)
  assert.match(result.text, /quota: observed/)
  assert.match(result.text, /quota reading: remaining 100 of 100/)
  assert.match(result.text, /- cpa-us2/)
  assert.match(result.text, /quota: unknown/)
  assert.match(result.text, /quota reason: unsupported_source/)
  assert.match(result.text, /last attempt: unavailable/)
  assert.match(result.text, /suspension: unsupported/)
  assert.match(result.text, /failure: not_configured/)
  const credentialRef = 'TMAX_TEST'
  assert.equal(result.text.includes(credentialRef), false, 'no credential reference in provider text')
})

test('routing and gateway sections report selections and unknowns', async () => {
  const { state } = await applied()
  const routing = await runCommand(state, 'routing')
  assert.equal(routing.kind, 'success')
  assert.match(routing.text, /routing: 2 projected model\(s\)/)
  assert.match(routing.text, /deepseek-v4\.1-flash/)
  assert.match(routing.text, /selection: opencode-go-2\/deepseek-v4\.1-flash \(priority 1\)/)
  assert.match(routing.text, /candidates: 2 \(1 selected\); excluded: 0/)

  const gateway = await runCommand(state, 'gateway')
  assert.equal(gateway.kind, 'success')
  assert.match(gateway.text, /gateway: configured yes; ready unknown/)
  assert.match(gateway.text, /listen: 127\.0\.0\.1:17840/)
  assert.match(gateway.text, /daemon: unidentified \(daemon_identity_unavailable\)/)
  assert.match(gateway.text, /supervision: unavailable \(service_manager_unavailable\)/)
  assert.match(gateway.text, /autostart: saved intent yes; registration unavailable; unit matches no/)
  assert.match(gateway.text, /shared key: absent/)
})

test('diagnostics reports read provenance, history availability, and errors', async () => {
  const { fake, state } = await applied()
  const result = await runCommand(state, 'diagnostics')
  assert.equal(result.kind, 'success')
  assert.match(result.text, /read model: inventory/)
  assert.match(result.text, /history: unavailable \(daemon_identity_unavailable\)/)
  assert.match(result.text, /last error: none reported/)
  assert.match(result.text, /errors: 0/)

  // A typed section failure is visible, never flattened into an empty section.
  await fake.patch(current => {
    current.dispatch['provider list'] = {
      code: 'provider_list_unavailable',
      status: 'error',
      data: null,
      blockers: [{ code: 'provider_list_unavailable', message: 'provider storage is unavailable' }],
      __exit: 4,
    }
  })
  const degraded = await runCommand(state, 'diagnostics')
  assert.match(degraded.text, /errors: 1/)
  assert.match(degraded.text, /providers\/provider_list_unavailable: provider storage is unavailable/)
})

test('a failed providers, routing, or inventory read is stated, not counted as zero', async () => {
  const { fake, state } = await applied()

  // Providers section failed: no "0 providers" success claim.
  await fake.patch(current => {
    current.dispatch['provider list'] = {
      code: 'provider_list_unavailable',
      status: 'error',
      data: null,
      blockers: [{ code: 'provider_list_unavailable', message: 'provider storage is unavailable' }],
      __exit: 4,
    }
  })
  const providers = await runCommand(state, 'providers')
  assert.equal(providers.kind, 'success')
  assert.match(providers.text, /^tmax providers: unavailable \(provider_list_unavailable: provider storage is unavailable\)/)
  assert.equal(/tmax providers: 0/.test(providers.text), false, 'a failed providers read is not zero providers')
  const statusWithProviders = await runCommand(state, '')
  assert.match(statusWithProviders.text, /providers: unavailable \(provider_list_unavailable: /)
  assert.equal(/providers: 0 tmax/.test(statusWithProviders.text), false)

  // A routing row failure is stated per row, not as a zero-model claim.
  await fake.patch(current => {
    current.dispatch['provider list'] = baseScenario().dispatch['provider list']
    current.dispatch['model explain deepseek-v4.1-flash --protocol responses'] = {
      code: 'model_explain_unavailable',
      status: 'error',
      data: null,
      blockers: [{ code: 'model_explain_unavailable', message: 'routing evidence is unavailable' }],
      __exit: 4,
    }
  })
  const routing = await runCommand(state, 'routing')
  assert.equal(routing.kind, 'success')
  assert.match(routing.text, /routing: 2 projected model\(s\)/)
  assert.match(routing.text, /error: model_explain_unavailable: routing evidence is unavailable/)
  assert.equal(/routing: 0/.test(routing.text), false, 'a failed explanation is not zero models')

  // A failed inventory leaves routes unknown, not empty.
  await fake.patch(current => {
    current.dispatch['model explain deepseek-v4.1-flash --protocol responses'] =
      baseScenario().dispatch['model explain deepseek-v4.1-flash --protocol responses']
    current.dispatch['agent inspect dsh-main'] = {
      code: 'unknown_target',
      status: 'error',
      data: null,
      blockers: [{ code: 'unknown_target', message: 'Unknown DSH target "missing".' }],
      __exit: 2,
    }
  })
  const status = await runCommand(state, '')
  assert.match(status.text, /routes: unavailable \(unknown_target: /)
  assert.equal(/routes: 0/.test(status.text), false, 'a failed inventory read is not zero routes')
  // Readable sections still summarize.
  assert.match(status.text, /providers: 2/)
})

test('history lists bounded retained records and states unavailability with the reason', async () => {
  const { fake, state } = await applied()
  const secret = 'sk-history-SECRET'
  await fake.patch(current => {
    current.history = historyEnvelope(14)
    current.history.data.records[0].token = secret
  })
  await fake.clearCalls()
  const listed = await runCommand(state, 'history')
  assert.equal(listed.kind, 'success')
  assert.match(listed.text, /process: proc-1/)
  assert.match(listed.text, /retained: 14; oldest: 2026-11-23T10:00:00Z/)
  assert.match(listed.text, /records: 14 matched; 0 omitted by limit; in-progress included yes/)
  assert.match(listed.text, new RegExp(`retained records listed: ${HISTORY_RECORD_LIMIT} of 14`))
  assert.match(listed.text, /opencode-go-2\/deepseek-v4\.1-flash \[responses\]/)
  assert.match(listed.text, /reason upstream_rejection/)
  assert.equal(listed.text.includes(secret), false, 'record fields the contract does not name are never echoed')
  assert.deepEqual(await fake.calls(), [['history', 'list']])

  // A blocked daemon read is a success command that states the reason.
  await fake.patch(current => {
    current.history = {
      code: 'daemon_identity_unavailable',
      status: 'blocked',
      data: null,
      blockers: [{ code: 'daemon_identity_unavailable', message: 'Daemon identity challenge, authority, or process did not match' }],
      __exit: 3,
    }
  })
  const blocked = await runCommand(state, 'history')
  assert.equal(blocked.kind, 'success')
  assert.match(blocked.text, /history unavailable: daemon_identity_unavailable/)
  assert.match(blocked.text, /Daemon identity challenge/)
  assert.equal(/records: 0/.test(blocked.text), false, 'an unavailable history is not an empty history')

  // A completed read with no daemon history is an availability fact.
  await fake.patch(current => {
    current.history = success({
      snapshot_state: 'absent',
      coverage: { prior_process_coverage: 'unavailable' },
      records: [],
      reason_code: 'no_daemon_history',
    })
  })
  const absent = await runCommand(state, 'history')
  assert.equal(absent.kind, 'success')
  assert.match(absent.text, /history unavailable: no retained daemon history/)
  assert.match(absent.text, /reason: no_daemon_history/)

  // A transport failure is stated, not thrown.
  await fake.patch(current => {
    current.history = { __raw: 'not a tmax envelope', __exit: 4 }
  })
  const transport = await runCommand(state, 'history')
  assert.equal(transport.kind, 'success')
  assert.match(transport.text, /^history unavailable: /)
  assert.match(transport.text, /did not answer|could not be executed/)
})

test('unknown subcommands are command errors before any read', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  const unknown = await runCommand(state, 'bogus')
  assert.equal(unknown.kind, 'error')
  assert.match(unknown.text, /unknown \/tmax subcommand "bogus"/)
  assert.match(unknown.text, /usage: \/tmax/)
  const extra = await runCommand(state, 'status extra')
  assert.equal(extra.kind, 'error')
  assert.deepEqual(await fake.calls(), [], 'the parser never reads tmax for an unknown subcommand')
})

test('command reads never observe, refresh, apply, open, or mutate tmax', async () => {
  const { fake, state } = await applied()
  await fake.clearCalls()
  for (const subcommand of ['', 'providers', 'routing', 'gateway', 'diagnostics', 'history']) {
    const result = await runCommand(state, subcommand)
    assert.equal(result.kind, 'success', `${subcommand || 'status'}: ${result.text}`)
  }
  const calls = await fake.calls()
  assert.ok(calls.length >= COMMAND_SUBCOMMANDS.length)
  const allowed = argv => ['agent', 'gateway', 'status', 'provider', 'config', 'model'].includes(argv[0])
    || (argv[0] === 'history' && argv[1] === 'list')
  for (const argv of calls) {
    assert.ok(allowed(argv), `unexpected command argv: ${argv.join(' ')}`)
  }
  assert.equal(
    calls.some(argv => argv[0] === 'provider' && ['refresh', 'quota-resume', 'failure-reset'].includes(argv[1])),
    false,
    'no provider observation or repair',
  )
  assert.equal(calls.some(argv => argv[0] === 'agent' && argv[1] === 'apply'), false, 'no connect/apply')
  assert.equal(
    calls.some(argv => argv[0] === 'gateway' && ['start', 'stop', 'restart'].includes(argv[1])),
    false,
    'no lifecycle action',
  )
  assert.equal(calls.some(argv => argv[0] === 'web'), false, 'no management action')
  assert.ok(state.httpRoutes.length > 0, 'the HTTP routes stay registered alongside the command')
})
