import assert from 'node:assert/strict'
import test from 'node:test'
import { readFile } from 'node:fs/promises'

// The page is a loader bundle, not an ES module. Importing it only registers a
// factory; every test reads the same source but instantiates the factory with
// its own React/host fakes, so no DOM (and no browser) is required.
const CLIENT_URL = new URL('../client.js', import.meta.url)
const FIXTURE_URL = new URL('./fixtures/state.json', import.meta.url)
const STATE_NS = 'tmaxDshPage'

const fixture = JSON.parse(await readFile(FIXTURE_URL, 'utf8'))
const source = await readFile(CLIENT_URL, 'utf8')
const connected = fixture.cases.connectedHomeOwned

let loadedEntry
async function loadEntry() {
  if (loadedEntry !== undefined) return loadedEntry
  const previous = globalThis.window
  globalThis.window = { __ModuleLoader__: { load: entry => { loadedEntry = entry } } }
  try {
    await import(CLIENT_URL.href)
  } finally {
    if (previous === undefined) delete globalThis.window
    else globalThis.window = previous
  }
  assert.ok(loadedEntry, 'the loader registered a factory')
  return loadedEntry
}

/** A no-op React good enough for pure factories (no component is rendered). */
function inertReact() {
  return {
    createElement: () => ({}),
    useState: value => [value, () => {}],
    useEffect: () => {},
    useCallback: fn => fn,
  }
}

async function moduleWith(React) {
  const entry = await loadEntry()
  return entry.factory(specifier => {
    assert.equal(specifier, 'react', 'the client requires only react')
    return React
  })
}

function t(key) { return key === 'colon' ? ':' : key }

// ---------------------------------------------------------------------------
// A tiny React runtime: three hooks, one render pass, synchronous effect flush.
// Its only jobs are to run the page's mount effect and to let a test click the
// buttons the page actually renders.
// ---------------------------------------------------------------------------

function createRuntime() {
  const cells = []
  let cursor = 0
  let dirty = false
  let effectQueue = []
  let lastTree

  const React = {
    createElement(type, props, ...children) {
      return {
        type,
        props: props ?? {},
        children: children.flat(Infinity).filter(child => child !== null && child !== undefined && child !== false && child !== true),
      }
    },
    useState(initial) {
      const index = cursor
      cursor += 1
      if (cells[index] === undefined) {
        cells[index] = { value: typeof initial === 'function' ? initial() : initial }
      }
      const cell = cells[index]
      return [cell.value, next => {
        cell.value = typeof next === 'function' ? next(cell.value) : next
        dirty = true
      }]
    },
    useEffect(fn, deps) {
      const index = cursor
      cursor += 1
      const cell = cells[index] ?? (cells[index] = {})
      const changed = cell.deps === undefined || deps === undefined
        || deps.length !== cell.deps.length || deps.some((item, at) => item !== cell.deps[at])
      cell.deps = deps
      if (changed) effectQueue.push({ index, fn })
    },
    useCallback(fn, deps) {
      const index = cursor
      cursor += 1
      const cell = cells[index] ?? (cells[index] = {})
      const changed = cell.deps === undefined || deps === undefined
        || deps.length !== cell.deps.length || deps.some((item, at) => item !== cell.deps[at])
      cell.deps = deps
      if (changed) cell.value = fn
      return cell.value
    },
  }

  function expand(node) {
    if (node === null || node === undefined || typeof node !== 'object') return node
    if (Array.isArray(node)) return node.map(expand)
    if (typeof node.type === 'function') return expand(node.type(node.props))
    return { type: node.type, props: node.props, children: node.children.map(expand) }
  }

  function render(component) {
    cursor = 0
    effectQueue = []
    const element = component()
    const tree = expand(element)
    for (const { index, fn } of effectQueue) {
      const cell = cells[index]
      if (typeof cell.cleanup === 'function') cell.cleanup()
      const cleanup = fn()
      cell.cleanup = typeof cleanup === 'function' ? cleanup : undefined
    }
    dirty = false
    lastTree = tree
    return tree
  }

  async function settle(component) {
    let tree = render(component)
    for (let round = 0; round < 20; round += 1) {
      await new Promise(resolve => setTimeout(resolve, 0))
      if (!dirty) break
      tree = render(component)
    }
    return tree
  }

  return { React, render, settle, dirty: () => dirty, last: () => lastTree }
}

function textOf(node) {
  if (node === null || node === undefined) return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join(' ')
  if (typeof node !== 'object') return ''
  return textOf(node.children)
}

/** The polite live region's text: what a screen reader is told. */
function liveText(tree) {
  return collectNodes(tree, node => node.props?.role === 'status' && node.props?.['aria-live'] === 'polite').map(textOf).join(' ')
}

function collectButtons(node, found = []) {
  if (node === null || node === undefined || typeof node !== 'object') return found
  if (Array.isArray(node)) {
    for (const child of node) collectButtons(child, found)
    return found
  }
  if (node.type === 'button') found.push(node)
  collectButtons(node.children, found)
  return found
}

function buttonByText(tree, label) {
  return collectButtons(tree).find(button => textOf(button).trim() === label)
}

function collectNodes(node, predicate, found = []) {
  if (node === null || node === undefined || typeof node !== 'object') return found
  if (Array.isArray(node)) {
    for (const child of node) collectNodes(child, predicate, found)
    return found
  }
  if (predicate(node)) found.push(node)
  collectNodes(node.children, predicate, found)
  return found
}

function createDocumentStub() {
  const listeners = []
  return {
    baseURI: 'http://plato:3080/',
    documentElement: { lang: 'en' },
    visibilityState: 'visible',
    head: { appendChild() {} },
    createElement() { return { dataset: {}, textContent: '', remove() {} } },
    addEventListener(type) { listeners.push(type) },
    removeEventListener() {},
    listeners,
  }
}

function applyContext(module, registrations, services = {}) {
  const ctx = {
    effect(fn) { fn() },
    get(name) { return services[name] },
    locale: {
      register(name, dictionaries) {
        registrations.push({ name, dictionaries })
        return () => {}
      },
      bind() { return t },
      subscribe() { return () => {} },
    },
    slots: {
      inject(name, fn) { fn() },
      register(options, component) {
        registrations.push({ options, component })
        return () => {}
      },
    },
  }
  module.apply(ctx)
}

/** One fake page runtime: DOM, fetch, module and slot registrations. */
async function mountSurface(respond, options = {}) {
  const runtime = createRuntime()
  const module = await moduleWith(runtime.React)
  const documentStub = createDocumentStub()
  const calls = []
  const registrations = []
  const previousDocument = globalThis.document
  const previousFetch = globalThis.fetch
  globalThis.document = documentStub
  globalThis.fetch = async (url, init = {}) => {
    const call = { url: String(url), method: init.method ?? 'GET', body: init.body }
    calls.push(call)
    const result = await respond(call)
    return { ok: result.status >= 200 && result.status < 300, status: result.status, json: async () => result.body }
  }
  applyContext(module, registrations, options.services ?? {})
  return {
    runtime,
    module,
    documentStub,
    calls,
    registrations,
    restore() {
      if (previousDocument === undefined) delete globalThis.document
      else globalThis.document = previousDocument
      if (previousFetch === undefined) delete globalThis.fetch
      else globalThis.fetch = previousFetch
    },
  }
}

/**
 * One page instance over one fake fetch. `respond` receives
 * `{ url, method, body }` and returns `{ status, body }`.
 */
async function mountPage(respond) {
  const surface = await mountSurface(respond)
  const page = surface.registrations.find(entry => entry.options?.name === 'main')
  assert.ok(page, 'the page slot is registered')
  const component = () => page.component({})
  return {
    calls: surface.calls,
    documentStub: surface.documentStub,
    async mount() { return surface.runtime.settle(component) },
    async click(label) {
      const button = buttonByText(surface.runtime.last(), label)
      assert.ok(button, `a "${label}" button is rendered`)
      assert.notEqual(button.props.disabled, true, `the "${label}" button is enabled`)
      button.props.onClick()
      return surface.runtime.settle(component)
    },
    async settle() { return surface.runtime.settle(component) },
    restore: surface.restore,
  }
}

/**
 * One Phase 3 seat over one fake fetch. `selector` picks the slot registration;
 * `props` are the owner/runtime props the DSH renderer would supply.
 */
async function mountSeat(respond, selector, props = {}, options = {}) {
  const surface = await mountSurface(respond, options)
  const entry = surface.registrations.find(selector)
  assert.ok(entry, 'the seat is registered on its slot')
  const component = () => entry.component({ t, ...props })
  return {
    calls: surface.calls,
    registrations: surface.registrations,
    mount() { return surface.runtime.settle(component) },
    settle() { return surface.runtime.settle(component) },
    current() { return surface.runtime.last() },
    async click(label) {
      const button = buttonByText(surface.runtime.last(), label)
      assert.ok(button, `a "${label}" button is rendered`)
      assert.notEqual(button.props.disabled, true, `the "${label}" button is enabled`)
      button.props.onClick()
      return surface.runtime.settle(component)
    },
    restore: surface.restore,
  }
}

const slot = name => entry => entry.options?.name === name

function pathOf(call) {
  return new URL(call.url).pathname
}

function reply(record) {
  const url = new URL(record.url)
  if (url.pathname === '/_tmax/state') return { status: 200, body: record.state }
  return { status: 404, body: { ok: false, code: 'not_found', message: 'no route' } }
}

function withState(state) {
  return async record => reply({ ...record, state })
}

/** A small, complete host read model for the Phase 3 seat tests. */
function seatState(overrides) {
  const base = {
    ok: true,
    errors: [],
    schemaVersion: 2,
    generatedAt: '2026-11-23T10:15:00Z',
    configurationRevision: 'abc123def4567890abc123def4567890abc123def4567890',
    tmaxVersion: '0.1.0',
    authority: '/home/user/.tmax',
    target: { id: 'dsh-main', path: '/home/user/.dsh', blockers: [] },
    ownership: {
      owner: 'home',
      scope: 'home_wide',
      reason: null,
      homeIntegration: { present: true, entry: true, artifact: true },
      conflict: null,
      mode: 'auto',
    },
    gateway: {
      configured: true,
      ready: true,
      listen: '127.0.0.1:17840',
      daemonDiscoveryState: 'identified',
      reasonCode: null,
      supervision: { state: 'available', reasonCode: null },
      autostart: { savedIntent: true, registration: 'registered', unitMatches: true },
      sharedKey: { loadedState: 'loaded', reasonCode: null },
    },
    config: { state: 'valid', revision: 'abc123def4567890abc123def4567890abc123def4567890', errors: [] },
    routes: [{
      name: 'tmax',
      provider: 'tmax',
      protocol: 'responses',
      baseUrl: 'http://127.0.0.1:17840',
      credentialRef: 'TMAX_REF',
      credentialConfigured: true,
      scope: 'home_wide',
      active: true,
      blockers: [],
      models: [{
        id: 'deepseek-v4.1-flash',
        name: 'deepseek-v4.1-flash',
        kind: 'alias',
        continuity: 'stateless',
        candidateCount: 3,
        availability: 'unverified',
        capabilities: { contextWindow: 500000, input: ['text', 'image'], reasoningEfforts: { max: 'max' } },
      }],
    }],
    providers: { tmax: [] },
    routing: [{
      model: 'deepseek-v4.1-flash',
      protocol: 'responses',
      kind: 'alias',
      continuity: 'stateless',
      candidateCount: 3,
      availability: 'unverified',
      selection: { qualifiedModel: 'opencode-go-2/deepseek-v4.1-flash', provider: 'opencode-go-2', model: 'deepseek-v4.1-flash', priority: 1 },
      candidates: [{ qualifiedModel: 'opencode-go-2/deepseek-v4.1-flash', priority: 1, selected: true }],
      excluded: [],
      error: null,
    }],
    diagnostics: { history: { state: 'unavailable', reasonCode: 'daemon_identity_unavailable' }, lastProjectionAt: '2026-11-23T10:14:31Z', lastError: null },
  }
  return overrides === undefined ? base : { ...base, ...overrides }
}

/** The connected fixture with per-provider evidence replaced by id. */
function withProviderEvidence(changes) {
  return {
    ...connected,
    providers: {
      ...connected.providers,
      tmax: connected.providers.tmax.map(provider => changes[provider.id] === undefined
        ? provider
        : { ...provider, ...changes[provider.id] }),
    },
  }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

test('client registers one first-level page and its locale namespace', async () => {
  const module = await moduleWith(inertReact())
  assert.deepEqual(module.inject, ['slots', 'locale'])
  assert.equal(typeof module.apply, 'function')
  assert.equal(typeof module.__pure, 'object')

  const registrations = []
  applyContext(module, registrations)

  const locale = registrations.find(entry => entry.name !== undefined)
  assert.equal(locale.name, STATE_NS)
  assert.equal(typeof locale.dictionaries.en, 'object')
  assert.equal(typeof locale.dictionaries.zh, 'object')

  const slots = registrations.filter(entry => entry.options !== undefined)
  assert.deepEqual(slots.map(entry => entry.options.name), [
    'sidebar.panellist',
    'main',
    'sidebar.panellist.trailing',
    'settings.models.footer',
    'tool.call.toolview',
  ])
  const byName = name => slots.find(entry => entry.options.name === name)
  assert.equal(byName('sidebar.panellist').options.id, 'tmax')
  assert.equal(byName('sidebar.panellist').options.order, 20)
  assert.equal(byName('sidebar.panellist').options.locale, STATE_NS)
  assert.equal(byName('sidebar.panellist').options.label(), 'panel')
  assert.equal(byName('main').options.key, 'tmax')
  // Seats: one per DSH surface, addressed by the page's own key or id.
  const marker = byName('sidebar.panellist.trailing')
  assert.equal(marker.options.id, 'tmax')
  assert.equal(marker.options.order, 20)
  assert.equal(marker.options.locale, STATE_NS)
  const footer = byName('settings.models.footer')
  assert.equal(footer.options.id, 'tmax')
  assert.equal(footer.options.order, 100)
  assert.equal(footer.options.locale, STATE_NS)
  const toolview = byName('tool.call.toolview')
  assert.equal(toolview.options.key, 'tmax_status')
  assert.equal(toolview.options.locale, STATE_NS)
  for (const entry of slots) assert.equal(typeof entry.component, 'function', `${entry.options.name} renders`)
})

test('the sidebar glyph is decorative for assistive technology', async () => {
  const runtime = createRuntime()
  const module = await moduleWith(runtime.React)
  const registrations = []
  applyContext(module, registrations)
  const icon = registrations.find(entry => entry.options?.name === 'sidebar.panellist').component
  const glyph = icon({ size: 16 })
  assert.equal(glyph.type, 'svg')
  assert.equal(glyph.props['aria-hidden'], 'true')
  assert.equal(glyph.props.width, 16)
})

// ---------------------------------------------------------------------------
// Pure truth rules
// ---------------------------------------------------------------------------

test('a window reading states the share of the quota it has used', async () => {
  const { windowReading } = (await moduleWith(inertReact())).__pure

  const consumed = windowReading({ label: { key: 'weekly_allowance' }, basis: 'consumed_percent', percent: 37 }, 0, t)
  assert.equal(consumed.name, 'windowWeekly')
  assert.equal(consumed.used, 37)
  assert.equal(consumed.value, '37%')
  assert.equal(consumed.quota, '100%', 'a percentage is a share of its own 100% limit')
  assert.equal(consumed.spoken, 'usedShare 37% / 100%')

  const remainingPercent = windowReading({ label: { key: 'credit' }, basis: 'remaining_percent', percent: 80 }, 0, t)
  assert.equal(remainingPercent.used, 20)
  assert.equal(remainingPercent.value, '20%')
  assert.equal(remainingPercent.spoken, 'usedShare 20% / 100%')

  // Counter values arrive as strings. A counter nothing has touched is a
  // measured zero used: an empty bar, printed as 0 of its limit.
  const untouched = windowReading({ label: { key: 'weekly_allowance' }, basis: 'counters', counters: { limit: '100', remaining: '100' } }, 0, t)
  assert.equal(untouched.used, 0)
  assert.equal(untouched.value, '0')
  assert.equal(untouched.quota, '100')
  assert.equal(untouched.spoken, 'usedShare 0 / 100')

  const half = windowReading({ basis: 'counters', counters: { total: 10, remaining: 4 } }, 0, t)
  assert.equal(half.used, 60)
  assert.equal(half.value, '6')
  assert.equal(half.quota, '10')

  // A reported used amount is the authority; subtracting from what is left is
  // the fallback for accounts that never report it.
  const reported = windowReading({ basis: 'counters', counters: { total: '100', remaining: '94', used: '6' } }, 0, t)
  assert.equal(reported.used, 6)
  assert.equal(reported.value, '6')

  // Counters are independent fields: a used amount without what is left still
  // measures the share, and is not read as unreadable.
  const usedOnly = windowReading({ basis: 'counters', counters: { limit: '100', used: '6' } }, 0, t)
  assert.equal(usedOnly.unreadable, undefined)
  assert.equal(usedOnly.used, 6)
  assert.equal(usedOnly.value, '6')
  assert.equal(usedOnly.quota, '100')

  // An account that reports running usage without a limit has no quota to
  // print: the percentage is the reading, and no total is invented.
  const usageOnly = windowReading({ basis: 'consumed_percent', percent: 0, counters: { currentValue: 0, remaining: 4000, usage: 4000 } }, 0, t)
  assert.equal(usageOnly.value, '0%')
  assert.equal(usageOnly.quota, '100%')

  // Counters are a share only on the bases that define them; any other basis is text.
  const foreignBasis = windowReading({ basis: 'unknown_basis', counters: { total: 50, remaining: 100 } }, 0, t)
  assert.equal(foreignBasis.unreadable, true)
  assert.equal(foreignBasis.used, undefined)

  // No used amount and no remaining, a zero total, and a row without a basis
  // are each not a computable share: the guard is the counter, not the basis.
  for (const row of [
    { basis: 'counters', label: { key: 'weekly_allowance' }, counters: { limit: '100' } },
    { basis: 'counters', counters: { limit: 0, remaining: 0 } },
    { basis: 'counters', label: { key: 'weekly_allowance' }, counters: { total: 0, used: '0' } },
    { label: { key: 'weekly_allowance' } },
  ]) {
    const reading = windowReading(row, 0, t)
    assert.equal(reading.used, undefined)
    assert.equal(reading.unreadable, true)
    assert.equal(reading.text, 'quotaUnreadable')
  }

  const duration = windowReading({ label: { key: 'duration', duration: 300, unit: 'TIME_UNIT_MINUTE' }, counters: { limit: 100, remaining: 100 } }, 0, t)
  assert.equal(duration.name, '300 unitMinute')

  const unnamed = [windowReading({ counters: { total: 10, remaining: 5 } }, 0, t), windowReading({ counters: { total: 20, remaining: 5 } }, 1, t)]
  const split = (await moduleWith(inertReact())).__pure.disambiguate(unnamed)
  assert.deepEqual(split.map(item => item.name), ['windowGeneric 1', 'windowGeneric 2'])
})

test('host-derived names are bounded before they reach the DOM', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const long = 'x'.repeat(500)
  // The tool card is built from tool text: a hostile target id/path is clipped.
  const summary = pure.toolStatusSummary([{
    type: 'text',
    text: JSON.stringify({ ok: true, target: { id: long, path: long }, routes: [] }),
  }])
  assert.equal(summary.targetId.length, 81)
  assert.ok(summary.targetId.endsWith('…'))
  assert.equal(summary.targetPath.length, 161)
  assert.ok(summary.targetPath.endsWith('…'))

  // Account and model names are host-owned too; the page clips them.
  const route = seatState().routes[0]
  const state = seatState({
    routes: [{ ...route, models: [{ name: long }] }],
    providers: { tmax: [{ id: long, billing: { state: 'observed', rows: [{ percent: 40, basis: 'remaining_percent', label: { key: 'weekly' } }] } }] },
  })
  const page = await mountPage(withState(state))
  try {
    const text = textOf(await page.mount())
    assert.equal(text.includes(long), false, 'no unbounded host name reaches the DOM')
    assert.match(text, /x{120}…/, 'the page draws the bounded prefix')
  } finally {
    page.restore()
  }
})

test('section read failures are typed, bounded and routed to their section', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const state = {
    errors: [
      { section: 'providers', code: 'provider_list_unavailable', message: 'tmax provider list failed' },
      { section: 'gateway', code: 'gateway_unavailable', message: 'gateway status failed' },
      { section: 'projection', code: 'projection_unavailable', message: 'projection failed' },
      { section: 'agent-settings', code: 'agent_settings_unavailable', message: 'agent settings unavailable' },
      { section: 'mystery', code: 'weird', message: 'unknown section' },
      'plain failure',
      { section: 'target', code: 'agent_inspect_unavailable' },
      { section: 'providerList', code: 'camel_case_section', message: 'camel case section' },
    ],
  }
  const all = pure.sectionErrors(state)
  assert.deepEqual(all.map(error => error.section),
    ['providers', 'gateway', 'projection', 'agent_settings', 'mystery', 'unknown', 'target', 'provider_list'])
  assert.equal(all[0].text, 'provider_list_unavailable — tmax provider list failed')
  assert.equal(all[6].text, 'agent_inspect_unavailable')
  assert.deepEqual(pure.errorsForSection(state, 'providers').map(error => error.code),
    ['provider_list_unavailable', 'camel_case_section'])
  assert.deepEqual(pure.errorsForSection(state, 'target').map(error => error.code), ['agent_inspect_unavailable'])
  assert.deepEqual(pure.sectionErrors({}), [])
  assert.deepEqual(pure.errorsForSection({}, 'projection'), [])
  assert.equal(pure.sectionErrorText({ code: 'x', message: 'y' }), 'x — y')
})

test('projection failures are recognized in both host section spellings', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const page = { owner: 'page', homeIntegration: { present: false } }

  // Today's host reports a failed projection read under 'target' with a null target.
  const targetShape = {
    routes: [],
    target: null,
    errors: [{ section: 'target', code: 'projection_unavailable', message: 'projection read failed' }],
  }
  assert.equal(pure.projectionFailed(targetShape), true)
  assert.deepEqual(pure.projectionErrors(targetShape).map(error => error.code), ['projection_unavailable'])
  // An agent-inspect failure keeps a target object; it is not a projection failure.
  assert.equal(pure.projectionFailed({
    routes: [],
    target: { id: 'dsh-main' },
    errors: [{ section: 'target', code: 'agent_inspect_unavailable', message: 'agent inspect failed' }],
  }), false)
  // Routes that exist are projection evidence even if another target read failed.
  assert.equal(pure.projectionFailed({
    routes: [{ active: false }],
    target: null,
    errors: [{ section: 'target', code: 'x', message: 'y' }],
  }), false)
  // The explicit 'projection' section is recognized even when a target object
  // exists (the host may emit it directly in a future shape).
  assert.equal(pure.projectionFailed({
    routes: [],
    target: { id: 'dsh-main' },
    errors: [{ section: 'projection', code: 'projection_read_failed', message: 'x' }],
  }), true)
  const explicit = {
    routes: [],
    target: null,
    errors: [{ section: 'projection', code: 'projection_read_failed', message: 'x' }],
  }
  assert.equal(pure.projectionFailed(explicit), true)
  assert.deepEqual(pure.projectionErrors(explicit).map(error => error.code), ['projection_read_failed'])

  // Verdict and next action agree on the host shape.
  assert.deepEqual(pure.verdict({ phase: 'ready', value: { ok: true, ...targetShape, ownership: page } }, t),
    { tone: 'error', text: 'stateProjection' })
  assert.deepEqual(pure.nextAction({ phase: 'ready', value: { ok: true, ...targetShape, ownership: page, config: { state: 'valid' } } }),
    { kind: 'retry', reason: 'projection' })
})


test('accounts are ordered by what they used, and a window prints usage of its quota', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const t = key => key
  const row = (basis, extra = {}) => ({ basis, label: { key: 'weekly' }, reset: { at: '2026-11-30T00:00:00Z', kind: 'announced' }, ...extra })
  const provider = (id, rows) => ({ id, billing: { state: 'observed', rows } })
  const state = {
    providers: { tmax: [
      provider('consumed', [row('consumed_percent', { percent: 96 })]),
      provider('remaining', [row('remaining_percent', { percent: 25 })]),
      provider('counters', [row('counters', { counters: { limit: '100', remaining: '40' } })]),
      provider('credit', [row('credit', { counters: { total: '8000000000', remaining: '7712000000' }, label: { key: 'credit', kind: 'subscription' } })]),
      provider('unmeasurable', [row('unknown_basis', { percent: 50 })]),
      provider('full', [row('remaining_percent', { percent: 100 })]),
      provider('no-window', []),
    ], agent: [] },
  }
  const accounts = pure.accountsOf(state, t)
  // An account without a window takes no card; the rest are ordered by what
  // they have used, and an account nothing can measure follows them.
  assert.deepEqual(accounts.map(account => account.provider.id), [
    'consumed', 'remaining', 'counters', 'credit', 'full', 'unmeasurable',
  ])
  const reading = id => accounts.find(account => account.provider.id === id).readings[0]
  assert.equal(reading('consumed').used, 96)
  assert.equal(reading('remaining').used, 75, 'a remaining share becomes what is used')
  assert.equal(reading('counters').used, 60, 'counters become a share of the limit')
  assert.equal(reading('credit').used, 4, 'credit becomes a whole-percent share of the total')
  assert.equal(reading('full').used, 0, 'nothing used is a measured zero, not an unknown')
  assert.equal(reading('unmeasurable').used, undefined, 'a basis nobody computes is text, never a bar')
  assert.equal(accounts.find(account => account.provider.id === 'unmeasurable').mostUsed, undefined)
  // Tones: ninety percent is nearly gone, seventy-five is running low.
  assert.equal(pure.windowTone(90), 'error')
  assert.equal(pure.windowTone(89), 'warn')
  assert.equal(pure.windowTone(75), 'warn')
  assert.equal(pure.windowTone(74), 'ok')
  assert.equal(pure.windowTone(undefined), '')
  // A quantity window prints what it used beside the quota it used it from.
  assert.equal(reading('counters').value, '60')
  assert.equal(reading('counters').quota, '100')
  assert.equal(reading('consumed').quota, '100%', 'a percentage prints the 100% limit it is a share of')
})

test('unqualified reset times are marked, never dated', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const unqualified = { kind: 'unqualified_epoch_millis', raw: 1795073471000, encoding: 'epoch_milliseconds_unqualified' }
  assert.equal(pure.resetQualified(unqualified), false)
  assert.equal(pure.resetUnqualified(unqualified), true)
  assert.equal(pure.resetAt(unqualified), undefined)

  const qualified = { at: '2026-11-26T15:42:16Z', kind: 'provider_timestamp' }
  assert.equal(pure.resetQualified(qualified), true)
  assert.equal(pure.resetUnqualified(qualified), false)
  assert.ok(pure.resetAt(qualified) instanceof Date)

  const unqualifiedReading = pure.windowReading({
    label: { key: 'weekly_allowance' },
    counters: { limit: 100, remaining: 100 },
    reset: unqualified,
  }, 0, t)
  assert.equal(unqualifiedReading.resetUnqualified, true)
  assert.equal(unqualifiedReading.reset, undefined)

  const qualifiedReading = pure.windowReading({
    label: { key: 'weekly_allowance' },
    counters: { limit: 100, remaining: 100 },
    reset: qualified,
  }, 0, t)
  assert.equal(qualifiedReading.resetUnqualified, false)
  assert.ok(qualifiedReading.reset instanceof Date)
  // The fixture's tmax row is the qualified case.
  assert.equal(pure.windowReading(connected.providers.tmax[0].billing.rows[0], 0, t).resetUnqualified, false)
})

test('verdict answers reachability, never a catalog group name', async () => {
  const { verdict } = (await moduleWith(inertReact())).__pure
  const route = { provider: 'renamed-tmax', credentialConfigured: true, active: true }
  const applied = { owner: 'settings', scope: 'profile_scoped' }
  const unapplied = { owner: 'unapplied', reason: 'apply_not_run' }

  assert.deepEqual(verdict({ phase: 'loading' }, t), { tone: 'idle', text: 'checking' })
  assert.deepEqual(verdict({ phase: 'failed' }, t), { tone: 'error', text: 'stateUnreadable' })
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [] } }, t), { tone: 'idle', text: 'stateDisconnected' })

  // Declared but not live: DSH has not loaded the applied settings yet.
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [{ ...route, active: false }], ownership: applied } }, t),
    { tone: 'idle', text: 'stateReload' })
  // Declared but never applied: applying is the missing step.
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [{ ...route, active: false }], ownership: unapplied } }, t),
    { tone: 'warn', text: 'stateNotApplied' })

  // A live route with a known-missing credential is registered, not usable.
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [{ ...route, credentialConfigured: false }], gateway: {} } }, t),
    { tone: 'warn', text: 'stateNotUsable' })
  // Readiness that was never reported stays unknown.
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [{ ...route, credentialConfigured: null }], gateway: {} } }, t),
    { tone: 'idle', text: 'stateUnknown' })
  // Liveness that was never reported cannot be called live; the reload hint is
  // the only honest reading.
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [{ ...route, active: null }] } }, t),
    { tone: 'idle', text: 'stateReload' })

  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [route], gateway: { ready: true } } }, t),
    { tone: 'ok', text: 'stateReady' })
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [route], gateway: { ready: false } } }, t),
    { tone: 'error', text: 'stateGateway' })
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [route], gateway: { ready: null } } }, t),
    { tone: 'idle', text: 'stateUnknown' })

  // A failed projection read cannot support a "not connected" claim.
  assert.deepEqual(verdict({ phase: 'ready', value: { ok: true, routes: [], errors: [{ section: 'projection', code: 'x', message: 'y' }], ownership: applied } }, t),
    { tone: 'error', text: 'stateProjection' })
  // An unsupported schema is a visible failure.
  assert.deepEqual(verdict({ phase: 'failed', schemaUnsupported: true }, t),
    { tone: 'error', text: 'stateSchemaUnsupported' })
  // A failed read with a last-good state says "last known".
  assert.deepEqual(verdict({ phase: 'failed', value: { ok: true, routes: [route], gateway: { ready: true } } }, t),
    { tone: 'ok', text: 'stateLastKnown: stateReady' })
})

test('next action is the one step that can change the reported state', async () => {
  const { nextAction } = (await moduleWith(inertReact())).__pure
  const applied = { owner: 'settings', scope: 'profile_scoped' }
  const unapplied = { owner: 'unapplied', reason: 'apply_not_run' }
  const live = { provider: 'tmax', credentialConfigured: true, active: true }
  const inactive = { provider: 'tmax', credentialConfigured: true, active: false }
  const unconfigured = { provider: 'tmax', credentialConfigured: false, active: false }

  // A failed read never offers a write, even with a last-known good state.
  assert.deepEqual(nextAction({ phase: 'failed' }), { kind: 'retry' })
  assert.deepEqual(
    nextAction({ phase: 'failed', value: { ok: true, routes: [], ownership: applied } }),
    { kind: 'retry' })

  // Nothing declared: applying is the step that gives DSH something.
  assert.deepEqual(nextAction({ phase: 'ready', value: { ok: true, routes: [] } }), { kind: 'connect' })
  // Declared but not loaded by DSH: the reload is the non-writing step.
  assert.deepEqual(
    nextAction({ phase: 'ready', value: { ok: true, routes: [inactive], ownership: applied } }),
    { kind: 'reload' })
  assert.deepEqual(
    nextAction({ phase: 'ready', value: { ok: true, routes: [{ ...inactive, credentialConfigured: false }], ownership: unapplied } }),
    { kind: 'connect' })
  // A conflict is surfaced instead of Apply.
  assert.deepEqual(
    nextAction({ phase: 'ready', value: { ok: true, routes: [unconfigured], ownership: { ...applied, conflict: { code: 'route_owner_conflict', message: 'taken' } } } }),
    { kind: 'conflict', code: 'route_owner_conflict', message: 'taken' })
  // Apply repairs a credential only when one is known to be missing, and never
  // for a route that is already live.
  assert.deepEqual(
    nextAction({ phase: 'ready', value: { ok: true, routes: [unconfigured], ownership: applied } }),
    { kind: 'connect' })
  assert.deepEqual(
    nextAction({ phase: 'ready', value: { ok: true, routes: [live], ownership: applied } }),
    undefined)
  // An unreported credential is unknown, so no write is offered for it; the
  // non-writing reload is the only step left.
  assert.deepEqual(
    nextAction({ phase: 'ready', value: { ok: true, routes: [{ ...inactive, credentialConfigured: null }], ownership: applied, config: { state: 'valid' } } }),
    { kind: 'reload' })
})

test('apply preview and observe results are bounded host evidence', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const lines = pure.connectLines({
    dryRun: true,
    schema_version: 1,
    target: { id: 'dsh-main', path: '/home/user/.dsh' },
    gateway: { listen: '127.0.0.1:17840', ready: null, reason_code: 'daemon_identity_unavailable' },
    credentials: { path: '/home/user/.dsh/.credentials.yaml', state: 'absent', permissions: null },
    references: [
      { route: 'tmax', protocol: 'responses', credential_ref: 'TMAX_REF', action: 'would_write' },
      { route: 'renamed', protocol: 'responses', credential_ref: 'OTHER_REF', action: 'unchanged' },
    ],
    changed: null,
    effects: { native_writes: false, secret_reads: false, processes_started: false, locks_created: false },
    message: 'preview only',
  }, t)
  assert.deepEqual(lines.map(line => line.label),
    ['target', 'gatewayTitle', 'credentialsTitle', 'connectRefs', 'connectEffects', 'connectDetail'])
  assert.deepEqual(lines[0].items, ['target: dsh-main', 'targetPath: /home/user/.dsh'])
  assert.deepEqual(lines[1].items, [
    'gatewayListen: 127.0.0.1:17840',
    'gatewayReady: unknown',
    'gatewayReason: reasonDaemonIdentity',
  ])
  assert.deepEqual(lines[2].items, [
    'credentialPath: /home/user/.dsh/.credentials.yaml',
    'credentialState: absent',
  ])
  assert.deepEqual(lines[3].items, [
    'tmax · responses · TMAX_REF · actionWouldWrite',
    'renamed · responses · OTHER_REF · actionUnchanged',
  ])
  assert.deepEqual(lines[4].items, [
    'effectNativeWrites: no',
    'effectSecretReads: no',
    'effectProcessesStarted: no',
    'effectLocksCreated: no',
  ])
  assert.deepEqual(lines[5].items, ['preview only'])

  const blocked = pure.connectLines({ changed: true, blockers: [{ message: 'not usable' }], next_actions: ['reload DSH'] }, t)
  assert.deepEqual(blocked.map(line => line.label), ['connectChanged', 'connectBlockers', 'connectNextActions'])
  assert.deepEqual(blocked[0].items, ['yes'])
  assert.deepEqual(blocked[1].items, ['not usable'])

  // `tmax agent apply` data: applied state, planned/changed/unchanged files,
  // recovery, and fact gaps. Paths and states are disclosed; file contents and
  // secrets never are.
  const apply = pure.connectLines({
    target: 'dsh-main',
    state: 'applied_reload_pending',
    files: [{ path: '/home/user/.dsh/cordis.patch.yml', format: 'yaml', after: 'SECRET-CONTENT' }],
    changed_files: ['/home/user/.dsh/cordis.patch.yml'],
    unchanged_files: ['/home/user/.dsh/tmax-dsh.mjs'],
    recovery: { planned: 'exact native bytes in current-user-only files', retained: 'rec_1' },
    fact_gaps: [{ entry: 'tmax', model: 'deepseek-v4.1-flash', missing: ['reasoning_efforts'] }],
    effects: { native_writes: true, secret_reads: false },
  }, t)
  assert.deepEqual(apply.map(line => line.label),
    ['target', 'connectState', 'connectFiles', 'connectChangedFiles', 'connectUnchangedFiles',
      'connectRecovery', 'connectFactGaps', 'connectEffects'])
  assert.deepEqual(apply[0].items, ['dsh-main'])
  assert.deepEqual(apply[1].items, ['applied_reload_pending'])
  assert.deepEqual(apply[2].items, ['/home/user/.dsh/cordis.patch.yml: yaml'])
  assert.deepEqual(apply[3].items, ['/home/user/.dsh/cordis.patch.yml'])
  assert.deepEqual(apply[4].items, ['/home/user/.dsh/tmax-dsh.mjs'])
  assert.deepEqual(apply[5].items, [
    'recoveryPlanned: exact native bytes in current-user-only files',
    'recoveryRetained: rec_1',
  ])
  assert.deepEqual(apply[6].items, ['tmax/deepseek-v4.1-flash: reasoning_efforts'])
  assert.deepEqual(apply[7].items, ['effectNativeWrites: yes', 'effectSecretReads: no'])
  assert.equal(JSON.stringify(apply).includes('SECRET-CONTENT'), false, 'file contents never cross')

  assert.equal(pure.connectLines(undefined, t).length, 0)
  const longMessage = pure.connectLines({ message: 'x'.repeat(400) }, t)
  assert.equal(longMessage[0].items[0].length, 201)
  assert.equal(longMessage[0].items[0].endsWith('…'), true)

  const items = pure.observeItems({
    items: [
      { source: 'tmax', id: 'kimi', status: 'refreshed' },
      { source: 'agent', id: 'glm', status: 'failed', code: 'upstream_rejection', message: 'refused' },
      { source: 'tmax', id: 'cpa', status: 'blocked', code: 'unsupported_source' },
      { source: 'agent', id: 'proxy', status: 'unchanged', code: 'dry_run' },
      'not an item',
    ],
  })
  assert.deepEqual(items.map(item => item.tone), ['ok', 'error', 'warn', 'idle', 'idle'])
  assert.equal(items[1].detail, 'upstream_rejection: refused')
  assert.equal(items[2].detail, 'unsupported_source')
  assert.equal(pure.observeItems(undefined).length, 0)
  const many = pure.observeItems({ items: Array.from({ length: 60 }, (_, index) => ({ source: 'tmax', id: `p${index}`, status: 'refreshed' })) })
  assert.equal(many.length, 50)
})

test('dates and counters format in the interface language', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  assert.equal(pure.clockOf('not a date'), '')
  assert.equal(pure.clockOf(undefined), '')
  assert.match(pure.clockOf('2026-11-23T10:15:00Z', 'en-US'), /\d/)
  assert.equal(pure.formatReset(new Date('invalid'), t), '')
  // A reset kept today or tomorrow states its clock; a later day states only
  // the day; the year appears only when it is not this one.
  const now = new Date(2026, 10, 25, 9, 0, 0).getTime()
  assert.match(pure.formatReset(new Date(2026, 10, 25, 16, 21), t, now), /^today \d/)
  assert.match(pure.formatReset(new Date(2026, 10, 26, 8, 0), t, now), /^tomorrow \d/)
  const later = pure.formatReset(new Date(2026, 11, 9, 11, 13), t, now)
  assert.doesNotMatch(later, /2026/)
  assert.match(later, /9/)
  assert.match(pure.formatReset(new Date(2027, 0, 5, 0, 0), t, now), /2027/)
  // A reset that already passed this year keeps the same short date: the year
  // marks a different year, not a direction in time.
  assert.doesNotMatch(pure.formatReset(new Date(2026, 10, 20, 8, 0), t, now), /2026/)
  assert.equal(pure.compact(0), '0')
  assert.equal(pure.compact(100), '100')
})

// ---------------------------------------------------------------------------
// Locale and source hygiene
// ---------------------------------------------------------------------------

function localeLiteral(name) {
  const start = source.indexOf(`    const ${name} = {`)
  assert.notEqual(start, -1, `${name} locale literal exists`)
  const open = source.indexOf('{', start)
  let depth = 0
  let close = -1
  for (let at = open; at < source.length; at += 1) {
    if (source[at] === '{') depth += 1
    else if (source[at] === '}') {
      depth -= 1
      if (depth === 0) { close = at; break }
    }
  }
  assert.notEqual(close, -1, `${name} locale literal closes`)
  const body = source.slice(open + 1, close)
  const keys = []
  const pattern = /^\s{6}([A-Za-z0-9_]+):/gm
  let match
  while ((match = pattern.exec(body)) !== null) keys.push(match[1])
  return { keys, body }
}

test('every key reached through a table or a pair exists in both dictionaries', async () => {
  const module = await moduleWith(inertReact())
  const dictionaries = module.__pure.locales
  // Keys reached as values of a state table or as the second member of a
  // [field, key] pair never appear in a t('…') literal, so a literal scan misses
  // them and the page would print the raw key.
  // A translation key is camelCase; plain words beside it ('en', 'locale', 'auto')
  // are enum values, not keys.
  const camel = /^([a-z][A-Za-z0-9]*[A-Z][A-Za-z0-9]*)$/
  const reached = new Set()
  for (const match of source.matchAll(/\['[a-z_]+', '([^']+)'\]/g)) {
    if (camel.test(match[1])) reached.add(match[1])
  }
  for (const match of source.matchAll(/^    const [A-Z_]+ = \{[\s\S]*?\}\n/gm)) {
    for (const key of match[0].matchAll(/: '([^']+)'/g)) {
      if (camel.test(key[1])) reached.add(key[1])
    }
  }
  assert.ok(reached.size > 40, `keys reached through tables: ${reached.size}`)
  for (const key of reached) {
    assert.equal(typeof dictionaries.en[key], 'string', `en.${key}`)
    assert.equal(typeof dictionaries.zh[key], 'string', `zh.${key}`)
  }
})

test('English and Chinese locales are complete, unique and colon-correct', async () => {
  const module = await moduleWith(inertReact())
  const { en, zh } = module.__pure.locales
  const enLiteral = localeLiteral('en')
  const zhLiteral = localeLiteral('zh')

  assert.deepEqual(enLiteral.keys.filter((key, at) => enLiteral.keys.indexOf(key) !== at), [])
  assert.deepEqual(zhLiteral.keys.filter((key, at) => zhLiteral.keys.indexOf(key) !== at), [])
  assert.deepEqual([...enLiteral.keys].sort(), Object.keys(en).sort())
  assert.deepEqual(Object.keys(en).sort(), Object.keys(zh).sort())

  for (const key of Object.keys(en)) {
    assert.equal(typeof en[key], 'string', `en.${key} is a string`)
    assert.equal(typeof zh[key], 'string', `zh.${key} is a string`)
    assert.notEqual(en[key], '', `en.${key} is not empty`)
    assert.notEqual(zh[key], '', `zh.${key} is not empty`)
    assert.equal(en[key].includes('：'), false, `en.${key} uses an ASCII colon`)
  }
  assert.equal(en.colon, ':')
  assert.equal(zh.colon, '：')
  // The control names the operation main actually provides: native apply.
  assert.equal(en.connect, 'Apply')
  assert.equal(zh.connect, '应用')
  assert.equal(en.connectApply, 'Confirm apply')
  assert.equal(zh.connectApply, '确认应用')
  assert.equal(en.colon, ':')
  assert.equal(zh.colon, '：')

  const literalKeys = new Set([...source.matchAll(/\bt\('([A-Za-z0-9_]+)'\)/g)].map(match => match[1]))
  const missing = [...literalKeys].filter(key => en[key] === undefined)
  assert.deepEqual(missing, [])
})

test('client reads one state route and carries no auto-observation path', async () => {
  const module = await moduleWith(inertReact())
  const pure = module.__pure

  for (const helper of ['windowReading', 'verdict', 'nextAction', 'safeDashboard', 'clockOf', 'formatReset', 'providerActions', 'actionPayload', 'actionSucceeded', 'actionResultLines', 'markerStatus', 'modelsAvailability', 'stateGate', 'toolStatusSummary', 'accountsOf', 'windowTone', 'accountOfModel']) {
    assert.equal(typeof pure[helper], 'function', `__pure.${helper} is exported`)
  }

  assert.equal(module.inject.includes('remote'), false)
  assert.equal(module.inject.includes('remote.session'), false)
  assert.doesNotMatch(source, /localStorage|sessionStorage/)
  assert.doesNotMatch(source, /setInterval|visibilitychange/)
  assert.doesNotMatch(source, /endsWith\(/)
  assert.doesNotMatch(source, /\bmodelCatalog\b/)
  assert.doesNotMatch(source, /remote\.session/)
  assert.doesNotMatch(source, /ctx\.remote/)

  const endpoints = [...source.matchAll(/\/_tmax\/[a-z-]+/g)].map(match => match[0])
  // The page carries no history reader: retained history is a CLI read.
  assert.deepEqual([...new Set(endpoints)].sort(), ['/_tmax/action', '/_tmax/connect', '/_tmax/observe', '/_tmax/state'])

  const tokens = [...source.matchAll(/var\((--[a-z0-9-]+)/g)].map(match => match[1])
  const foreign = [...new Set(tokens)].filter(token => !token.startsWith('--dsw-alias-') && !token.startsWith('--ds'))
  assert.deepEqual(foreign, [])
})

// ---------------------------------------------------------------------------
// Mount and explicit actions (fake DOM + fake fetch)
// ---------------------------------------------------------------------------

test('an inventory-backed state renders with unknown model metadata', async () => {
  // The host now composes the read model from `agent inspect` / `gateway
  // status`; model kind, continuity and candidate counts are null there. Null
  // is unknown, and a failed `authority` section is a read failure, not a zero.
  const route = seatState().routes[0]
  const state = seatState({
    routes: [{
      ...route,
      models: [{ id: 'm1', name: 'm1', kind: null, continuity: null, candidateCount: null, availability: 'unverified' }],
    }],
    routing: [{
      model: 'm1',
      kind: null,
      continuity: null,
      candidateCount: null,
      availability: 'unverified',
      selection: null,
      candidates: [],
      excluded: [],
      error: null,
    }],
    diagnostics: {
      history: { state: 'unavailable', reasonCode: 'daemon_identity_unavailable' },
      lastProjectionAt: null,
      lastError: null,
      readModel: 'inventory',
    },
    errors: [{ section: 'authority', code: 'authority_unavailable', message: 'tmax status failed' }],
  })
  const page = await mountPage(withState(state))
  try {
    const text = textOf(await page.mount())
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    // The failed enrichment read is visible evidence, bounded and code first,
    // with a retry that reads again rather than observing a provider.
    assert.match(text, /authority_unavailable/)
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    // A model with no routing evidence names itself and claims no account;
    // nothing on the page invents a kind, continuity, or candidate count.
    assert.match(text, /m1/)
    assert.doesNotMatch(text, /modelKind|modelContinuity|modelCandidates/)
  } finally {
    page.restore()
  }
})

test('mount fetches only GET /_tmax/state and never observes a provider', async () => {
  const page = await mountPage(withState(connected))
  try {
    const tree = await page.mount()
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    assert.equal(page.calls.some(call => call.method === 'POST'), false)
    // Opening the page runs no action: repair buttons wait for a click.
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/action').length, 0)
    assert.equal(page.documentStub.listeners.includes('visibilitychange'), false)

    const text = textOf(tree)
    // The verdict comes from the read model; internal identifiers do not reach
    // the page: no revision, no schema number, no read-model source.
    assert.match(text, /stateUnknown/)
    assert.doesNotMatch(text, /352a0e3f/)
    assert.doesNotMatch(text, /schema 1/)
    assert.doesNotMatch(text, /inventory/)
    // An account that reports no quota window takes no card, and nothing on the
    // page explains an unreadable source: it is simply not listed.
    assert.doesNotMatch(text, /cpa-us2/)
    assert.doesNotMatch(text, /unknownQuotaUnsupported/)
    assert.doesNotMatch(text, /GLM Coding CN/)
    // Each bar states to a screen reader what it draws: the window, what it has
    // used, and the same value the eye reads.
    const bars = collectNodes(tree, node => node.props?.role === 'img' && String(node.props.className) === 'tmx-bar')
    assert.ok(bars.length >= 2, 'every reported window has a bar')
    for (const bar of bars) {
      const spoken = String(bar.props['aria-label'])
      assert.match(spoken, /usedShare \d+(.*\/ \d+%?)?$/, spoken)
    }
    // Nothing is used in this fixture: every window is an empty bar printed as
    // zero of its quota, never as an unknown and never as a full bar.
    const fills = collectNodes(tree, node => String(node.props?.className ?? '') === 'tmx-barFill')
    assert.ok(fills.length >= 2)
    for (const fill of fills) assert.equal(fill.props.style.width, '0%')
    assert.match(text, /0\s+\/\s+100/)
    // An exhausted window is a full bar in the error tone, and the same 100% the
    // eye reads beside it.
    const exhausted = withProviderEvidence({ kimi: {} })
    exhausted.providers.tmax[0].billing.rows = [{ label: { key: 'monthly' }, basis: 'consumed_percent', percent: 100, reset: { at: '2026-12-09T03:13:27Z', kind: 'announced' } }]
    const drained = await mountPage(withState(exhausted))
    try {
      const drainedTree = await drained.mount()
      const spent = collectNodes(drainedTree, node => String(node.props?.className ?? '') === 'tmx-barFill')
      assert.equal(spent.length, 1)
      assert.equal(spent[0].props.style.width, '100%')
      assert.equal(spent[0].props['data-tone'], 'error')
      assert.match(textOf(drainedTree), /windowMonthly\s+100%/)
    } finally {
      drained.restore()
    }
    // A model names the account its routing selects, and the default is marked.
    assert.match(text, /deepseek-v4.1-flash/)
    assert.match(text, /opencode-go-2/)
    assert.match(text, /default/)
    // Routing detail is not the page's: no candidates, no priorities.
    assert.doesNotMatch(text, /routingCandidates|routeActive|ownership/)
  } finally {
    page.restore()
  }
})


test('a registered route without a credential is not "connected"', async () => {
  const unusable = {
    ...connected,
    routes: connected.routes.map(route => ({ ...route, credentialConfigured: false })),
    gateway: { ...connected.gateway, ready: true },
    target: { ...connected.target, blockers: [] },
  }
  const page = await mountPage(withState(unusable))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /stateNotUsable/)
    assert.doesNotMatch(text, /stateReady/)
    // Applying is the one thing that repairs a credential tmax knows is
    // missing, so it is the verdict line's own control.
    const apply = buttonByText(tree, 'connect')
    assert.ok(apply, 'Apply sits on the verdict line')
    assert.equal(page.calls.some(call => call.method === 'POST'), false, 'mount runs nothing')
  } finally {
    page.restore()
  }
})

test('declared routes DSH has not loaded say so, name their blockers, and offer no write', async () => {
  const state = {
    ...connected,
    ownership: { owner: 'page', scope: 'profile_scoped', reason: null, conflict: null },
    routes: [{
      ...connected.routes[0],
      active: false,
      blockers: [{ code: 'model_hidden', message: 'glm-5.4 excluded: no eligible candidate' }],
    }],
  }
  const page = await mountPage(withState(state))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /stateReload/)
    assert.match(text, /nextReload/)
    // The host's own route blocker is named beside the route it belongs to.
    assert.match(text, /model_hidden: glm-5.4 excluded: no eligible candidate/)
    assert.equal(buttonByText(tree, 'connect'), undefined)
    assert.equal(page.calls.some(call => call.method === 'POST'), false)
  } finally {
    page.restore()
  }
})


test('an unsupported state schema is a visible failure, not a payload', async () => {
  const page = await mountPage(withState({ ...connected, schemaVersion: 3 }))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /stateSchemaUnsupported/)
    assert.doesNotMatch(text, /Kimi Coding Plan/)
    assert.doesNotMatch(text, /accessTitle/)
    assert.ok(buttonByText(tree, 'retry'))
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
  } finally {
    page.restore()
  }
})

test('section read failures replace empty and disconnected claims', async () => {
  const failed = {
    ...connected,
    // Host shape: a failed projection read is reported under 'target' with a
    // null target; an unavailable settings service is reported under 'providers'.
    errors: [
      { section: 'target', code: 'projection_unavailable', message: 'projection read failed' },
      { section: 'target', code: 'agent_inspect_unavailable', message: 'agent inspect failed' },
      { section: 'providers', code: 'provider_list_unavailable', message: 'provider list failed' },
      { section: 'providers', code: 'agent_settings_unavailable', message: 'agent settings unavailable' },
      { section: 'gateway', code: 'gateway_unavailable', message: 'gateway status failed' },
      { section: 'config', code: 'config_validate_unavailable', message: 'config validate failed' },
    ],
    target: null,
    routes: [],
    routing: [],
    providers: { tmax: [] },
  }
  const page = await mountPage(withState(failed))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /stateProjection/)
    for (const pattern of [
      /projection_unavailable/,
      /agent_inspect_unavailable/,
      /provider_list_unavailable/,
      /agent_settings_unavailable/,
      /gateway_unavailable/,
      /config_validate_unavailable/,
      /readFailed/,
    ]) {
      assert.match(text, pattern)
    }
    // A failed projection/providers read is not "not connected", "no routes",
    // or an empty unknown-quota list.
    assert.doesNotMatch(text, /stateDisconnected/)
    assert.doesNotMatch(text, /noRoutes/)
    assert.doesNotMatch(text, /unknownQuota/)
    assert.doesNotMatch(text, /notReported/)
    // The failed projection offers a retry, and no observation happens.
    assert.ok(buttonByText(tree, 'retry'))
    assert.equal(page.calls.some(call => call.method === 'POST'), false)
  } finally {
    page.restore()
  }
})

test('a section read failure with usable routes offers a non-observing retry', async () => {
  const state = {
    ...connected,
    // The fixture blocker would be a stronger action; this case is only a read.
    target: { ...connected.target, blockers: [] },
    errors: [{ section: 'gateway', code: 'gateway_unavailable', message: 'gateway status failed' }],
  }
  const page = await mountPage(withState(state))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /nextSectionRetry/)
    assert.match(text, /gateway_unavailable/)
    assert.ok(buttonByText(tree, 'retry'))
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])

    // Retry re-reads state; it never posts an observation.
    await page.click('retry')
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`),
      ['GET /_tmax/state', 'GET /_tmax/state'])
    assert.equal(page.calls.some(call => call.method === 'POST'), false)
  } finally {
    page.restore()
  }
})

test('an explicit projection section error is a projection retry', async () => {
  const state = {
    ...connected,
    errors: [{ section: 'projection', code: 'projection_unavailable', message: 'projection failed' }],
    routes: [],
    routing: [],
    target: { ...connected.target, blockers: [] },
  }
  const page = await mountPage(withState(state))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /stateProjection/)
    assert.match(text, /nextProjectionRetry/)
    assert.ok(buttonByText(tree, 'retry'))
    assert.equal(page.calls.some(call => call.method === 'POST'), false)
  } finally {
    page.restore()
  }
})

test('a clean state offers no retry action', async () => {
  const clean = {
    ...connected,
    target: { ...connected.target, blockers: [] },
    gateway: { ...connected.gateway, ready: true },
  }
  const page = await mountPage(withState(clean))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.equal(buttonByText(tree, 'retry'), undefined)
    assert.doesNotMatch(text, /nextSectionRetry/)
    assert.doesNotMatch(text, /nextProjectionRetry/)
    assert.doesNotMatch(text, /nextActionTitle/)
    assert.doesNotMatch(text, /readFailed/)
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
  } finally {
    page.restore()
  }
})

test('a failed provider read does not hide live routes or claim an empty list', async () => {
  const state = {
    ...connected,
    errors: [{ section: 'providers', code: 'provider_list_unavailable', message: 'provider list failed' }],
    providers: { tmax: [] },
  }
  const page = await mountPage(withState(state))
  try {
    const text = textOf(await page.mount())
    assert.match(text, /provider_list_unavailable/)
    assert.match(text, /readFailed/)
    assert.doesNotMatch(text, /unknownQuota/)
    assert.doesNotMatch(text, /notReported/)
    // An empty list the failure caused is never read as "no account", and the
    // route evidence is independent of the provider read.
    assert.doesNotMatch(text, /noAccounts/)
    assert.match(text, /deepseek-v4.1-flash/)
  } finally {
    page.restore()
  }
})

test('a readable empty account list says so instead of reading as a failure', async () => {
  // A readable empty list is a reported zero; it is not an unknown quota and
  // not a section that failed to report.
  const empty = { ...connected, providers: { tmax: [] }, errors: [] }
  const page = await mountPage(withState(empty))
  try {
    const text = textOf(await page.mount())
    assert.match(text, /noAccounts/)
    assert.doesNotMatch(text, /readFailed/)
    assert.doesNotMatch(text, /unknownQuota/)
    assert.doesNotMatch(text, /notReported/)
  } finally {
    page.restore()
  }
})



test('an invalid or unreadable config is the next action before Apply', async () => {
  const ownership = { owner: 'page', scope: 'profile_scoped', reason: null, homeIntegration: { present: false, entry: false, artifact: false }, conflict: null }
  for (const [config, expected] of [
    [{ state: 'invalid', revision: 'abc12345', errors: [{ code: 'bad_schema', message: 'configuration does not match the schema' }] }, [/nextConfig/, /bad_schema/]],
    [{ state: 'unavailable', revision: null, errors: [{ code: 'config_validate_failed', message: 'config validate failed' }] }, [/nextConfigUnavailable/, /config_validate_failed/]],
  ]) {
    const state = { ...connected, routes: [], ownership, config }
    const page = await mountPage(withState(state))
    try {
      const tree = await page.mount()
      const text = textOf(tree)
      for (const pattern of expected) assert.match(text, pattern)
      assert.equal(buttonByText(tree, 'connect'), undefined)
      assert.equal(page.calls.some(call => call.method === 'POST'), false)
    } finally {
      page.restore()
    }
  }
})

test('a host state with absent sections renders unknowns instead of crashing', async () => {
  const minimal = {
    ok: true,
    schemaVersion: 2,
    generatedAt: '2026-11-23T10:15:00Z',
    configurationRevision: 'abc12345',
    routes: [],
    providers: { tmax: [] },
    routing: [],
    target: { id: 'dsh-main' },
    gateway: {},
    config: {},
    diagnostics: {},
  }
  const page = await mountPage(withState(minimal))
  try {
    const text = textOf(await page.mount())
    // Absent sections are absent: the page says what it can and invents nothing.
    assert.match(text, /stateDisconnected/)
    assert.match(text, /noRoutes/)
    assert.match(text, /noAccounts/)
    assert.doesNotMatch(text, /unknown undefined|NaN/)
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
  } finally {
    page.restore()
  }
})

test('Refresh is the only observation path: one explicit POST /_tmax/observe', async () => {
  const updated = { ...connected, configurationRevision: 'bbbbbbbbccccccccddddddddeeeeeeeeffffffff000000001111111122222222' }
  const page = await mountPage(async record => {
    if (pathOf(record) === '/_tmax/state') return { status: 200, body: connected }
    if (pathOf(record) === '/_tmax/observe') {
      // 207: at least one item failed, but the host still composed a fresh state.
      return {
        status: 207,
        body: {
          ok: false,
          generatedAt: '2026-11-23T10:16:00Z',
          dryRun: false,
          items: [{ source: 'tmax', id: 'kimi', status: 'failed', code: 'upstream_rejection', message: 'refused' }],
          state: updated,
        },
      }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    await page.mount()
    assert.equal(page.calls.length, 1)

    const tree = await page.click('refresh')
    assert.equal(page.calls.length, 2)
    assert.equal(page.calls[1].method, 'POST')
    assert.equal(pathOf(page.calls[1]), '/_tmax/observe')
    assert.equal(page.calls[1].body, JSON.stringify({}))

    const text = textOf(tree)
    // The one account the observation could not read is named, code first.
    assert.match(text, /observeFailed:\s+kimi \(upstream_rejection: refused\)/)
    // The fresh snapshot from the observation response is adopted without a GET.
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1)
  } finally {
    page.restore()
  }
})

test('a failed refresh keeps the old table and names the failure', async () => {
  const page = await mountPage(async record => {
    if (pathOf(record) === '/_tmax/state') return { status: 200, body: connected }
    if (pathOf(record) === '/_tmax/observe') {
      return { status: 502, body: { ok: false, code: 'observe_unavailable', message: 'transport failed' } }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    await page.mount()
    const tree = await page.click('refresh')
    const text = textOf(tree)
    assert.match(text, /observeFailed/)
    assert.match(text, /observe_unavailable: transport failed/)
    // A failed observation must not erase a good reading, and the verdict now
    // says the table is last-known rather than current.
    assert.match(text, /kimi/)
    assert.match(text, /stateLastKnown/)
  } finally {
    page.restore()
  }
})

test('an unreadable ownership withholds Apply and offers a read retry', async () => {
  // The settings could not be parsed, so nothing can say whether the routes
  // are applied. Writing on that guess could overwrite a working home.
  const state = {
    ...connected,
    routes: [],
    routing: [],
    target: { ...connected.target, blockers: [] },
    ownership: { owner: 'unknown', scope: null, reason: 'settings_unreadable', conflict: null },
  }
  const page = await mountPage(withState(state))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.equal(buttonByText(tree, 'connect'), undefined, 'no write is offered')
    assert.match(text, /nextSettingsUnreadable/)
    assert.ok(buttonByText(tree, 'retry'), 'a read retry is offered')
    assert.equal(page.calls.some(call => call.method === 'POST'), false)
  } finally {
    page.restore()
  }
})

test('a failed read withdraws Apply and leaves only a read retry', async () => {
  const pageOwned = {
    ...connected,
    routes: [],
    target: { ...connected.target, blockers: [] },
    ownership: {
      owner: 'page', scope: 'profile_scoped', reason: null,
      homeIntegration: { present: false, entry: false, artifact: false }, conflict: null, mode: 'auto',
    },
  }
  const page = await mountPage(async record => {
    if (pathOf(record) === '/_tmax/state') return { status: 200, body: pageOwned }
    if (pathOf(record) === '/_tmax/observe') {
      return { status: 502, body: { ok: false, code: 'observe_unavailable', message: 'transport failed' } }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    const ready = await page.mount()
    // The ready state's next action is Apply.
    assert.ok(buttonByText(ready, 'connect'), 'a ready page-owned state offers Apply')
    const failed = await page.click('refresh')
    const text = textOf(failed)
    assert.match(text, /stateLastKnown/)
    // The last-known state is stale evidence: no write may be offered from it.
    assert.equal(buttonByText(failed, 'connect'), undefined, 'a failed read withdraws Apply')
    assert.ok(buttonByText(failed, 'retry'), 'the only next action is the read retry')
    // That retry is one GET; it never observes or writes.
    const before = page.calls.length
    await page.click('retry')
    const last = page.calls[page.calls.length - 1]
    assert.equal(page.calls.length, before + 1)
    assert.equal(last.method, 'GET')
    assert.equal(pathOf(last), '/_tmax/state')
  } finally {
    page.restore()
  }
})

test('a later refused read keeps the last good table and names its section errors', async () => {
  let phase = 'good'
  const refused = {
    ok: false,
    schemaVersion: 2,
    errors: [{ section: 'projection', code: 'inventory_unavailable', message: 'inventory read failed' }],
    target: null,
    routes: [],
    routing: [],
    providers: { tmax: [] },
    diagnostics: { lastError: null },
  }
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: phase === 'good' ? connected : refused }
    if (path === '/_tmax/observe') {
      return { status: 502, body: { ok: false, code: 'observe_unavailable', message: 'transport failed' } }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    await page.mount()
    assert.match(textOf(await page.click('refresh')), /stateLastKnown/)
    phase = 'refused'
    const tree = await page.click('retry')
    const text = textOf(tree)
    // A refused read model does not clear the last good table...
    assert.match(text, /kimi/)
    assert.match(text, /stateLastKnown/)
    // ...and the refused read's own section failure is named once, beside it.
    assert.match(text, /readFailed: inventory_unavailable/)
    assert.equal(buttonByText(tree, 'retry') ? collectButtons(tree).filter(button => textOf(button).trim() === 'retry').length : 0, 1, 'one retry, not one per panel')
    assert.equal(buttonByText(tree, 'connect'), undefined, 'a refused read offers no write')
  } finally {
    page.restore()
  }
})

test('Apply previews with dryRun before it writes, then uses the returned state', async () => {
  const noRoutes = {
    ...connected,
    routes: [],
    target: { ...connected.target, blockers: [] },
    ownership: { owner: 'page', scope: 'profile_scoped', reason: null, homeIntegration: { present: false, entry: false, artifact: false }, conflict: null },
  }
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: noRoutes }
    if (path === '/_tmax/connect') {
      const body = JSON.parse(record.body)
      const connect = {
        schema_version: 1,
        target: { id: 'dsh-main', path: '/home/user/.dsh' },
        gateway: { listen: '127.0.0.1:17840', ready: null, reason_code: 'daemon_identity_unavailable' },
        credentials: { path: '/home/user/.dsh/.credentials.yaml', state: 'absent', permissions: null },
        references: [{ route: 'tmax', protocol: 'responses', credential_ref: 'TMAX_REF', action: body.dryRun === true ? 'would_write' : 'written' }],
        changed: body.dryRun === true ? null : true,
        effects: { native_writes: body.dryRun !== true, secret_reads: false, processes_started: false, locks_created: false },
      }
      if (body.dryRun === true) return { status: 200, body: { ok: true, dryRun: true, connect, state: noRoutes } }
      return { status: 200, body: { ok: true, dryRun: false, connect, state: connected } }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    const ready = await page.mount()
    const before = textOf(ready)
    // Nothing declared: the verdict says so and Apply is its own control, with
    // no explanatory sentence beside it.
    assert.match(before, /stateDisconnected/)
    assert.ok(buttonByText(ready, 'connect'))
    assert.doesNotMatch(before, /nextConnect/)

    const preview = await page.click('connect')
    assert.equal(page.calls.length, 2)
    assert.equal(pathOf(page.calls[1]), '/_tmax/connect')
    assert.equal(page.calls[1].body, JSON.stringify({ dryRun: true }))
    const previewText = textOf(preview)
    assert.match(previewText, /connectPreview/)
    assert.match(previewText, /TMAX_REF/)
    assert.match(previewText, /actionWouldWrite/)
    assert.match(previewText, /effectNativeWrites: no/)
    // The preview discloses the target, gateway and credential file state.
    assert.match(previewText, /target: dsh-main/)
    assert.match(previewText, /targetPath: \/home\/user\/\.dsh/)
    assert.match(previewText, /gatewayListen: 127\.0\.0\.1:17840/)
    assert.match(previewText, /gatewayReady: unknown/)
    assert.match(previewText, /credentialPath: \/home\/user\/\.dsh\/\.credentials\.yaml/)
    assert.match(previewText, /credentialState: absent/)

    const applied = await page.click('connectApply')
    assert.equal(page.calls.length, 3)
    assert.equal(pathOf(page.calls[2]), '/_tmax/connect')
    assert.equal(page.calls[2].body, JSON.stringify({ dryRun: false }))
    const appliedText = textOf(applied)
    assert.match(appliedText, /connectApplied/)
    assert.match(appliedText, /actionWritten/)
    assert.match(appliedText, /effectNativeWrites: yes/)
    // The mutation returned state; the page renders it without another GET.
    assert.match(appliedText, /kimi/)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1)
  } finally {
    page.restore()
  }
})

test('an unreadable tmax state names the host failure and offers a retry', async () => {
  const unreadable = {
    ok: false,
    schemaVersion: 2,
    generatedAt: '2026-11-23T10:15:00Z',
    target: null,
    routes: [],
    providers: { tmax: [] },
    routing: [],
    diagnostics: { lastError: { code: 'tmax_unreadable', message: 'tmax CLI timed out' } },
  }
  const page = await mountPage(withState(unreadable))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /hostError/)
    assert.match(text, /tmax_unreadable: tmax CLI timed out/)
    assert.ok(buttonByText(tree, 'retry'), 'a retry button is offered')
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
  } finally {
    page.restore()
  }
})

// ---------------------------------------------------------------------------
// Explicit repair actions (Phase 2)
// ---------------------------------------------------------------------------

test('action availability follows provider evidence', async () => {
  const providerActionsOf = (await moduleWith(inertReact())).__pure.providerActions
  const quiet = { id: 'p', suspension: { state: 'unsupported' }, failure: { state: 'not_configured', consecutiveFailures: 0 } }
  const suspended = { id: 'p', suspension: { state: 'suspended' }, failure: { state: 'not_configured', consecutiveFailures: 0 } }
  const failing = { id: 'p', suspension: { state: 'unsupported' }, failure: { state: 'active', consecutiveFailures: 3 } }
  assert.deepEqual(providerActionsOf(quiet).map(action => action.action), ['observe_provider'])
  assert.deepEqual(providerActionsOf(suspended).map(action => action.action), ['resume_quota', 'observe_provider'])
  assert.deepEqual(providerActionsOf(failing).map(action => action.action), ['reset_failure', 'observe_provider'])
  // A provider without an id cannot be an operand at all.
  assert.deepEqual(providerActionsOf({ suspension: { state: 'suspended' } }), [])
})

test('action payloads carry one exact allowlisted operation', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  assert.deepEqual(pure.actionPayload({ action: 'resume_quota', provider: 'kimi' }, true),
    { action: 'resume_quota', provider: 'kimi', dryRun: true })
  assert.deepEqual(pure.actionPayload({ action: 'reset_failure', provider: 'kimi' }, false),
    { action: 'reset_failure', provider: 'kimi', dryRun: false })
  assert.deepEqual(pure.actionPayload({ action: 'observe_provider', provider: 'kimi', scope: 'auth' }, true),
    { action: 'observe_provider', provider: 'kimi', scope: 'auth', dryRun: true })
  // An out-of-enum scope falls back to the one scope the button means.
  assert.deepEqual(pure.actionPayload({ action: 'observe_provider', provider: 'kimi', scope: 'everything' }, true),
    { action: 'observe_provider', provider: 'kimi', scope: 'quota', dryRun: true })
  assert.deepEqual(pure.actionPayload({ action: 'open_management' }, false), { action: 'open_management', dryRun: false })
  // No provider or no known action, no request: the client invents no operand.
  assert.equal(pure.actionPayload({ action: 'resume_quota' }, true), undefined)
  assert.equal(pure.actionPayload({ action: 'resume_quota', provider: '' }, true), undefined)
  assert.equal(pure.actionPayload({ action: 'delete_everything', provider: 'kimi' }, true), undefined)
  assert.equal(pure.actionPayload(undefined, true), undefined)
})

test('a typed action result is grouped, bounded, and failure is not success', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const preview = {
    ok: true,
    result: {
      code: 'web_status',
      status: 'unchanged',
      message: 'the listener is not running',
      data: { ready: false, listen: '127.0.0.1:17840' },
      blockers: [{ code: 'listener_not_running', message: 'start the listener' }],
      next_actions: ['web open'],
      effects: { native_writes: false, processes_started: false, secret_reads: false, locks_created: false },
    },
  }
  assert.equal(pure.actionSucceeded(preview, true), true)
  assert.equal(pure.actionSucceeded(preview, false), false)
  assert.equal(pure.actionSucceeded({ ok: true, result: { status: 'blocked', code: 'quota_not_suspended' } }, true), false)
  assert.equal(pure.actionSucceeded({ ok: true, result: { status: 'failed' } }, true), false)
  assert.equal(pure.actionSucceeded({ ok: false, result: { status: 'succeeded' } }, true), false)
  assert.equal(pure.actionSucceeded(undefined, true), false)
  // A result with no status field is still whatever `ok` says.
  assert.equal(pure.actionSucceeded({ ok: true }, true), true)

  const lines = pure.actionResultLines(preview.result, t)
  const flat = lines.map(line => `${line.label}: ${line.items.join(' | ')}`).join(' || ')
  assert.match(flat, /actionCode: web_status/)
  assert.match(flat, /actionStatus: unchanged/)
  assert.match(flat, /actionMessage: the listener is not running/)
  assert.match(flat, /actionData: ready: false \| listen: 127\.0\.0\.1:17840/)
  assert.match(flat, /connectBlockers: listener_not_running: start the listener/)
  assert.match(flat, /connectNextActions: web open/)
  assert.match(flat, /effectNativeWrites: no/)
  assert.match(flat, /effectProcessesStarted: no/)
  // A management preview reads the nested `web status` facts by name, so the
  // readiness is disclosed instead of a daemon JSON blob.
  const management = pure.actionResultLines({
    code: 'ok',
    status: 'success',
    data: {
      daemon: { discovery_state: 'unidentified', reason_code: 'daemon_identity_unavailable', web: { state: 'stopped', listen: null, url: null } },
      gateway: { ready: false, listen: '127.0.0.1:17840' },
    },
  }, t, 'open_management')
  const managementFlat = management.map(line => `${line.label}: ${line.items.join(' | ')}`).join(' || ')
  // The host's normalized `unidentified` and tmax's real tokens both render as
  // their known words; only unknown tokens stay verbatim.
  assert.match(managementFlat, /gatewayDaemon: daemonUnidentified/)
  assert.match(managementFlat, /gatewayReason: daemon_identity_unavailable/)
  const compatible = pure.actionResultLines({
    code: 'ok',
    status: 'success',
    data: { daemon: { discovery_state: 'compatible', web: { state: 'running', listen: '127.0.0.1:3013', url: 'http://127.0.0.1:3013' } } },
  }, t, 'open_management')
  const compatibleFlat = compatible.map(line => `${line.label}: ${line.items.join(' | ')}`).join(' || ')
  assert.match(compatibleFlat, /gatewayDaemon: daemonIdentified/)
  assert.doesNotMatch(compatibleFlat, /compatible/)
  const oddToken = pure.actionResultLines({ data: { daemon: { discovery_state: 'incompatible' } } }, t, 'open_management')
  assert.match(oddToken.map(line => line.items.join(' ')).join(' '), /incompatible/)
  assert.match(managementFlat, /webListener: webStopped/)
  assert.match(managementFlat, /gatewayReady: no/)
  assert.match(managementFlat, /gatewayListen: 127\.0\.0\.1:17840/)
  // A non-management result uses the generic bounded data renderer.
  const generic = pure.actionResultLines({ data: { changed: true } }, t, 'resume_quota')
  assert.match(generic.map(line => line.items.join(' ')).join(' '), /changed: true/)
  // Host text is bounded: a huge message cannot become a transcript.
  const huge = pure.actionResultLines({ code: 'x'.repeat(10), status: 'failed', message: 'y'.repeat(4000) }, t)
  for (const line of huge) for (const item of line.items) assert.ok(item.length <= 300)
  assert.deepEqual(pure.actionResultLines(undefined, t), [])
})

test('a management preview names only listener states the host reported', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const lines = (data, translate = t) => pure.actionResultLines({ code: 'ok', status: 'success', data }, translate, 'open_management')
  const flat = (data, translate = t) => lines(data, translate)
    .map(line => `${line.label}: ${line.items.join(' | ')}`)
    .join(' || ')
  const en = key => pure.locales.en[key] ?? key
  const zh = key => pure.locales.zh[key] ?? key

  // `daemon.web: null` and an absent key are missing reports, not stops.
  const nullWeb = flat({ daemon: { discovery_state: 'unidentified', web: null } })
  assert.match(nullWeb, /webListener: notReported/)
  assert.doesNotMatch(nullWeb, /webStopped|webRunning/)
  const absentWeb = flat({ daemon: { discovery_state: 'absent' } })
  assert.match(absentWeb, /webListener: notReported/)
  // A listener record with no state field is unknown, not stopped either.
  const noState = flat({ daemon: { web: { listen: '127.0.0.1:17840', url: null } } })
  assert.match(noState, /webListener: unknown/)
  assert.doesNotMatch(noState, /webStopped|webRunning/)

  // Only explicit lifecycle states name one.
  const stopped = flat({ daemon: { web: { state: 'stopped', listen: null, url: null } } })
  assert.match(stopped, /webListener: webStopped/)
  assert.doesNotMatch(stopped, /webRunning/)
  const running = flat({ daemon: { web: { state: 'running', listen: '127.0.0.1:17840', url: 'http://127.0.0.1:17840' } } })
  assert.match(running, /webListener: webRunning/)
  assert.match(running, /gatewayListen: 127\.0\.0\.1:17840/)
  assert.match(running, /webUrl: http:\/\/127\.0\.0\.1:17840/)

  // An unrecognized machine state is shown verbatim, never as a lifecycle word.
  const unavailable = flat({ daemon: { web: { state: 'unavailable', listen: null, url: null } } })
  assert.match(unavailable, /webListener: unavailable/)
  assert.doesNotMatch(unavailable, /webStopped|webRunning/)

  // Locale parity: both dictionaries carry the words the preview uses.
  for (const dictionary of [pure.locales.en, pure.locales.zh]) {
    for (const key of ['webListener', 'webRunning', 'webStopped', 'notReported', 'unknown']) {
      assert.equal(typeof dictionary[key], 'string', `${key} is translated`)
      assert.notEqual(dictionary[key], '', `${key} is not empty`)
    }
    assert.notEqual(dictionary.webStopped, dictionary.notReported)
    assert.notEqual(dictionary.webRunning, dictionary.webStopped)
  }

  // Real English and Chinese copy: a missing record never reads as stopped.
  assert.match(flat({ daemon: { web: null } }, en), /Web listener: not reported/)
  assert.doesNotMatch(flat({ daemon: { web: null } }, en), /Web listener: not running/)
  assert.match(flat({ daemon: { web: { state: 'stopped' } } }, en), /Web listener: not running/)
  assert.match(flat({ daemon: { web: { state: 'running' } } }, en), /Web listener: running/)
  assert.match(flat({ daemon: { web: null } }, zh), /Web 监听： *未报告/)
  assert.match(flat({ daemon: { web: { state: 'stopped' } } }, zh), /Web 监听： *未运行/)
})

test('a suspended provider previews then executes one resume, and mount runs no action', async () => {
  const suspended = withProviderEvidence({
    kimi: { suspension: { state: 'suspended', reasonCode: 'quota_exhausted', resetAt: '2026-11-26T15:42:16Z', source: 'kimi_usage_api', window: 'weekly' } },
  })
  const resumed = withProviderEvidence({
    kimi: { suspension: { state: 'unsupported', reasonCode: null, resetAt: null, source: null, window: null } },
  })
  const resumedState = { ...resumed, configurationRevision: 'ccccccccddddddddeeeeeeeeffffffff0000000011111111222222223333333344444444' }
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: suspended }
    if (path === '/_tmax/action') {
      const body = JSON.parse(record.body)
      if (body.dryRun === true) {
        return {
          status: 200,
          body: {
            ok: true,
            action: body.action,
            dryRun: true,
            result: {
              code: 'quota_resume_preview',
              status: 'unchanged',
              message: 'the provider would be resumed',
              data: { suspension: 'suspended', changed: false },
              blockers: [],
              next_actions: ['resume_quota'],
              effects: { native_writes: false, secret_reads: false, processes_started: false, locks_created: false },
            },
            state: suspended,
          },
        }
      }
      return {
        status: 200,
        body: {
          ok: true,
          action: body.action,
          dryRun: false,
          result: { code: 'quota_resumed', status: 'succeeded', message: 'suspension lifted', data: { changed: true }, effects: { native_writes: true } },
          state: resumedState,
        },
      }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    const mounted = await page.mount()
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/action').length, 0)
    assert.ok(buttonByText(mounted, 'actionResumeQuota'), 'a suspended quota offers exactly one resume')
    assert.equal(buttonByText(mounted, 'actionResetFailure'), undefined, 'no failure, no reset')

    const preview = await page.click('actionResumeQuota')
    assert.equal(page.calls.length, 2)
    assert.equal(page.calls[1].method, 'POST')
    assert.equal(pathOf(page.calls[1]), '/_tmax/action')
    assert.equal(page.calls[1].body, JSON.stringify({ action: 'resume_quota', provider: 'kimi', dryRun: true }))
    const previewText = textOf(preview)
    assert.match(previewText, /actionPreviewTitle/)
    assert.match(previewText, /actionPreviewNote/)
    assert.match(previewText, /quota_resume_preview/)
    assert.ok(buttonByText(preview, 'actionConfirm'), 'a preview waits for explicit confirmation')
    assert.ok(buttonByText(preview, 'actionCancel'))

    const executed = await page.click('actionConfirm')
    assert.equal(page.calls.length, 3)
    assert.equal(page.calls[2].body, JSON.stringify({ action: 'resume_quota', provider: 'kimi', dryRun: false }))
    const executedText = textOf(executed)
    assert.match(executedText, /actionDone/)
    assert.match(executedText, /quota_resumed/)
    assert.match(executedText, /effectNativeWrites: yes/)
    // The returned read model is adopted without another GET, and the suspended
    // tag and resume button are gone because the evidence changed.
    assert.doesNotMatch(executedText, /providerSuspended/)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1)
    assert.equal(buttonByText(executed, 'actionResumeQuota'), undefined)
  } finally {
    page.restore()
  }
})

test('Reset failure sends one exact provider operation; observing is the page Refresh alone', async () => {
  const failing = withProviderEvidence({
    kimi: { failure: { state: 'active', consecutiveFailures: 3, activePriority: null, expiresAt: null, reasonCode: 'upstream_rejection' } },
  })
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: failing }
    if (path === '/_tmax/action') {
      const body = JSON.parse(record.body)
      return {
        status: 200,
        body: {
          ok: true,
          action: body.action,
          dryRun: body.dryRun === true,
          result: {
            code: body.dryRun === true ? 'action_preview' : 'action_done',
            status: body.dryRun === true ? 'unchanged' : 'succeeded',
            data: { provider: body.provider, scope: body.scope ?? null },
            effects: { native_writes: false, processes_started: false },
          },
          state: failing,
        },
      }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    const mounted = await page.mount()
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/action').length, 0)
    assert.ok(buttonByText(mounted, 'actionResetFailure'), 'an active failure offers one reset')
    // No card repeats the observation: one Refresh reads every account.
    assert.equal(buttonByText(mounted, 'actionObserveQuota'), undefined)

    await page.click('actionResetFailure')
    assert.equal(page.calls.length, 2)
    assert.equal(page.calls[1].body, JSON.stringify({ action: 'reset_failure', provider: 'kimi', dryRun: true }))
    await page.click('actionConfirm')
    assert.equal(page.calls.length, 3)
    assert.equal(page.calls[2].body, JSON.stringify({ action: 'reset_failure', provider: 'kimi', dryRun: false }))
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1)
  } finally {
    page.restore()
  }
})
test('a refused or unavailable action is a failure, never success, and keeps the last good state', async () => {
  const suspended = withProviderEvidence({
    kimi: { suspension: { state: 'suspended', reasonCode: 'quota_exhausted', resetAt: null, source: null, window: null } },
  })
  const changed = { ...suspended, configurationRevision: 'ddddddddeeeeeeeeffffffff000000001111111122222222333333334444444455555555' }
  let attempts = 0
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: suspended }
    if (path === '/_tmax/action') {
      attempts += 1
      if (attempts === 1) {
        // 409: a typed tmax refusal, with a fresh state the page must not adopt.
        return {
          status: 409,
          body: {
            ok: false,
            action: 'resume_quota',
            dryRun: true,
            result: { code: 'quota_not_suspended', status: 'blocked', message: 'the provider is not suspended' },
            state: changed,
          },
        }
      }
      return { status: 502, body: { ok: false, code: 'action_unavailable', message: 'tmax did not answer' } }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    await page.mount()
    const refused = await page.click('actionResumeQuota')
    const refusedText = textOf(refused)
    assert.match(refusedText, /actionFailed/)
    assert.match(refusedText, /quota_not_suspended/)
    assert.match(refusedText, /actionStatus blocked/)
    assert.doesNotMatch(refusedText, /actionDone/)
    // A refused action keeps the last good read model, not the one in the error.
    assert.match(refusedText, /kimi/, 'the last good state stays on screen')
    assert.doesNotMatch(refusedText, /unreachable-account/)
    assert.ok(buttonByText(refused, 'close'))

    await page.click('close')
    const unavailableText = textOf(await page.click('actionResumeQuota'))
    assert.match(unavailableText, /actionFailed/)
    assert.match(unavailableText, /action_unavailable: tmax did not answer/)
    assert.doesNotMatch(unavailableText, /actionDone/)
    assert.match(unavailableText, /kimi/, 'the last good state stays on screen')
  } finally {
    page.restore()
  }
})

test('a pending action disables every action control until it settles', async () => {
  const suspended = withProviderEvidence({
    kimi: { suspension: { state: 'suspended', reasonCode: 'quota_exhausted', resetAt: null, source: null, window: null } },
  })
  let release
  const gate = new Promise(resolve => { release = resolve })
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: suspended }
    if (path === '/_tmax/action') {
      await gate
      return {
        status: 200,
        body: {
          ok: true,
          action: 'resume_quota',
          dryRun: true,
          result: { code: 'quota_resume_preview', status: 'unchanged' },
          state: suspended,
        },
      }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    const mounted = await page.mount()
    const resume = buttonByText(mounted, 'actionResumeQuota')
    assert.ok(resume)
    resume.props.onClick()
    const pending = await page.settle()
    // While the preview is in flight every action control, and Refresh, is
    // disabled: one explicit operation at a time.
    const pendingButtons = collectButtons(pending)
    assert.ok(pendingButtons.length >= 3)
    for (const button of pendingButtons) {
      assert.equal(button.props.disabled, true, `"${textOf(button).trim()}" is disabled while busy`)
    }
    assert.match(textOf(pending), /actionWorking|actionPreviewTitle/)

    release()
    const settled = await page.settle()
    assert.ok(buttonByText(settled, 'actionConfirm'), 'the preview arrives once the host answers')
  } finally {
    page.restore()
  }
})


// ---------------------------------------------------------------------------
// Phase 3 DSH seats: marker, Settings footer, model menu, tool card, gateway UI
// ---------------------------------------------------------------------------

test('the sidebar marker answers with one severity, never a success it cannot prove', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const base = {
    ok: true,
    target: { blockers: [] },
    ownership: { conflict: null },
    gateway: { configured: true, ready: true, daemonDiscoveryState: 'identified' },
    routes: [{ active: true, credentialConfigured: true, blockers: [] }],
    providers: { tmax: [{ id: 'kimi', suspension: { state: 'unsupported' }, failure: {} }] },
  }
  assert.deepEqual(pure.markerStatus(base), { tone: 'ok', key: 'markerReady' })
  assert.deepEqual(pure.markerStatus(undefined), { tone: 'unknown', key: 'markerUnknown' })
  assert.deepEqual(pure.markerStatus({ ...base, ok: false }), { tone: 'unknown', key: 'markerUnknown' })
  assert.deepEqual(pure.markerStatus({
    ...base,
    providers: { tmax: [{ id: 'kimi', suspension: { state: 'suspended' } }] },
  }), { tone: 'error', key: 'markerSuspended' })
  assert.deepEqual(pure.markerStatus({ ...base, gateway: { ...base.gateway, ready: false } }),
    { tone: 'error', key: 'markerGatewayDown' })
  assert.deepEqual(pure.markerStatus({ ...base, ownership: { conflict: { code: 'duplicate' } } }),
    { tone: 'error', key: 'markerConflict' })
  assert.deepEqual(pure.markerStatus({ ...base, target: { blockers: [{ code: 'gateway_not_ready' }] } }),
    { tone: 'error', key: 'markerBlocker' })
  assert.deepEqual(pure.markerStatus({
    ...base,
    routes: [{ active: true, credentialConfigured: true, blockers: [{ code: 'route_no_usable_models' }] }],
  }), { tone: 'error', key: 'markerBlocker' })
  // A ready-unknown gateway is a warning: nothing has failed, nothing is proven.
  assert.deepEqual(pure.markerStatus({ ...base, gateway: { ...base.gateway, ready: null } }),
    { tone: 'warn', key: 'markerGatewayUnknown' })
  assert.deepEqual(pure.markerStatus({ ...base, routes: [{ active: false, credentialConfigured: true, blockers: [] }] }),
    { tone: 'warn', key: 'markerInactive' })
  assert.deepEqual(pure.markerStatus({ ...base, routes: [{ active: true, credentialConfigured: false, blockers: [] }] }),
    { tone: 'warn', key: 'markerCredential' })
  assert.deepEqual(pure.markerStatus({ ...base, routes: [] }), { tone: 'warn', key: 'markerNoRoute' })
})

test('Settings > Models availability names why tmax models are absent', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const route = { active: true, credentialConfigured: true, blockers: [] }
  assert.equal(pure.modelsAvailability(undefined), 'unknown')
  assert.equal(pure.modelsAvailability({ ok: false }), 'unknown')
  assert.equal(pure.modelsAvailability({ ok: true, ownership: { reason: 'apply_not_run' }, routes: [], errors: [] }), 'none')
  assert.equal(pure.modelsAvailability({ ok: true, ownership: {}, routes: [], errors: [] }), 'none')
  assert.equal(pure.modelsAvailability({ ok: true, ownership: {}, routes: [{ ...route, active: false }], errors: [] }), 'inactive')
  assert.equal(pure.modelsAvailability({ ok: true, ownership: {}, routes: [{ ...route, credentialConfigured: false }], errors: [] }), 'credential')
  assert.equal(pure.modelsAvailability({ ok: true, ownership: {}, routes: [route], errors: [] }), 'available')
  // A failed projection read is unknown, never "no models".
  assert.equal(pure.modelsAvailability({ ok: true, ownership: {}, routes: [], errors: [{ section: 'projection', code: 'x', message: 'y' }] }), 'unknown')
  // A state that carries no route list has nothing to count.
  assert.equal(pure.modelsAvailability({ ok: true, ownership: {}, errors: [] }), 'unknown')
  // A failed section read leaves the list unknown even when routes look fine.
  assert.equal(pure.modelsAvailability({
    ok: true,
    ownership: {},
    routes: [{ active: true, credentialConfigured: true }],
    errors: [{ section: 'providers', code: 'providers_unavailable', message: 'read failed' }],
  }), 'unknown')
})

test('the state gate accepts only schemaVersion 2 with ok true', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const good = { schemaVersion: 2, ok: true, routes: [] }
  assert.deepEqual(pure.stateGate(good, { status: 200 }), { object: true, state: good })
  // A non-object payload is not a state and is never adopted.
  assert.equal(pure.stateGate(null, { status: 200 }).object, false)
  assert.equal(pure.stateGate(undefined, { status: 200 }).object, false)
  // An unsupported schema is a typed failure carrying the value it saw.
  assert.deepEqual(pure.stateGate({ schemaVersion: 3 }, { status: 200 }),
    { object: true, schemaUnsupported: true, message: '3' })
  // A refused model is an error, never evidence.
  assert.equal(pure.stateGate({ schemaVersion: 2, ok: false, code: 'state_unavailable', message: 'no answer' }, { status: 502 }).message,
    'state_unavailable: no answer')
  assert.equal(pure.stateGate({ schemaVersion: 2, ok: 'yes' }, { status: 200 }).state, undefined)
})

test('unknown credential readiness is never drawn as a missing credential', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const base = {
    ok: true,
    target: { blockers: [] },
    ownership: { conflict: null },
    gateway: { configured: true, ready: true, daemonDiscoveryState: 'identified' },
    providers: { tmax: [] },
  }
  const active = credentialConfigured => [{ active: true, credentialConfigured, blockers: [] }]
  // A readiness tmax never reported is a warning that names the unknown.
  assert.deepEqual(pure.markerStatus({ ...base, routes: active(null) }),
    { tone: 'warn', key: 'markerCredentialUnknown' })
  assert.deepEqual(pure.markerStatus({ ...base, routes: active(undefined) }),
    { tone: 'warn', key: 'markerCredentialUnknown' })
  // Only a credential the host reports as missing is named as missing.
  assert.deepEqual(pure.markerStatus({ ...base, routes: active(false) }),
    { tone: 'warn', key: 'markerCredential' })
  assert.deepEqual(pure.markerStatus({ ...base, routes: [...active(false), ...active(null)] }),
    { tone: 'warn', key: 'markerCredential' })
  assert.deepEqual(pure.markerStatus({ ...base, routes: active(true) }),
    { tone: 'ok', key: 'markerReady' })

  const state = routes => ({ ok: true, ownership: {}, routes, errors: [] })
  assert.equal(pure.modelsAvailability(state(active(null))), 'credentialUnknown')
  assert.equal(pure.modelsAvailability(state(active(undefined))), 'credentialUnknown')
  assert.equal(pure.modelsAvailability(state(active(false))), 'credential')
  assert.equal(pure.modelsAvailability(state([...active(false), ...active(null)])), 'credential')
  assert.equal(pure.modelsAvailability(state(active(true))), 'available')

  // The page's own verdict already drew the same line: unknown readiness is
  // not "registered, not usable"; only a reported absence is.
  const load = routes => ({ phase: 'ready', value: { ...base, routes } })
  assert.deepEqual(pure.verdict(load(active(null)), t), { tone: 'idle', text: 'stateUnknown' })
  assert.deepEqual(pure.verdict(load(active(false)), t), { tone: 'warn', text: 'stateNotUsable' })
})

test('the tmax_status tool text is parsed as data, never as a success claim', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const value = {
    ok: true,
    target: { id: 'dsh-main', path: '/home/user/.dsh' },
    gateway: { ready: false },
    routes: [{ provider: 'tmax', models: ['a', 'b'] }, { provider: 'other', models: ['c'] }],
    providers: { tmax: [{ id: 'kimi' }, { id: 'cpa' }] },
    blockers: [{ code: 'gateway_not_ready', message: 'start it' }],
    errors: [],
    routing: [],
    diagnostics: {},
  }
  const summary = pure.toolStatusSummary([{ type: 'text', text: JSON.stringify(value) }])
  assert.equal(summary.parsed, true)
  assert.equal(summary.ok, true)
  assert.equal(summary.targetId, 'dsh-main')
  assert.equal(summary.targetPath, '/home/user/.dsh')
  assert.equal(summary.gatewayReady, false)
  assert.equal(summary.routeCount, 2)
  assert.deepEqual(summary.routeNames, ['tmax', 'other'])
  assert.equal(summary.modelCount, 3)
  assert.equal(summary.providerCount, 2)
  assert.equal(summary.blockerCount, 1)
  assert.deepEqual(summary.blockers, ['gateway_not_ready: start it'])
  // Non-JSON text is kept bounded and is not a parsed success.
  const broken = pure.toolStatusSummary([{ type: 'text', text: `not json ${'x'.repeat(1000)}` }])
  assert.equal(broken.parsed, false)
  assert.ok(broken.text.length <= 401)
  assert.equal(broken.ok, undefined)
  assert.equal(pure.toolStatusSummary([]), undefined)
  assert.equal(pure.toolStatusSummary(undefined), undefined)
})

test('the sidebar marker reads state once, never posts, and holds no controls', async () => {
  const suspended = seatState({ providers: { tmax: [{ id: 'kimi', suspension: { state: 'suspended' } }], agent: [] } })
  const page = await mountSeat(withState(suspended), slot('sidebar.panellist.trailing'), { wide: true, active: true })
  try {
    const tree = await page.mount()
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    assert.equal(page.calls.filter(call => call.method === 'POST').length, 0)
    const mark = collectNodes(tree, node => String(node.props?.className ?? '').includes('tmx-navMark'))[0]
    assert.ok(mark, 'the marker renders')
    assert.equal(mark.props['data-tone'], 'error')
    assert.equal(mark.props['data-wide'], 'true')
    assert.equal(mark.props.title, 'title: markerSuspended')
    assert.equal(collectButtons(tree).length, 0, 'the marker is not interactive')
    assert.match(textOf(tree), /markerSuspended/)
  } finally {
    page.restore()
  }
})

test('the sidebar marker degrades to a dot in the rail', async () => {
  const page = await mountSeat(withState(seatState()), slot('sidebar.panellist.trailing'), { wide: false, active: false })
  try {
    const tree = await page.mount()
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    const mark = collectNodes(tree, node => String(node.props?.className ?? '').includes('tmx-navMark'))[0]
    assert.equal(mark.props['data-wide'], 'false')
    assert.equal(mark.props['data-tone'], 'ok')
    assert.equal(collectButtons(tree).length, 0)
  } finally {
    page.restore()
  }
})

test('the Settings footer explains missing models and previews Apply before executing', async () => {
  const selected = []
  const empty = seatState({
    routes: [],
    ownership: {
      owner: 'page',
      scope: 'profile_scoped',
      reason: null,
      homeIntegration: { present: false, entry: false, artifact: false },
      conflict: null,
      mode: 'auto',
    },
  })
  const connectedState = {
    ...seatState(),
    configurationRevision: 'feedfacecafebeef1234567890feedface',
  }
  const page = await mountSeat(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: empty }
    if (path === '/_tmax/connect') {
      const body = JSON.parse(record.body)
      // The host passes raw `tmax agent apply` data through.
      const connect = body.dryRun === true
        ? {
          target: 'dsh-main',
          dry_run: true,
          files: [{ path: '/home/user/.dsh/cordis.patch.yml' }],
          effects: { native_writes: false, secret_reads: false, processes_started: false, locks_created: false },
        }
        : {
          target: 'dsh-main',
          state: 'applied_reload_pending',
          written: true,
          changed_files: ['/home/user/.dsh/cordis.patch.yml'],
          unchanged_files: ['/home/user/.dsh/tmax-dsh.mjs'],
          recovery: 'rec_1',
        }
      return { status: 200, body: { ok: true, dryRun: body.dryRun === true, connect, state: body.dryRun === true ? empty : connectedState } }
    }
    return { status: 404, body: { ok: false } }
  }, slot('settings.models.footer'), {}, { services: { layout: { selectPanel: id => selected.push(id) } } })
  try {
    const mounted = await page.mount()
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    const mountedText = textOf(mounted)
    assert.match(mountedText, /modelsNone/)
    assert.match(mountedText, /footerGateway/)
    assert.match(mountedText, /gatewayReady/)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/observe').length, 0)

    const preview = await page.click('connect')
    assert.equal(page.calls.length, 2)
    assert.equal(page.calls[1].method, 'POST')
    assert.equal(pathOf(page.calls[1]), '/_tmax/connect')
    assert.equal(page.calls[1].body, JSON.stringify({ dryRun: true }))
    const previewText = textOf(preview)
    assert.match(previewText, /connectPreview/)
    // The preview discloses the planned native file and the effect flags.
    assert.match(previewText, /connectFiles/)
    assert.match(previewText, /cordis\.patch\.yml/)
    assert.match(previewText, /effectNativeWrites: no/)
    assert.match(previewText, /effectSecretReads: no/)

    const applied = await page.click('connectApply')
    assert.equal(page.calls.length, 3)
    assert.equal(page.calls[2].body, JSON.stringify({ dryRun: false }))
    const appliedText = textOf(applied)
    assert.match(appliedText, /connectApplied/)
    // The result reports applied state, changed/unchanged files, and recovery.
    assert.match(appliedText, /connectState applied_reload_pending/)
    assert.match(appliedText, /connectChangedFiles/)
    assert.match(appliedText, /cordis\.patch\.yml/)
    assert.match(appliedText, /connectUnchangedFiles/)
    assert.match(appliedText, /tmax-dsh\.mjs/)
    assert.match(appliedText, /connectRecovery rec_1/)
    // The returned read model is adopted: the card now reports a listed route.
    assert.match(appliedText, /modelsAvailable/)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/observe').length, 0)

    await page.click('footerOpenPage')
    assert.deepEqual(selected, ['tmax'])
  } finally {
    page.restore()
  }
})

test('the Settings footer disables the page jump when the layout has no tmax page', async () => {
  const page = await mountSeat(withState(seatState()), slot('settings.models.footer'))
  try {
    const tree = await page.mount()
    const open = buttonByText(tree, 'footerOpenPage')
    assert.ok(open, 'the Open tmax control renders')
    assert.equal(open.props.disabled, true)
    assert.equal(open.props.title, 'footerPageUnavailable')
  } finally {
    page.restore()
  }
})

test('a page jump the layout refuses disables itself instead of throwing', async () => {
  let attempts = 0
  const page = await mountSeat(withState(seatState()), slot('settings.models.footer'), {}, {
    services: {
      layout: {
        selectPanel: () => {
          attempts += 1
          throw new Error('layout.selectPanel: main panel "tmax" is not registered')
        },
      },
    },
  })
  try {
    const tree = await page.mount()
    const open = buttonByText(tree, 'footerOpenPage')
    assert.equal(open.props.disabled, false, 'the layout service exists, so the control is offered')
    const after = await page.click('footerOpenPage')
    assert.equal(attempts, 1)
    const closed = buttonByText(after, 'footerOpenPage')
    assert.equal(closed.props.disabled, true, 'a refused jump disables the control')
    assert.equal(closed.props.title, 'footerPageUnavailable')
    assert.equal(page.calls.filter(call => call.method === 'POST').length, 0)
  } finally {
    page.restore()
  }
})

test('the tmax_status card renders the tool result without another read', async () => {
  const content = [{
    type: 'text',
    text: JSON.stringify({
      ok: true,
      target: { id: 'dsh-main', path: '/home/user/.dsh' },
      gateway: { ready: false },
      routes: [{ provider: 'tmax', models: ['a', 'b'] }],
      providers: { tmax: [{ id: 'kimi' }, { id: 'cpa' }] },
      blockers: [{ code: 'gateway_not_ready', message: 'start it' }],
      errors: [],
      routing: [],
      diagnostics: {},
    }),
  }]
  const page = await mountSeat(withState(seatState()), slot('tool.call.toolview'), {
    phase: 'result',
    block: { kind: 'tool-result', content, isError: false },
  })
  try {
    const tree = await page.mount()
    assert.equal(page.calls.length, 0, 'the tool card reads nothing')
    const text = textOf(tree)
    assert.match(text, /toolTarget/)
    assert.match(text, /dsh-main/)
    assert.match(text, /home\/user\/\.dsh/)
    assert.match(text, /toolRoutes/)
    assert.match(text, /gatewayNotReady/)
    assert.match(text, /toolProviders/)
    assert.match(text, /2 tmax/)
    assert.match(text, /toolModels/)
    assert.match(text, /toolBlockers/)
    assert.match(text, /gateway_not_ready: start it/)
  } finally {
    page.restore()
  }
})

test('the tmax_status card falls back to bounded text for pending and unreadable calls', async () => {
  async function renderCard(props) {
    const page = await mountSeat(withState(seatState()), slot('tool.call.toolview'), props)
    try {
      return { tree: await page.mount(), calls: page.calls.length }
    } finally {
      page.restore()
    }
  }
  const pending = await renderCard({ phase: 'start', block: { name: 'tmax_status', args: {} } })
  assert.match(textOf(pending.tree), /toolPending/)
  assert.equal(pending.calls, 0, 'a pending card reads nothing')

  const broken = await renderCard({
    phase: 'result',
    block: { kind: 'tool-result', content: [{ type: 'text', text: `not json ${'x'.repeat(1000)}` }], isError: false },
  })
  const brokenText = textOf(broken.tree)
  assert.match(brokenText, /toolUnreadable/)
  assert.match(brokenText, /not json/)
  assert.ok(brokenText.length < 700, 'the fallback text is bounded')
  assert.equal(broken.calls, 0)
})

test('Start is offered for a configured gateway whose readiness is unknown, never for an unconfigured one', async () => {
  for (const [gateway, offered] of [
    [{ ...connected.gateway, configured: true, ready: null }, true],
    [{ ...connected.gateway, configured: false, ready: false }, false],
  ]) {
    const state = { ...connected, gateway, target: { ...connected.target, blockers: [] } }
    const page = await mountPage(withState(state))
    try {
      const tree = await page.mount()
      assert.equal(buttonByText(tree, 'actionStartGateway') !== undefined, offered, JSON.stringify(gateway))
    } finally {
      page.restore()
    }
  }
})

test('a gateway that is not ready is started from the verdict line, and a ready one offers no lifecycle control', async () => {
  const running = {
    ...connected,
    gateway: { ...connected.gateway, ready: true, daemonDiscoveryState: 'identified' },
    target: { ...connected.target, blockers: [] },
  }
  const page = await mountPage(async record => {
    const path = pathOf(record)
    if (path === '/_tmax/state') return { status: 200, body: connected }
    if (path === '/_tmax/action') {
      const body = JSON.parse(record.body)
      return {
        status: 200,
        body: {
          ok: true,
          action: body.action,
          dryRun: body.dryRun === true,
          result: {
            code: body.dryRun === true ? 'gateway_preview' : 'gateway_started',
            status: body.dryRun === true ? 'dry_run' : 'success',
            data: {
              daemon: { discovery_state: 'compatible', web: { state: 'running', listen: '127.0.0.1:3013', url: 'http://127.0.0.1:3013' } },
              gateway: { ready: true, listen: '127.0.0.1:17840' },
            },
          },
          state: running,
        },
      }
    }
    return { status: 404, body: { ok: false } }
  })
  try {
    const mounted = await page.mount()
    // Fixture gateway: configured, readiness unknown. Starting is the only verb
    // that can change that, and it is the verdict line's own control.
    const start = buttonByText(mounted, 'actionStartGateway')
    assert.ok(start, 'Start sits on the verdict line')
    assert.equal(start.props['data-kind'], 'primary')
    assert.equal(buttonByText(mounted, 'actionStopGateway'), undefined)
    assert.equal(buttonByText(mounted, 'actionRestartGateway'), undefined)

    // One click previews; nothing runs before the explicit confirmation.
    const preview = await page.click('actionStartGateway')
    assert.equal(page.calls.length, 2)
    assert.equal(page.calls[1].body, JSON.stringify({ action: 'gateway_start', dryRun: true }))
    const previewText = textOf(preview)
    assert.match(previewText, /actionPreviewTitle/)
    assert.match(previewText, /actionGatewayNote/)
    assert.match(previewText, /webRunning/)
    // A reader who cannot see the panel learns that the preview arrived.
    assert.match(liveText(preview), /actionPreviewTitle/)

    const executed = await page.click('actionConfirm')
    assert.equal(page.calls.length, 3)
    assert.equal(page.calls[2].body, JSON.stringify({ action: 'gateway_start', dryRun: false }))
    const executedText = textOf(executed)
    assert.match(executedText, /actionDone/)
    assert.match(liveText(executed), /actionDone: gateway_started/)
    // The returned read model is adopted: nothing is left to start, and the
    // page offers no stop or restart, which belong to the CLI.
    assert.match(executedText, /stateReady/)
    assert.equal(buttonByText(executed, 'actionStartGateway'), undefined)
    assert.equal(buttonByText(executed, 'actionStopGateway'), undefined)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/observe').length, 0)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1)
  } finally {
    page.restore()
  }
})

// ---------------------------------------------------------------------------
// Phase 3 review fixes: real tokens, Apply evidence, honest counts, marker
// ---------------------------------------------------------------------------

test('the sidebar marker degrades on an invalid config or a failed section read', async () => {
  const pure = (await moduleWith(inertReact())).__pure
  const base = {
    ok: true,
    target: { blockers: [] },
    ownership: { conflict: null },
    config: { state: 'valid', errors: [] },
    errors: [],
    gateway: { configured: true, ready: true, daemonDiscoveryState: 'identified' },
    routes: [{ active: true, credentialConfigured: true, blockers: [] }],
    providers: { tmax: [] },
  }
  assert.deepEqual(pure.markerStatus(base), { tone: 'ok', key: 'markerReady' })
  // A configuration that cannot be applied is an error even when the route and
  // gateway fields look usable.
  assert.deepEqual(pure.markerStatus({ ...base, config: { state: 'invalid', errors: [] } }),
    { tone: 'error', key: 'markerConfigError' })
  // An unvalidated config, or any failed section read, is a warning, never ok.
  assert.deepEqual(pure.markerStatus({ ...base, config: { state: 'unavailable', errors: [] } }),
    { tone: 'warn', key: 'markerConfigUnavailable' })
  assert.deepEqual(pure.markerStatus({ ...base, errors: [{ section: 'providers', code: 'x', message: 'y' }] }),
    { tone: 'warn', key: 'markerSectionError' })
  // A reported failure still outranks a degraded-read warning.
  assert.deepEqual(pure.markerStatus({
    ...base,
    errors: [{ section: 'providers', code: 'x', message: 'y' }],
    gateway: { ...base.gateway, ready: false },
  }), { tone: 'error', key: 'markerGatewayDown' })
})

test('a degraded marker is drawn on the row without opening the page', async () => {
  const page = await mountSeat(withState(seatState({ config: { state: 'invalid', revision: 'abc', errors: [] } })),
    slot('sidebar.panellist.trailing'), { wide: true, active: false })
  try {
    const tree = await page.mount()
    assert.deepEqual(page.calls.map(call => `${call.method} ${pathOf(call)}`), ['GET /_tmax/state'])
    const mark = collectNodes(tree, node => String(node.props?.className ?? '').includes('tmx-navMark'))[0]
    assert.equal(mark.props['data-tone'], 'error')
    assert.equal(mark.props.title, 'title: markerConfigError')
    assert.equal(collectButtons(tree).length, 0)
  } finally {
    page.restore()
  }
})

test('the Settings footer offers Apply only when the page next action is Apply', async () => {
  const route = seatState().routes[0]
  const pageOwnership = {
    owner: 'page',
    scope: 'profile_scoped',
    reason: null,
    homeIntegration: { present: false, entry: false, artifact: false },
    conflict: null,
    mode: 'auto',
  }
  const cases = [
    { name: 'invalid config', connect: false, state: seatState({ config: { state: 'invalid', revision: 'x', errors: [] } }) },
    { name: 'unavailable config', connect: false, state: seatState({ config: { state: 'unavailable', revision: 'x', errors: [] } }) },
    {
      name: 'projection failure',
      connect: false,
      state: seatState({ routes: [], target: null, errors: [{ section: 'projection', code: 'projection_unavailable', message: 'no projection' }] }),
    },
    // Nothing declared yet: Apply is exactly the step that declares something.
    { name: 'no routes yet', connect: true, state: seatState({ routes: [] }) },

    { name: 'unknown credential', connect: false, state: seatState({ routes: [{ ...route, credentialConfigured: null }] }) },
    { name: 'page-owned no routes', connect: true, state: seatState({ routes: [], ownership: pageOwnership }) },
    { name: 'known missing credential', connect: true, state: seatState({ routes: [{ ...route, credentialConfigured: false }] }) },
  ]
  for (const item of cases) {
    const page = await mountSeat(withState(item.state), slot('settings.models.footer'))
    try {
      const tree = await page.mount()
      assert.equal(buttonByText(tree, 'connect') !== undefined, item.connect,
        `${item.name}: Apply ${item.connect ? 'is offered' : 'is withheld'}`)
      assert.equal(page.calls.filter(call => call.method === 'POST').length, 0, `${item.name}: no write on mount`)
      assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/observe').length, 0, `${item.name}: no observation`)
    } finally {
      page.restore()
    }
  }
})

test('the Settings footer does not count an unknown credential as unusable', async () => {
  const route = seatState().routes[0]
  const page = await mountSeat(withState(seatState({ routes: [{ ...route, credentialConfigured: null }] })),
    slot('settings.models.footer'))
  try {
    const text = textOf(await page.mount())
    // The route and its liveness are known; only the usable count is unknown.
    assert.match(text, /footerRoutes: 1\/1 footerActive · unknown footerUsable/)
    assert.doesNotMatch(text, /0 usable/)
    assert.match(text, /footerReason: modelsCredentialUnknown/)
    assert.doesNotMatch(text, /footerReason: modelsCredential[^U]/)
    assert.equal(buttonByText(page.current(), 'connect'), undefined, 'unknown readiness offers no credential write')
    assert.equal(page.calls.filter(call => call.method === 'POST').length, 0)
    // The seat says how old the read model it draws is.
    assert.match(text, /updated /)
  } finally {
    page.restore()
  }
})

test('an apply-returned read model is adopted only through the state gate', async () => {
  const pageOwnership = {
    owner: 'page',
    scope: 'profile_scoped',
    reason: null,
    homeIntegration: { present: false, entry: false, artifact: false },
    conflict: null,
    mode: 'auto',
  }
  const empty = seatState({ routes: [], ownership: pageOwnership })
  const cases = [
    {
      name: 'unsupported schema',
      state: { ...seatState(), schemaVersion: 3 },
      pattern: /hostError: schema 3/,
    },
    {
      name: 'refused read model',
      state: { ...seatState(), ok: false, code: 'state_unavailable', message: 'tmax did not answer' },
      pattern: /state_unavailable: tmax did not answer/,
    },
  ]
  for (const item of cases) {
    const page = await mountSeat(async record => {
      const path = pathOf(record)
      if (path === '/_tmax/state') return { status: 200, body: empty }
      if (path === '/_tmax/connect') {
        return { status: 200, body: { ok: true, dryRun: false, connect: { changed: true }, state: item.state } }
      }
      return { status: 404, body: { ok: false } }
    }, slot('settings.models.footer'))
    try {
      await page.mount()
      await page.click('connect')
      const applied = await page.click('connectApply')
      const text = textOf(applied)
      // The write is reported, but the payload it returned is never evidence.
      assert.match(text, /connectApplied/, `${item.name}: the write is reported`)
      assert.match(text, item.pattern, `${item.name}: the rejected read is visible`)
      assert.doesNotMatch(text, /modelsAvailable/, `${item.name}: no model claim`)
      assert.match(text, /footerRoutes: unknown/, `${item.name}: no route claim`)
      assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/state').length, 1,
        `${item.name}: no silent second read`)
    } finally {
      page.restore()
    }
  }
})

test('the Settings footer names a failed gateway section instead of a readiness', async () => {
  const state = seatState({
    errors: [{ section: 'gateway', code: 'gateway_unavailable', message: 'gateway status failed' }],
  })
  const page = await mountSeat(withState(state), slot('settings.models.footer'))
  try {
    const text = textOf(await page.mount())
    // The gateway object may still be present, but a failed read is no evidence.
    assert.match(text, /footerGateway: unknown · footerReadFailed/)
    assert.doesNotMatch(text, /footerGateway: gatewayReady/)
    assert.match(text, /footerRoutes: unknown/)
    assert.match(text, /footerModels: unknown/)
  } finally {
    page.restore()
  }
})

test('a failed footer read reports unknown counts instead of zeros', async () => {
  const page = await mountSeat(async record => {
    if (pathOf(record) === '/_tmax/state') {
      return { status: 502, body: { ok: false, code: 'state_unavailable', message: 'tmax did not answer' } }
    }
    return { status: 404, body: { ok: false } }
  }, slot('settings.models.footer'))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /hostError/)
    assert.match(text, /state_unavailable: tmax did not answer/)
    // No fabricated zero counts from a read that never succeeded.
    assert.match(text, /footerRoutes: unknown/)
    assert.match(text, /footerModels: unknown/)
    assert.doesNotMatch(text, /footerRoutes: 0\/0/)
    assert.equal(buttonByText(tree, 'connect'), undefined, 'no Apply without a read model')
    assert.ok(buttonByText(tree, 'footerReload'), 'an explicit reload is offered')
    assert.equal(page.calls.filter(call => call.method === 'POST').length, 0)
  } finally {
    page.restore()
  }
})

test('the tmax_status card suppresses zero counts on a failed result', async () => {
  const failure = {
    ok: false,
    target: null,
    ownership: null,
    gateway: null,
    routes: [],
    providers: { tmax: [] },
    routing: [],
    diagnostics: { history: { state: 'unavailable', reasonCode: 'tmax_unavailable' }, lastProjectionAt: null, lastError: 'unavailable' },
    errors: [{ section: 'target', code: 'tmax_unavailable', message: 'tmax unavailable' }],
    blockers: [{ source: 'section', code: 'tmax_unavailable', message: 'tmax unavailable' }],
  }
  const page = await mountSeat(withState(seatState()), slot('tool.call.toolview'), {
    phase: 'result',
    block: { kind: 'tool-result', content: [{ type: 'text', text: JSON.stringify(failure) }], isError: false },
  })
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    assert.match(text, /toolFailed/)
    // The unobserved lists are unknown, not "0 routes / 0 models / 0 providers".
    assert.match(text, /toolRoutes: unknown/)
    assert.match(text, /toolModels: unknown/)
    assert.match(text, /toolProviders: unknown/)
    assert.doesNotMatch(text, /toolRoutes: 0/)
    assert.doesNotMatch(text, /toolModels: 0/)
    // The reported blocker is still evidence.
    assert.match(text, /tmax_unavailable: tmax unavailable/)
    assert.equal(page.calls.length, 0)
  } finally {
    page.restore()
  }
})

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------


test('a projection failure is unknown footer evidence, not zero counts', async () => {
  const broken = seatState({
    routes: [],
    errors: [{ section: 'projection', code: 'projection_unavailable', message: 'the route projection failed' }],
  })
  const page = await mountSeat(withState(broken), slot('settings.models.footer'))
  try {
    const tree = await page.mount()
    const text = textOf(tree)
    // A readable state with a failed projection still has no route evidence.
    assert.match(text, /footerRoutes: unknown/)
    assert.match(text, /footerModels: unknown/)
    assert.match(text, /modelsUnknown/)
    assert.doesNotMatch(text, /footerRoutes: 0\/0/)
    assert.doesNotMatch(text, /footerModels: 0/)
    // The projection failure is not Apply evidence, and the honest control is
    // a read retry.
    assert.equal(buttonByText(tree, 'connect'), undefined, 'a failed projection is not Apply evidence')
    assert.ok(buttonByText(tree, 'footerReload'), 'the section read can be retried')
    assert.equal(page.calls.filter(call => call.method === 'POST').length, 0)
    assert.equal(page.calls.filter(call => pathOf(call) === '/_tmax/observe').length, 0)
  } finally {
    page.restore()
  }
})
