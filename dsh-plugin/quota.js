// Per-vendor quota reads for providers that live in the agent's own model
// configuration rather than in tmax.
//
// tmax remains the authority for the providers it manages. This module answers
// the other half of the question — "what is left on the subscriptions I
// configured directly in DSH" — without copying tmax's provider list, without
// writing to the DSH profile, and without ever returning a credential.
//
// Each vendor is implemented separately because a subscription quota is not a
// protocol: every vendor publishes its own endpoint, its own auth style, and
// its own idea of what a "window" is. A vendor we do not know is reported as
// not observable rather than guessed at.

const USER_AGENT = 'tmax-dsh-plugin'

/**
 * One entry per vendor tmax already supports. `base` lists the hosts a
 * provider's inference configuration can point at, so a DSH provider is
 * matched to a vendor by where it sends models — never by its name.
 */
const VENDORS = [
  {
    id: 'stepfun-token-plan',
    keys: ['stepfun'],
    name: 'StepFun Step Plan',
    base: ['api.stepfun.com/step_plan'],
    auth: 'bearer',
    quotaUrl: 'https://api.stepfun.com/step_plan/v1/usage',
    read: readStepfun,
  },
  {
    id: 'glm-coding-plan',
    keys: ['zai-coding-cn', 'glm-coding-plan'],
    name: 'GLM Coding Plan',
    base: ['open.bigmodel.cn/api/coding', 'api.z.ai/api/coding'],
    auth: 'raw',
    quotaUrl: 'https://open.bigmodel.cn/api/monitor/usage/quota/limit',
    altQuotaUrl: ['api.z.ai', 'https://api.z.ai/api/monitor/usage/quota/limit'],
    read: readGlm,
  },
  {
    id: 'kimi-code-plan-cn',
    keys: ['kimi-coding'],
    name: 'Kimi Coding Plan',
    base: ['api.kimi.com/coding'],
    auth: 'bearer',
    quotaUrl: 'https://api.kimi.com/coding/v1/usages',
    read: readKimi,
  },
  {
    id: 'kimi-code-plan-global',
    name: 'Kimi Coding Plan (Global)',
    base: ['api.kimi.ai/coding'],
    auth: 'bearer',
    quotaUrl: 'https://api.kimi.ai/coding/v1/usages',
    read: readKimi,
  },
  {
    id: 'minimax-cn-coding-plan',
    keys: ['minimax-cn'],
    name: 'MiniMax Coding Plan',
    base: ['api.minimaxi.com'],
    auth: 'bearer',
    quotaUrl: 'https://api.minimaxi.com/v1/token_plan/remains',
    read: readMinimax,
  },
  {
    id: 'minimax-coding-plan',
    keys: ['minimax'],
    name: 'MiniMax Coding Plan (Global)',
    base: ['api.minimax.io'],
    auth: 'bearer',
    quotaUrl: 'https://api.minimax.io/v1/token_plan/remains',
    read: readMinimax,
  },
  {
    id: 'opencode-go',
    keys: ['opencode-go', 'opencode'],
    name: 'OpenCode Go',
    base: ['opencode.ai/inference/go', 'opencode.ai/zen'],
    auth: 'bearer',
    quotaUrl: 'https://opencode.ai/zen/go/v1/usage',
    read: readOpencodeGo,
  },
]

/** The vendor whose inference host a base URL belongs to, if any. */
export function vendorForKey(key) {
  if (typeof key !== 'string') return undefined
  return VENDORS.find(vendor => (vendor.keys ?? []).includes(key))
}

export function vendorFor(baseURL) {
  if (typeof baseURL !== 'string' || baseURL === '') return undefined
  const host = hostAndPath(baseURL)
  return VENDORS.find(vendor => vendor.base.some(prefix => host.includes(prefix)))
}

function hostAndPath(url) {
  try {
    const parsed = new URL(url)
    return `${parsed.host}${parsed.pathname}`
  } catch {
    return ''
  }
}

/** The destination a vendor's quota read must use for this host. */
function quotaUrlFor(vendor, baseURL) {
  if (vendor.altQuotaUrl !== undefined && hostAndPath(baseURL).startsWith(vendor.altQuotaUrl[0])) {
    return vendor.altQuotaUrl[1]
  }
  return vendor.quotaUrl
}

// ---------------------------------------------------------------------------
// Transport: one GET, no redirects, no retries, bounded body and deadline.
// A quota read is a read; a 3xx or a slow vendor must not become a silent
// second request or an unbounded read.

const BODY_LIMIT = 64 * 1024
const DEADLINE_MS = 30_000

async function requestQuota(vendor, key, baseURL) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), DEADLINE_MS)
  try {
    const response = await fetch(quotaUrlFor(vendor, baseURL), {
      method: 'GET',
      redirect: 'manual',
      signal: controller.signal,
      headers: {
        authorization: vendor.auth === 'bearer' ? `Bearer ${key}` : key,
        accept: 'application/json',
        'user-agent': USER_AGENT,
      },
    })
    if (response.status < 200 || response.status >= 300) {
      return { error: `upstream ${response.status}`, reasonCode: 'upstream_rejection' }
    }
    const text = await readBounded(response)
    if (text === undefined) return { error: 'response too large', reasonCode: 'source_format_error' }
    let body
    try {
      body = JSON.parse(text)
    } catch {
      return { error: 'response is not JSON', reasonCode: 'source_format_error' }
    }
    return { body }
  } catch (error) {
    const aborted = error instanceof Error && error.name === 'AbortError'
    return {
      error: aborted ? 'vendor did not answer in time' : 'vendor is unreachable',
      reasonCode: aborted ? 'observation_deadline' : 'transport_error',
    }
  } finally {
    clearTimeout(timer)
  }
}

async function readBounded(response) {
  if (response.body === null) return ''
  const reader = response.body.getReader()
  const chunks = []
  let size = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    size += value.byteLength
    if (size > BODY_LIMIT) {
      void reader.cancel()
      return undefined
    }
    chunks.push(value)
  }
  const merged = new Uint8Array(size)
  let offset = 0
  for (const chunk of chunks) {
    merged.set(chunk, offset)
    offset += chunk.byteLength
  }
  return new TextDecoder().decode(merged)
}

// ---------------------------------------------------------------------------
// Vendors. Each returns rows in the same shape tmax's own projection uses, so
// one renderer draws both sources.

const isObject = value => typeof value === 'object' && value !== null && !Array.isArray(value)
const decimal = value => typeof value === 'string' && /^\d+(\.\d+)?$/.test(value)
const count = value => (Number.isInteger(value) && value >= 0 ? value : undefined)

function epochReset(raw) {
  return Number.isInteger(raw) && raw > 0
    ? { kind: 'window_end', raw, encoding: 'epoch_milliseconds' }
    : { kind: 'absent' }
}

function percentRow(label, basis, percent, reset, extra = {}) {
  return { label, basis, percent, reset, ...extra }
}

/** StepFun: a versioned envelope whose schema mismatch means "do not read". */
function readStepfun(body) {
  if (!isObject(body) || body.schema_version !== 1) return { error: 'unsupported schema', reasonCode: 'source_format_error' }
  const quota = body.quota
  if (!isObject(quota)) return { error: 'no quota in response', reasonCode: 'source_format_error' }
  if (quota.state !== 'known' || quota.allowed !== true || (quota.reason !== undefined && quota.reason !== null)) {
    return { error: 'vendor reports the plan is not usable', reasonCode: 'observation_failed' }
  }
  if (!Array.isArray(quota.windows) || quota.windows.length > 0) {
    return { error: 'unexpected window shape', reasonCode: 'source_format_error' }
  }
  const buckets = quota.credit_buckets
  if (!Array.isArray(buckets) || buckets.length === 0 || buckets.length > 8) {
    return { error: 'no credit bucket', reasonCode: 'source_format_error' }
  }
  const rows = []
  for (const bucket of buckets) {
    if (!isObject(bucket) || !decimal(bucket.total) || !decimal(bucket.remaining)) {
      return { error: 'credit bucket is not a pair of amounts', reasonCode: 'source_format_error' }
    }
    const row = {
      label: { key: 'credit', kind: typeof bucket.type === 'string' ? bucket.type : undefined },
      basis: 'credit',
      counters: { remaining: bucket.remaining, total: bucket.total },
      reset: epochReset(bucket.resets_at_ms),
    }
    if (Number.isInteger(bucket.expires_at_ms)) {
      row.expiry = { raw: bucket.expires_at_ms, encoding: 'epoch_milliseconds' }
    }
    rows.push(row)
  }
  return { rows, plan: typeof body.plan?.name === 'string' ? { name: body.plan.name } : undefined, notes: [] }
}

/** GLM: HTTP 200 carries a business error, and rows label themselves by enum. */
function readGlm(body) {
  if (!isObject(body) || body.success !== true || body.code !== 200) return { error: 'vendor rejected the read', reasonCode: 'upstream_rejection' }
  const data = body.data
  if (!isObject(data) || !Array.isArray(data.limits) || data.limits.length === 0 || data.limits.length > 16) {
    return { error: 'unexpected limit list', reasonCode: 'source_format_error' }
  }
  const seen = new Set()
  const rows = []
  for (const limit of data.limits) {
    if (!isObject(limit)) return { error: 'limit row is not an object', reasonCode: 'source_format_error' }
    const type = limit.type
    const unit = count(limit.unit)
    const number = count(limit.number)
    if (typeof type !== 'string' || unit === undefined || number === undefined) {
      return { error: 'limit row is missing its identity', reasonCode: 'source_format_error' }
    }
    const identity = `${type}/${unit}/${number}`
    if (seen.has(identity)) return { error: 'limit rows repeat', reasonCode: 'source_format_error' }
    seen.add(identity)
    const percent = limit.percentage === undefined ? undefined : count(limit.percentage)
    if (limit.percentage !== undefined && percent === undefined) {
      return { error: 'limit percentage is not a whole number', reasonCode: 'source_format_error' }
    }
    const counters = ['usage', 'currentValue', 'remaining'].map(key => limit[key])
    const hasCounters = counters.every(value => Number.isInteger(value) && value >= 0)
    const partial = counters.some(value => value !== undefined) && !hasCounters
    if (partial) return { error: 'limit counters are incomplete', reasonCode: 'source_format_error' }
    if (percent === undefined && !hasCounters) {
      return { error: 'limit row carries no reading', reasonCode: 'source_format_error' }
    }
    const row = {
      label: { key: 'limit', type, unit, number },
      basis: percent !== undefined ? 'consumed_percent' : 'counters',
      reset: epochReset(limit.nextResetTime),
    }
    if (percent !== undefined) row.percent = percent
    if (hasCounters) row.counters = { usage: counters[0], currentValue: counters[1], remaining: counters[2] }
    rows.push(row)
  }
  return { rows, plan: typeof data.level === 'string' ? { level: data.level } : undefined }
}

/** Kimi: exact decimal strings, partial data, and an overlapping `limits`. */
function readKimi(body) {
  if (!isObject(body)) return { error: 'response is not an object', reasonCode: 'source_format_error' }
  const rows = []
  const countersOf = source => {
    const counters = {}
    if (decimal(source.limit)) counters.limit = source.limit
    if (decimal(source.used)) counters.used = source.used
    if (decimal(source.remaining)) counters.remaining = source.remaining
    return Object.keys(counters).length === 0 ? undefined : counters
  }
  const resetOf = source => (typeof source.resetTime === 'string' && source.resetTime !== ''
    ? { kind: 'provider_timestamp', at: source.resetTime }
    : { kind: 'absent' })
  if (isObject(body.usage)) {
    const counters = countersOf(body.usage)
    if (counters !== undefined) rows.push({ label: { key: 'weekly_allowance' }, basis: 'counters', counters, reset: resetOf(body.usage) })
  }
  if (Array.isArray(body.limits)) {
    for (const entry of body.limits) {
      if (!isObject(entry) || !isObject(entry.detail)) continue
      const counters = countersOf(entry.detail)
      if (counters === undefined) continue
      const window = isObject(entry.window) ? entry.window : {}
      const duration = count(window.duration)
      rows.push({
        label: duration === undefined
          ? { key: 'duration' }
          : { key: 'duration', duration, unit: typeof window.timeUnit === 'string' ? window.timeUnit : undefined },
        basis: 'counters',
        counters,
        reset: resetOf(entry.detail),
      })
    }
  }
  if (rows.length === 0) return { error: 'no counter in the response', reasonCode: 'source_format_error' }
  return { rows }
}

/** MiniMax: remaining percent per bucket, statuses kept but never interpreted. */
function readMinimax(body) {
  if (!isObject(body) || !isObject(body.base_resp) || body.base_resp.status_code !== 0) {
    return { error: 'vendor rejected the read', reasonCode: 'upstream_rejection' }
  }
  const buckets = body.model_remains
  if (!Array.isArray(buckets) || buckets.length === 0 || buckets.length > 16) {
    return { error: 'unexpected bucket list', reasonCode: 'source_format_error' }
  }
  const seen = new Set()
  const rows = []
  for (const bucket of buckets) {
    if (!isObject(bucket) || typeof bucket.model_name !== 'string' || bucket.model_name === '') {
      return { error: 'bucket has no name', reasonCode: 'source_format_error' }
    }
    if (seen.has(bucket.model_name)) return { error: 'bucket names repeat', reasonCode: 'source_format_error' }
    seen.add(bucket.model_name)
    const video = bucket.model_name === 'video'
    const windows = video
      ? [['weekly', 'current_weekly_remaining_percent', 'current_weekly_status', 'weekly_end_time']]
      : [
          ['interval', 'current_interval_remaining_percent', 'current_interval_status', 'end_time'],
          ['weekly', 'current_weekly_remaining_percent', 'current_weekly_status', 'weekly_end_time'],
        ]
    for (const [window, percentKey, statusKey, endKey] of windows) {
      if (bucket[percentKey] === undefined) continue
      const percent = count(bucket[percentKey])
      if (percent === undefined) return { error: 'remaining percent is not whole', reasonCode: 'source_format_error' }
      const status = count(bucket[statusKey])
      rows.push(percentRow(
        { key: 'bucket', name: bucket.model_name, window },
        'remaining_percent',
        percent,
        epochReset(bucket[endKey]),
        status === undefined ? {} : { status },
      ))
    }
  }
  if (rows.length === 0) return { error: 'no window in the response', reasonCode: 'source_format_error' }
  return { rows }
}

/** OpenCode Go: three required windows, consumed percentages, announced resets. */
function readOpencodeGo(body) {
  if (!isObject(body) || !isObject(body.usage)) return { error: 'response has no usage', reasonCode: 'source_format_error' }
  const rows = []
  for (const window of ['rolling', 'weekly', 'monthly']) {
    const entry = body.usage[window]
    if (!isObject(entry)) return { error: `${window} window is missing`, reasonCode: 'source_format_error' }
    if (entry.status !== 'ok' && entry.status !== 'rate-limited') {
      return { error: `${window} window has an unknown status`, reasonCode: 'source_format_error' }
    }
    const percent = count(entry.percent)
    if (percent === undefined) return { error: `${window} percent is not whole`, reasonCode: 'source_format_error' }
    const reset = typeof entry.resetsAt === 'string' && /T.*Z$/.test(entry.resetsAt)
      ? { kind: 'announced', at: entry.resetsAt }
      : { kind: 'absent' }
    rows.push({ label: { key: window }, basis: 'consumed_percent', percent, status: entry.status, reset })
  }
  return { rows }
}

// ---------------------------------------------------------------------------
// Reading one DSH-configured provider.

/**
 * Observe one provider that exists only in the agent's configuration.
 * @returns a normalized provider record, or nothing when the vendor is unknown.
 */
export async function observeProvider({ provider, baseURL, apiKeyEnv, resolveKey }, now = new Date()) {
  const vendor = vendorFor(baseURL) ?? vendorForKey(provider)
  if (vendor === undefined) {
    return {
      id: `dsh:${provider}`,
      name: provider,
      source: 'dsh',
      vendor: undefined,
      baseURL,
      observedAt: now.toISOString(),
      reasonCode: 'unsupported_source',
      rows: [],
      windows: [],
    }
  }
  let key
  try {
    key = apiKeyEnv === undefined ? undefined : await resolveKey(apiKeyEnv)
  } catch {
    key = undefined
  }
  if (typeof key !== 'string' || key === '') {
    return {
      id: `dsh:${provider}`,
      name: provider,
      source: 'dsh',
      vendor: vendor.id,
      vendorName: vendor.name,
      baseURL,
      observedAt: now.toISOString(),
      reasonCode: 'credential_unavailable',
      credentialRef: apiKeyEnv,
      rows: [],
      windows: [],
    }
  }
  const result = await requestQuota(vendor, key, baseURL)
  if (result.body === undefined) {
    return {
      id: `dsh:${provider}`,
      name: provider,
      source: 'dsh',
      vendor: vendor.id,
      vendorName: vendor.name,
      baseURL,
      observedAt: now.toISOString(),
      reasonCode: result.reasonCode,
      message: result.error,
      rows: [],
      windows: [],
    }
  }
  const read = vendor.read(result.body)
  if (read.error !== undefined) {
    return {
      id: `dsh:${provider}`,
      name: provider,
      source: 'dsh',
      vendor: vendor.id,
      vendorName: vendor.name,
      baseURL,
      observedAt: now.toISOString(),
      reasonCode: read.reasonCode,
      message: read.error,
      rows: [],
      windows: [],
    }
  }
  return {
    id: `dsh:${provider}`,
    name: provider,
    source: 'dsh',
    vendor: vendor.id,
    vendorName: vendor.name,
    baseURL,
    observedAt: now.toISOString(),
    plan: read.plan,
    rows: read.rows,
    windows: read.rows.length,
  }
}
