import assert from 'node:assert/strict'
import test from 'node:test'

test('client bundle registers one read-only tmax settings section', async () => {
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
  assert.deepEqual(module.inject, ['slots', 'locale', 'remote'])
  const registrations = []
  const effects = []
  const ctx = {
    effect(fn) { effects.push(fn()) },
    locale: {
      register() { return () => {} },
      bind() { return key => key },
    },
    slots: {
      inject(name, fn) { assert.equal(name, 'settings.section'); fn() },
      register(options, component) { registrations.push({ options, component }); return () => {} },
    },
    remote: { session: { modelCatalog: async () => ({ ok: true, value: { groups: [] } }) } },
  }
  module.apply(ctx)
  assert.equal(registrations.length, 1)
  assert.equal(registrations[0].options.name, 'settings.section')
  assert.equal(registrations[0].options.id, 'tmax')
  assert.equal(registrations[0].options.order, 40)
  assert.equal(typeof registrations[0].component, 'function')
  delete globalThis.window
})
