import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { afterEach, describe, it } from 'node:test'

import { observeProvider, quotaRowsFor, vendorFor, vendorForKey } from '../quota.js'

const realFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = realFetch
})

/** A fetch double that records the request and answers with one JSON body. */
function stubFetch(body, status = 200) {
  const calls = []
  globalThis.fetch = async (url, init) => {
    calls.push({ url: String(url), init })
    return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })
  }
  return calls
}

const read = (provider, baseURL, key = 'k') => observeProvider({
  provider,
  baseURL,
  apiKeyEnv: 'REF',
  resolveKey: async () => key,
})

describe('vendor matching', () => {
  it('matches a provider by where its models are sent', () => {
    assert.equal(vendorFor('https://api.stepfun.com/step_plan/v1')?.id, 'stepfun-token-plan')
    assert.equal(vendorFor('https://api.z.ai/api/coding/paas/v4')?.id, 'glm-coding-plan')
    assert.equal(vendorFor('https://api.kimi.com/coding/v1')?.id, 'kimi-code-plan-cn')
    assert.equal(vendorFor('https://api.minimaxi.com/v1')?.id, 'minimax-cn-coding-plan')
    assert.equal(vendorFor('https://api.minimax.io/anthropic')?.id, 'minimax-coding-plan')
    assert.equal(vendorFor('https://opencode.ai/inference/go/openai/v1')?.id, 'opencode-go')
  })

  it('requires an exact host and an exact or slash-prefixed path', () => {
    assert.equal(vendorFor('https://api.stepfun.com/step_plan')?.id, 'stepfun-token-plan')
    assert.equal(vendorFor('https://api.stepfun.com/step_plan/')?.id, 'stepfun-token-plan')
    assert.equal(vendorFor('https://api.stepfun.com/step_plan/v1/chat')?.id, 'stepfun-token-plan')
    assert.equal(vendorFor('https://api.stepfun.com/step_planning/v1'), undefined)
    assert.equal(vendorFor('https://api.stepfun.com/v1'), undefined)
    assert.equal(vendorFor('https://api.z.ai/api/coding')?.id, 'glm-coding-plan')
    assert.equal(vendorFor('https://api.z.ai/api/codingx/paas/v4'), undefined)
    assert.equal(vendorFor('https://api.z.ai/api'), undefined)
    assert.equal(vendorFor('https://opencode.ai/zen')?.id, 'opencode-go')
    assert.equal(vendorFor('https://opencode.ai/zenith/v1'), undefined)
    // The host is the parsed hostname (case-folded), the path is exact.
    assert.equal(vendorFor('https://API.STEPFUN.COM/step_plan/v1')?.id, 'stepfun-token-plan')
    assert.equal(vendorFor('https://api.stepfun.com/Step_Plan/v1'), undefined)
    // A host-only allowlist entry accepts any path on that exact host.
    assert.equal(vendorFor('https://api.minimax.io')?.id, 'minimax-coding-plan')
  })

  it('does not match a vendor host that only appears in a proxy URL path', () => {
    assert.equal(vendorFor('https://proxy.example.test/api.minimax.io/v1'), undefined)
    assert.equal(vendorFor('https://proxy.example.test/https://api.stepfun.com/step_plan/v1'), undefined)
    assert.equal(vendorFor('https://proxy.example.test/api.z.ai/api/coding/paas/v4'), undefined)
  })

  it('does not match a host suffix or a vendor subdomain', () => {
    assert.equal(vendorFor('https://api.minimax.io.evil.test/v1'), undefined)
    assert.equal(vendorFor('https://api.kimi.com.evil.test/coding/v1'), undefined)
    assert.equal(vendorFor('https://api.stepfun.com.evil.test/step_plan/v1'), undefined)
    assert.equal(vendorFor('https://evil.api.stepfun.com/step_plan/v1'), undefined)
  })

  it('treats a malformed or non-HTTP base URL as no vendor at all', () => {
    assert.equal(vendorFor('not a url'), undefined)
    assert.equal(vendorFor('api.stepfun.com/step_plan'), undefined)
    assert.equal(vendorFor('ftp://api.stepfun.com/step_plan'), undefined)
    assert.equal(vendorFor(''), undefined)
    assert.equal(vendorFor(undefined), undefined)
  })

  it('identifies a vendor by key for display only', () => {
    assert.equal(vendorForKey('opencode-go')?.id, 'opencode-go')
    assert.equal(vendorForKey('kimi-coding')?.id, 'kimi-code-plan-cn')
    assert.equal(vendorForKey('some-gateway'), undefined)
  })

  it('leaves an unknown vendor unobservable without any request', async () => {
    const calls = stubFetch({})
    const record = await read('longcat', 'https://api.longcat.chat/openai')
    assert.equal(record.reasonCode, 'unsupported_source')
    assert.deepEqual(calls, [])
  })

  it('refuses a vendor-named route whose base URL points elsewhere', async () => {
    const calls = stubFetch({})
    for (const provider of ['stepfun', 'zai-coding-cn', 'kimi-coding', 'minimax', 'opencode-go']) {
      let resolutions = 0
      const record = await observeProvider({
        provider,
        baseURL: 'https://gateway.example.test/v1',
        apiKeyEnv: 'REF',
        resolveKey: async () => {
          resolutions += 1
          return 'k'
        },
      })
      assert.equal(record.reasonCode, 'unsupported_source', provider)
      assert.equal(record.vendor, undefined, provider)
      assert.equal(resolutions, 0, provider)
    }
    assert.deepEqual(calls, [])
  })

  it('refuses to observe a vendor-named route whose base URL is malformed', async () => {
    const calls = stubFetch({})
    const record = await read('stepfun', 'not a url')
    assert.equal(record.reasonCode, 'unsupported_source')
    assert.equal(record.vendor, undefined)
    assert.deepEqual(calls, [])
  })
})

describe('stepfun', () => {
  it('reads credit buckets as a proportion of the total', async () => {
    const calls = stubFetch({
      schema_version: 1,
      plan: { name: 'Token-Pro' },
      quota: {
        state: 'known',
        allowed: true,
        windows: [],
        credit_buckets: [{ type: 'subscription', total: '8000000000', remaining: '7556481546', resets_at_ms: 1795073471000 }],
      },
    })
    const record = await read('stepfun', 'https://api.stepfun.com/step_plan/v1')
    assert.equal(record.reasonCode, undefined)
    assert.equal(record.rows.length, 1)
    assert.deepEqual(record.rows[0].counters, { remaining: '7556481546', total: '8000000000' })
    assert.equal(record.rows[0].basis, 'credit')
    assert.equal(record.rows[0].reset.kind, 'window_end')
    assert.equal(calls[0].init.headers.authorization, 'Bearer k')
  })

  it('refuses a schema it does not recognise instead of guessing', async () => {
    stubFetch({ schema_version: 2, quota: { state: 'known', allowed: true, windows: [] } })
    const record = await read('stepfun', 'https://api.stepfun.com/step_plan/v1')
    assert.equal(record.reasonCode, 'source_format_error')
    assert.equal(record.rows.length, 0)
  })
})

describe('glm', () => {
  it('sends the key raw and keeps the row identity', async () => {
    const calls = stubFetch({
      success: true,
      code: 200,
      data: {
        level: 'max',
        limits: [
          { type: 'TOKENS_LIMIT', unit: 3, number: 5, percentage: 17, nextResetTime: 1793128801192 },
          { type: 'TIME_LIMIT', unit: 5, number: 1, usage: 4000, currentValue: 12, remaining: 3988, percentage: 0 },
        ],
      },
    })
    const record = await read('zai-coding-cn', 'https://api.z.ai/api/coding/paas/v4')
    assert.equal(record.rows.length, 2)
    assert.equal(record.rows[0].percent, 17)
    assert.deepEqual(record.rows[1].counters, { usage: 4000, currentValue: 12, remaining: 3988 })
    assert.equal(calls[0].url, 'https://api.z.ai/api/monitor/usage/quota/limit')
    assert.equal(calls[0].init.headers.authorization, 'k')
  })

  it('treats a business error inside HTTP 200 as a rejection', async () => {
    stubFetch({ success: false, code: 500 })
    const record = await read('zai-coding-cn', 'https://api.z.ai/api/coding/paas/v4')
    assert.equal(record.reasonCode, 'upstream_rejection')
  })

  it('selects the quota host by the parsed host of the base URL', async () => {
    const body = { success: true, code: 200, data: { limits: [{ type: 'TOKENS_LIMIT', unit: 3, number: 5, percentage: 17 }] } }
    const zai = stubFetch(body)
    const zaiRecord = await read('zai-coding-cn', 'https://api.z.ai/api/coding/paas/v4')
    assert.equal(zaiRecord.reasonCode, undefined)
    assert.equal(zai[0].url, 'https://api.z.ai/api/monitor/usage/quota/limit')

    const bigmodel = stubFetch(body)
    const bigmodelRecord = await read('zai-coding-cn', 'https://open.bigmodel.cn/api/coding/paas/v4')
    assert.equal(bigmodelRecord.reasonCode, undefined)
    assert.equal(bigmodel[0].url, 'https://open.bigmodel.cn/api/monitor/usage/quota/limit')
  })
})

describe('kimi', () => {
  it('keeps counter rows as reported, including an overlapping window list', async () => {
    stubFetch({
      usage: { limit: '100', used: '9', remaining: '91', resetTime: '2026-11-06T15:42:16.495050Z' },
      limits: [{ window: { duration: 300, timeUnit: 'TIME_UNIT_MINUTE' }, detail: { limit: '100', remaining: '100' } }],
    })
    const record = await read('kimi-coding', 'https://api.kimi.com/coding/v1')
    assert.equal(record.rows.length, 2)
    assert.equal(record.rows[0].label.key, 'weekly_allowance')
    assert.equal(record.rows[1].label.duration, 300)
    assert.equal(record.rows[1].reset.kind, 'absent')
  })
})

describe('minimax', () => {
  it('gives a video bucket its weekly window only', async () => {
    stubFetch({
      base_resp: { status_code: 0 },
      model_remains: [
        { model_name: 'general', current_interval_remaining_percent: 100, current_weekly_remaining_percent: 40, weekly_end_time: 1794182400000 },
        { model_name: 'video', current_weekly_remaining_percent: 100, weekly_end_time: 1794182400000 },
      ],
    })
    const record = await read('minimax', 'https://api.minimax.io/anthropic')
    assert.deepEqual(record.rows.map(row => `${row.label.name}/${row.label.window}`), ['general/interval', 'general/weekly', 'video/weekly'])
  })
})

describe('opencode-go', () => {
  it('requires all three windows before it reports anything', async () => {
    stubFetch({ usage: { rolling: { status: 'ok', percent: 0, resetsAt: '2026-11-13T01:03:00.000Z' } } })
    const record = await read('opencode-go', 'https://opencode.ai/zen/v1')
    assert.equal(record.reasonCode, 'source_format_error')
  })
})

describe('failure states', () => {
  it('names a missing credential without calling the vendor', async () => {
    const calls = stubFetch({})
    const record = await observeProvider({
      provider: 'stepfun',
      baseURL: 'https://api.stepfun.com/step_plan/v1',
      apiKeyEnv: 'STEPFUN_API_KEY',
      resolveKey: async () => undefined,
    })
    assert.equal(record.reasonCode, 'credential_unavailable')
    assert.equal(record.credentialRef, 'STEPFUN_API_KEY')
    assert.deepEqual(calls, [])
  })

  it('reports a non-2xx as a rejection', async () => {
    stubFetch({}, 502)
    const record = await read('stepfun', 'https://api.stepfun.com/step_plan/v1')
    assert.equal(record.reasonCode, 'upstream_rejection')
  })
})

/** A fetch double that records the request and stays in flight until aborted. */
function pendingFetch() {
  const calls = []
  globalThis.fetch = (url, init) => {
    calls.push({ url: String(url), init })
    return new Promise((resolve, reject) => {
      const signal = init?.signal
      const abort = () => {
        const error = new Error('The operation was aborted')
        error.name = 'AbortError'
        reject(error)
      }
      if (signal?.aborted === true) {
        abort()
        return
      }
      signal?.addEventListener('abort', abort, { once: true })
    })
  }
  return calls
}

/** Wait for an asynchronous predicate without sleeping a fixed duration. */
async function waitFor(predicate) {
  for (let attempt = 0; attempt < 500; attempt += 1) {
    if (predicate()) return
    await new Promise(resolve => setImmediate(resolve))
  }
  throw new Error('condition was not met')
}

describe('scope budget signal', () => {
  it('aborts an in-flight vendor read as observation_deadline', async () => {
    const calls = pendingFetch()
    const controller = new AbortController()
    const pending = observeProvider({
      provider: 'stepfun',
      baseURL: 'https://api.stepfun.com/step_plan/v1',
      apiKeyEnv: 'REF',
      resolveKey: async () => 'k',
      signal: controller.signal,
    })
    await waitFor(() => calls.length === 1)
    assert.equal(calls[0].init.signal.aborted, false)
    controller.abort()
    const record = await pending
    assert.equal(record.reasonCode, 'observation_deadline')
    assert.equal(record.vendor, 'stepfun-token-plan')
    assert.deepEqual(record.rows, [])
    assert.equal(calls[0].init.signal.aborted, true, 'the transport signal is aborted with the scope')
  })

  it('makes no request and resolves no credential once the budget has expired', async () => {
    const calls = pendingFetch()
    const controller = new AbortController()
    controller.abort()
    let resolutions = 0
    const record = await observeProvider({
      provider: 'stepfun',
      baseURL: 'https://api.stepfun.com/step_plan/v1',
      apiKeyEnv: 'REF',
      resolveKey: async () => {
        resolutions += 1
        return 'k'
      },
      signal: controller.signal,
    })
    assert.equal(record.reasonCode, 'observation_deadline')
    assert.equal(resolutions, 0)
    assert.deepEqual(calls, [])
  })

  it('keeps an unsupported source unsupported when the budget has expired', async () => {
    const calls = pendingFetch()
    const controller = new AbortController()
    controller.abort()
    let resolutions = 0
    const record = await observeProvider({
      provider: 'kimi-coding',
      baseURL: 'https://gateway.example.test/v1',
      apiKeyEnv: 'REF',
      resolveKey: async () => {
        resolutions += 1
        return 'k'
      },
      signal: controller.signal,
    })
    assert.equal(record.reasonCode, 'unsupported_source')
    assert.equal(resolutions, 0)
    assert.deepEqual(calls, [])
  })

  it('reads normally while the scope signal stays live', async () => {
    const calls = stubFetch({
      schema_version: 1,
      quota: { state: 'known', allowed: true, windows: [], credit_buckets: [{ type: 'subscription', total: '100', remaining: '40' }] },
    })
    const controller = new AbortController()
    const record = await observeProvider({
      provider: 'stepfun',
      baseURL: 'https://api.stepfun.com/step_plan/v1',
      apiKeyEnv: 'REF',
      resolveKey: async () => 'k',
      signal: controller.signal,
    })
    assert.equal(record.reasonCode, undefined)
    assert.equal(record.rows.length, 1)
    assert.equal(calls[0].init.signal instanceof AbortSignal, true)
    assert.equal(calls[0].init.signal.aborted, false)
  })
})

describe('shared contract', () => {
  const contract = JSON.parse(readFileSync(new URL('./fixtures/quota-contract.json', import.meta.url), 'utf8'))

  for (const testCase of contract.cases) {
    it(`projects ${testCase.name} exactly as tmax does`, () => {
      const read = quotaRowsFor(testCase.vendor, testCase.body)
      assert.equal(read.error, undefined, read.error)
      assert.deepEqual(read.rows, testCase.rows)
    })
  }
})
