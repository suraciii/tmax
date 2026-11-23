// Read-only request-history evidence for the page and the `/tmax history`
// command.
//
// tmax keeps bounded, redacted, in-memory request metadata. One `history list`
// read answers both surfaces: the host maps an allowlist of compact fields,
// caps the rows, and reports omission instead of returning a body archive. A
// typed tmax blocker or a no-daemon-history answer is availability evidence
// (HTTP 200, ok:false), never an empty history; only a missing transport
// envelope is a 502.
import { envelopeMessage, readEnvelope } from './cli.js'

// A retained-history read waits on daemon identity plus the control RPC and an
// owned process start, so it gets startup/process margin; it still starts no
// service itself.
export const HISTORY_READ_TIMEOUT_MS = 45_000

/** Most records one response carries; the read's own omission count is added. */
// The `/tmax` command's text display uses its own shorter row cap; this is the
// HTTP payload cap.
export const HISTORY_RECORD_LIMIT = 50

const CELL_LIMIT = 160
const MESSAGE_LIMIT = 240
const BLOCKER_LIMIT = 10

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {}
}

function list(value) {
  return Array.isArray(value) ? value : []
}

/** One bounded single-line string, or null when the value is not evidence. */
function text(value, limit = CELL_LIMIT) {
  if (typeof value !== 'string') return null
  const trimmed = value.replace(/\s+/gu, ' ').trim()
  if (trimmed === '') return null
  return trimmed.length > limit ? `${trimmed.slice(0, limit - 1)}…` : trimmed
}

function number(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function boolean(value) {
  return value === true || value === false ? value : null
}

/**
 * `incomplete_capture` is a count in current tmax output (0 or N omitted
 * records) and was a boolean in older output. Preserve whichever evidence the
 * CLI supplied: a numeric 0 is a material "no omission" fact, not unknown.
 */
function captureCount(value) {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  return boolean(value)
}

/**
 * The process/history scope, limits, and material coverage gaps of one read.
 * Every field is bounded; unknown stays null rather than being guessed.
 */
export function mapHistoryCoverage(value) {
  const coverage = object(value)
  if (Object.keys(coverage).length === 0) return null
  const limits = object(coverage.limits)
  return {
    processInstance: text(coverage.process_instance),
    processStartedAt: text(coverage.process_started_at),
    lastClearAt: text(coverage.last_clear_at),
    retainedCount: number(coverage.retained_count),
    oldest: text(coverage.oldest),
    newest: text(coverage.newest),
    evicted: number(coverage.evicted),
    incompleteCapture: captureCount(coverage.incomplete_capture),
    priorProcessCoverage: text(coverage.prior_process_coverage),
    directTrafficCoverage: text(coverage.direct_traffic_coverage),
    limits: {
      records: number(limits.records),
      recordBytes: number(limits.record_bytes),
    },
    retention: text(coverage.retention),
  }
}

/** tmax's typed blockers reduced to a bounded code/message/state allowlist. */
export function mapHistoryBlockers(value) {
  return list(value).slice(0, BLOCKER_LIMIT).flatMap(entry => {
    const blocker = object(entry)
    const code = text(blocker.code, 80)
    if (code === null) return []
    const mapped = { code }
    const message = text(blocker.message, MESSAGE_LIMIT)
    if (message !== null) mapped.message = message
    const state = text(blocker.state, 80)
    if (state !== null) mapped.state = state
    return [mapped]
  })
}

/**
 * One compact retained request row. The allowlist is the whole contract: a
 * body, header, credential, or any field this function does not name never
 * crosses. Optional evidence (selected destination, duration) appears only
 * when tmax retained it.
 */
export function mapHistoryRecord(value) {
  const record = object(value)
  const row = {
    request: text(record.request ?? record.handle, 128),
    time: text(record.time),
    timeBasis: text(record.time_basis),
    outcome: text(record.outcome),
    model: text(record.model),
    modelKind: text(record.model_kind),
    protocol: text(record.protocol),
    provider: text(record.provider),
    target: text(record.target),
    agentProvider: text(record.agent_provider),
    reasonCode: text(record.reason_code),
  }
  const selection = object(record.selection)
  const qualifiedModel = text(record.qualified_model ?? record.selected_model ?? selection.qualified_model)
  if (qualifiedModel !== null) row.qualifiedModel = qualifiedModel
  const durationMs = number(record.duration_ms ?? record.duration)
  if (durationMs !== null) row.durationMs = durationMs
  return row
}

function unavailableBody(read, data, messageFallback) {
  const outcome = object(read)
  const covered = object(data)
  const code = text(outcome.code, 80) ?? text(covered.reason_code, 80) ?? 'history_unavailable'
  const message = text(outcome.message, MESSAGE_LIMIT)
    ?? text(covered.message, MESSAGE_LIMIT)
    ?? text(envelopeMessage(outcome.envelope), MESSAGE_LIMIT)
    ?? messageFallback
  const blockers = mapHistoryBlockers(list(outcome.envelope?.blockers).length > 0 ? outcome.envelope.blockers : covered.blockers)
  const coverage = mapHistoryCoverage(covered.coverage)
  return {
    ok: false,
    code,
    message,
    ...(blockers.length > 0 ? { blockers } : {}),
    ...(coverage === null ? {} : { coverage }),
  }
}

/**
 * Map one `history list` read onto the history route contract.
 *
 * Success is `{ok:true, generatedAt, coverage, records, truncated?, omitted?}`;
 * a typed tmax refusal or no-daemon-history answer is `{ok:false, ...}` at 200;
 * a missing envelope is a 502 `history_unavailable`.
 */
export function mapHistoryRead(read, { generatedAt = new Date().toISOString(), recordLimit = HISTORY_RECORD_LIMIT } = {}) {
  const outcome = object(read)
  const envelope = outcome.envelope
  if (envelope === null || envelope === undefined) {
    return {
      status: 502,
      body: {
        ok: false,
        code: 'history_unavailable',
        message: text(outcome.message, MESSAGE_LIMIT) ?? 'tmax did not answer',
      },
    }
  }
  const data = object(outcome.data)
  if (outcome.ok !== true) {
    return { status: 200, body: unavailableBody(outcome, data, 'tmax did not complete the history read') }
  }
  // A completed read may still report that no daemon history exists; that is
  // an availability fact, never a zero-record claim.
  if (data.snapshot_state === 'absent' || data.reason_code === 'no_daemon_history') {
    const code = text(data.reason_code, 80) ?? 'no_daemon_history'
    const coverage = mapHistoryCoverage(data.coverage)
    return {
      status: 200,
      body: {
        ok: false,
        code,
        message: 'no retained daemon history',
        ...(coverage === null ? {} : { coverage }),
      },
    }
  }
  if (!Array.isArray(data.records) && data.matched_retained_count === undefined) {
    return {
      status: 200,
      body: { ok: false, code: 'history_unavailable', message: 'tmax returned no history evidence' },
    }
  }
  const rawRecords = Array.isArray(data.records) ? data.records : []
  const limit = Number.isInteger(recordLimit) && recordLimit > 0 ? recordLimit : HISTORY_RECORD_LIMIT
  const records = rawRecords.slice(0, limit).map(mapHistoryRecord)
  const omittedByRead = Number.isInteger(data.omitted_by_limit) && data.omitted_by_limit > 0 ? data.omitted_by_limit : 0
  const omittedByCap = rawRecords.length > limit ? rawRecords.length - limit : 0
  const omitted = omittedByRead + omittedByCap
  return {
    status: 200,
    body: {
      ok: true,
      generatedAt: text(generatedAt) ?? new Date().toISOString(),
      coverage: mapHistoryCoverage(data.coverage),
      records,
      ...(omitted > 0 ? { truncated: true, omitted } : {}),
    },
  }
}

/**
 * The history read service. `read` is injectable for tests; production uses
 * the one bounded CLI path. It never observes, refreshes, starts, clears, or
 * writes tmax.
 *
 * @param {object} input
 * @param {object} input.config
 * @param {object} [input.env]
 * @param {(message: string) => void} [input.log]
 * @param {Function} [input.read] - `readEnvelope(config, argv, options)`
 * @param {() => string} [input.now]
 * @returns {{read: () => Promise<{status: number, body: object}>}}
 */
export function createHistoryService({ config = {}, env = process.env, log, read = readEnvelope, now = () => new Date().toISOString() } = {}) {
  async function readHistory() {
    try {
      const result = await read(config, ['history', 'list'], { timeoutMs: HISTORY_READ_TIMEOUT_MS, env })
      return mapHistoryRead(result, { generatedAt: now() })
    } catch (error) {
      const message = text(error instanceof Error ? error.message : String(error), MESSAGE_LIMIT) ?? 'history read failed'
      log?.(`history read failed: ${message}`)
      return { status: 502, body: { ok: false, code: 'history_unavailable', message } }
    }
  }

  return { read: readHistory }
}
