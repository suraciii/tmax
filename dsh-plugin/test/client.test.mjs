import assert from 'node:assert/strict'
import test from 'node:test'

test('client bundle registers the tmax page and its sidebar entry', async () => {
  let loaded
  globalThis.window = { __ModuleLoader__: { load: entry => { loaded = entry } } }
  const React = {
    createElement: (type, props, ...children) => ({ type, props, children }),
    useState: value => [value, () => {}],
    useEffect: () => {},
    useCallback: fn => fn,
  }
  await import('../client.js')
  assert.equal(loaded.id, 'tmax')
  const module = loaded.factory(specifier => {
    assert.equal(specifier, 'react')
    return React
  })
  assert.deepEqual(module.inject, ['slots', 'locale', 'remote', 'remote.session'])
  const registrations = []
  const effects = []
  const ctx = {
    effect(fn) { effects.push(fn()) },
    locale: {
      register() { return () => {} },
      bind() { return key => key },
      subscribe() { return () => {} },
    },
    slots: {
      inject(name, fn) { effects.push(name); fn() },
      register(options, component) { registrations.push({ options, component }); return () => {} },
    },
    remote: { session: { modelCatalog: async () => ({ ok: true, value: { groups: [] } }) } },
  }
  module.apply(ctx)
  assert.deepEqual(registrations.map(entry => entry.options.name), ['sidebar.panellist', 'main'])
  assert.equal(registrations[0].options.id, 'tmax')
  assert.equal(registrations[0].options.order, 20)
  assert.equal(registrations[1].options.key, 'tmax')
  assert.equal(registrations[1].component('') && typeof registrations[1].component, 'function')
  delete globalThis.window
})
