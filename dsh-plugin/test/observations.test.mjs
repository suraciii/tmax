// Explicit agent-scope observation: one bounded, ordered batch.
//
// The route owns when an observation may run; this module owns how many vendor
// reads that batch may start, how long the whole scope may take, and what every
// provider reports when the budget runs out. These tests use a fake settings /
// credential context and a fake fetch so they need no DSH host and no network.
import assert from 'node:assert/strict'
import { afterEach, describe, it } from 'node:test'

import {
  AGENT_OBSERVE_CONCURRENCY,
  AGENT_OBSERVE_DEADLINE_MS,
  createObservations,
} from '../host/observations.js'

const realFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = realFetch
})

const GLM_QUOTA_BODY = {
  success: true,
  code: 200,
  data: { level: 'pro', limits: [{ type: 'TOKENS_LIMIT', unit: 5, number: 1, percentage: 37 }] },
}

const STEPFUN_QUOTA_BODY = {
  schema_version: 1,
  plan: { name: 'Token-Pro' },
  quota: {
    state: 'known',
    allowed: true,
    windows: [],
    credit_buckets: [{ type: 'subscription', total: '100', remaining: '40' }],
  },
}

const GLM_BASE = 'https://open.bigmodel.cn/api/coding/paas/v4'

/** Wait for an asynchronous predicate without sleeping a fixed duration. */
async function waitFor(predicate) {
  for (let attempt = 0; attempt < 500; attempt += 1) {
    if (predicate()) return
    await new Promise(resolve => setImmediate(resolve))
  }
  throw new Error('condition was not met')
}

/** A settings/credential context whose providers are the given descriptors. */
function fakeContext(providers, { resolve } = {}) {
  const value = { providers: {} }
  const directory = []
  for (const provider of providers) {
    value.providers[provider.provider] = {
      apiKeyEnv: provider.apiKeyEnv,
      baseURL: provider.baseURL,
      models: [{ id: `${provider.provider}-model` }],
    }
    directory.push({
      provider: provider.provider,
      displayName: provider.displayName ?? provider.provider,
      defaultBaseURL: provider.baseURL,
      settingsNs: 'llm-test',
      settingsPath: ['providers', provider.provider],
      declared: true,
    })
  }
  const credentials = { resolve: resolve ?? (async ref => ({ value: `secret:${ref}` })) }
  return {
    get(name) {
      if (name === 'settings') return { describe: () => [{ ns: 'llm-test', value }] }
      if (name === 'credentials') return credentials
      return undefined
    },
    llm: { listConfigurableProviders: () => directory },
  }
}

/** Distinct provider identities that all read the same GLM vendor endpoint. */
function glmProviders(count) {
  return Array.from({ length: count }, (unused, index) => ({
    provider: `glm-${index + 1}`,
    baseURL: GLM_BASE,
    apiKeyEnv: `GLM_KEY_${index + 1}`,
  }))
}

/** A fetch double that answers with one vendor body and counts calls. */
function countingFetch(body = GLM_QUOTA_BODY) {
  const calls = []
  globalThis.fetch = async (url, init) => {
    calls.push({ url: String(url), init })
    return new Response(JSON.stringify(body), { status: 200 })
  }
  return calls
}

/** A fetch double that stays in flight until its request signal aborts. */
function hangingFetch(calls = []) {
  globalThis.fetch = (url, init) => {
    calls.push({ url: String(url), init })
    return new Promise((resolve, reject) => {
      const abort = () => {
        const error = new Error('The operation was aborted')
        error.name = 'AbortError'
        reject(error)
      }
      if (init?.signal?.aborted === true) {
        abort()
        return
      }
      init?.signal?.addEventListener('abort', abort, { once: true })
    })
  }
  return calls
}

describe('agent observation batch', () => {
  it('ships a bounded concurrency and a whole-scope budget', () => {
    assert.ok(AGENT_OBSERVE_CONCURRENCY >= 2 && AGENT_OBSERVE_CONCURRENCY <= 4)
    assert.ok(AGENT_OBSERVE_DEADLINE_MS >= 45_000 && AGENT_OBSERVE_DEADLINE_MS <= 60_000)
  })

  it('bounds the concurrent vendor reads and keeps the discovered order', async () => {
    const providers = glmProviders(6)
    const observations = createObservations({ ctx: fakeContext(providers) })
    const gate = { open: false }
    const waiting = []
    let calls = 0
    let inFlight = 0
    let maxInFlight = 0
    globalThis.fetch = () => {
      calls += 1
      inFlight += 1
      maxInFlight = Math.max(maxInFlight, inFlight)
      return new Promise(resolve => {
        const finish = () => {
          inFlight -= 1
          resolve(new Response(JSON.stringify(GLM_QUOTA_BODY), { status: 200 }))
        }
        if (gate.open) finish()
        else waiting.push(finish)
      })
    }

    const pending = observations.observeAgent({ concurrency: 3 })
    await waitFor(() => calls === 3)
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(calls, 3, 'a fourth read does not start before one finishes')
    assert.equal(maxInFlight, 3)
    gate.open = true
    for (const finish of waiting.splice(0)) finish()

    const result = await pending
    assert.equal(calls, 6)
    assert.equal(maxInFlight, 3, 'the limit held for the whole batch')
    assert.deepEqual(result.items.map(item => item.id), providers.map(provider => `dsh:${provider.provider}`))
    assert.ok(result.items.every(item => item.status === 'refreshed'))
  })

  it('reads once per shared vendor-and-credential identity', async () => {
    const providers = [
      { provider: 'glm-one', baseURL: GLM_BASE, apiKeyEnv: 'GLM_KEY' },
      { provider: 'glm-two', baseURL: GLM_BASE, apiKeyEnv: 'GLM_KEY' },
      { provider: 'glm-three', baseURL: GLM_BASE, apiKeyEnv: 'OTHER_KEY' },
    ]
    const observations = createObservations({ ctx: fakeContext(providers) })
    const calls = countingFetch()
    const result = await observations.observeAgent()
    assert.equal(calls.length, 2, 'two identities mean two vendor reads')
    assert.deepEqual(result.items.map(item => item.id), ['dsh:glm-one', 'dsh:glm-two', 'dsh:glm-three'])
    assert.ok(result.items.every(item => item.status === 'refreshed'))
    const records = observations.providerRecords().providers
    assert.deepEqual(records.map(record => record.rows[0].percent), [37, 37, 37])
  })

  it('expires the scope: in-flight reads abort and queued providers fail with observation_deadline', async () => {
    const providers = glmProviders(3)
    const observations = createObservations({ ctx: fakeContext(providers) })
    const calls = hangingFetch()
    const result = await observations.observeAgent({ concurrency: 2, deadlineMs: 30 })

    assert.equal(calls.length, 2, 'a provider still queued at expiry is never contacted')
    assert.ok(calls.every(call => call.init.signal.aborted === true), 'in-flight requests were aborted')
    assert.deepEqual(result.items.map(item => [item.id, item.status, item.code]), [
      ['dsh:glm-1', 'failed', 'observation_deadline'],
      ['dsh:glm-2', 'failed', 'observation_deadline'],
      ['dsh:glm-3', 'failed', 'observation_deadline'],
    ])
    const records = observations.providerRecords().providers
    assert.ok(records.every(record => record.state === 'failed'))
    assert.ok(records.every(record => record.reasonCode === 'observation_deadline'))
    assert.ok(records.every(record => record.rows.length === 0))
  })

  it('keeps readings that finished before the budget expired', async () => {
    const providers = [
      { provider: 'stepfun-fast', baseURL: 'https://api.stepfun.com/step_plan/v1', apiKeyEnv: 'FAST' },
      { provider: 'glm-slow', baseURL: GLM_BASE, apiKeyEnv: 'SLOW' },
    ]
    const observations = createObservations({ ctx: fakeContext(providers) })
    globalThis.fetch = (url, init) => {
      if (init.headers.authorization.includes('FAST')) {
        return Promise.resolve(new Response(JSON.stringify(STEPFUN_QUOTA_BODY), { status: 200 }))
      }
      return new Promise((resolve, reject) => {
        init.signal.addEventListener('abort', () => {
          const error = new Error('The operation was aborted')
          error.name = 'AbortError'
          reject(error)
        }, { once: true })
      })
    }
    const result = await observations.observeAgent({ concurrency: 2, deadlineMs: 40 })
    assert.deepEqual(result.items.map(item => [item.id, item.status, item.code]), [
      ['dsh:stepfun-fast', 'refreshed', undefined],
      ['dsh:glm-slow', 'failed', 'observation_deadline'],
    ])
    const records = observations.providerRecords().providers
    assert.equal(records[0].state, 'observed')
    assert.equal(records[0].rows[0].counters.remaining, '40')
    assert.equal(records[1].state, 'failed')
    assert.deepEqual(records[1].rows, [])
  })

  it('never lets a timed-out attempt replace the cached reading', async () => {
    const provider = { provider: 'glm-cn', baseURL: GLM_BASE, apiKeyEnv: 'GLM_KEY' }
    const observations = createObservations({ ctx: fakeContext([provider]) })
    let mode = 'ok'
    globalThis.fetch = (url, init) => {
      if (mode === 'ok') return Promise.resolve(new Response(JSON.stringify(GLM_QUOTA_BODY), { status: 200 }))
      return new Promise((resolve, reject) => {
        init.signal.addEventListener('abort', () => {
          const error = new Error('The operation was aborted')
          error.name = 'AbortError'
          reject(error)
        }, { once: true })
      })
    }

    const first = await observations.observeAgent()
    assert.equal(first.items[0].status, 'refreshed')
    const good = observations.providerRecords().providers[0]
    assert.equal(good.state, 'observed')
    assert.equal(good.rows[0].percent, 37)
    assert.equal(typeof good.observedAt, 'string')

    mode = 'hang'
    const second = await observations.observeAgent({ deadlineMs: 30 })
    assert.equal(second.items[0].status, 'failed')
    assert.equal(second.items[0].code, 'observation_deadline')
    const after = observations.providerRecords().providers[0]
    assert.equal(after.state, 'failed')
    assert.equal(after.observedAt, good.observedAt, 'the reading time survives the timed-out attempt')
    assert.equal(after.rows.length, 1, 'the last reading survives')
    assert.equal(after.rows[0].percent, 37)
    assert.deepEqual(after.plan, { level: 'pro' })
    assert.equal(after.reasonCode, 'observation_deadline')
  })

  it('dry run performs no request, resolves no credential, and reports unchanged items', async () => {
    const providers = glmProviders(2)
    let resolutions = 0
    const observations = createObservations({
      ctx: fakeContext(providers, {
        resolve: async () => {
          resolutions += 1
          return { value: 'secret' }
        },
      }),
    })
    const calls = countingFetch()
    const result = await observations.observeAgent({ dryRun: true, deadlineMs: 5, concurrency: 1 })
    assert.deepEqual(result.items, [
      { source: 'agent', id: 'dsh:glm-1', status: 'unchanged', code: 'dry_run', message: 'dry run: no vendor request was made' },
      { source: 'agent', id: 'dsh:glm-2', status: 'unchanged', code: 'dry_run', message: 'dry run: no vendor request was made' },
    ])
    assert.equal(calls.length, 0)
    assert.equal(resolutions, 0)
    assert.ok(observations.providerRecords().providers.every(record => record.state === 'not_observed'))
  })

  it('discards a vendor answer that arrives after the budget expired', async () => {
    const provider = { provider: 'glm-cn', baseURL: GLM_BASE, apiKeyEnv: 'GLM_KEY' }
    const observations = createObservations({ ctx: fakeContext([provider]) })
    let release
    globalThis.fetch = () => new Promise(resolve => {
      release = () => resolve(new Response(JSON.stringify(GLM_QUOTA_BODY), { status: 200 }))
    })
    const result = await observations.observeAgent({ deadlineMs: 20 })
    assert.equal(result.items[0].status, 'failed')
    assert.equal(result.items[0].code, 'observation_deadline')
    release()
    await new Promise(resolve => setTimeout(resolve, 10))
    const after = observations.providerRecords().providers[0]
    assert.equal(after.state, 'failed', 'a late answer is not an observed reading')
    assert.deepEqual(after.rows, [])
  })

  it('settles cleanly when a vendor rejects after the budget expired', async () => {
    const provider = { provider: 'glm-cn', baseURL: GLM_BASE, apiKeyEnv: 'GLM_KEY' }
    const observations = createObservations({ ctx: fakeContext([provider]) })
    let rejectFetch
    globalThis.fetch = () => new Promise((resolve, reject) => {
      rejectFetch = reject
    })
    const result = await observations.observeAgent({ deadlineMs: 15 })
    assert.equal(result.items[0].code, 'observation_deadline')
    const error = new Error('late transport failure')
    error.name = 'AbortError'
    rejectFetch(error)
    // The observation result was already discarded; the late rejection must not
    // surface as an unhandled rejection or throw from this test.
    await new Promise(resolve => setTimeout(resolve, 10))
    assert.equal(observations.providerRecords().providers[0].state, 'failed')
  })
})
