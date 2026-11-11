import assert from 'node:assert/strict'
import test from 'node:test'

test('client bundle registers the tmax Plugins page', async () => {
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
    },
    slots: {
      inject(name, fn) { effects.push(name); fn() },
      register(options, component) { registrations.push({ options, component }); return () => {} },
    },
    remote: { session: { modelCatalog: async () => ({ ok: true, value: { groups: [] } }) } },
  }
  module.apply(ctx)
  assert.deepEqual(registrations.map(entry => entry.options.name).sort(), ['plugins.bundle.config', 'plugins.detail.actions'])
  assert.equal(registrations[0].options.key, 'tmax')
  assert.equal(typeof registrations[0].component, 'function')
  delete globalThis.window
})
