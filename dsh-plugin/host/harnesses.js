// The declared-target (Harness) section of the host read model.
//
// A Harness is one tmax-declared target. `tmax agent list` owns the target
// inventory and its compact counts; `tmax agent inspect TARGET` owns the
// desired provider entries, the applied state, the core-reported `unapplied`
// flag, and the target blockers. The list read is authoritative: a target that
// the list names stays a row even when its own inspection fails, so the page
// can name that failure instead of hiding the target. Unknown facts stay null;
// an empty readable list stays empty.
import { INVENTORY_TIMEOUT_MS, READ_TIMEOUT_MS, readEnvelope } from './cli.js'

function list(value) {
  return Array.isArray(value) ? value : []
}

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {}
}

function text(value) {
  return typeof value === 'string' && value !== '' ? value : null
}

function integer(value) {
  return Number.isInteger(value) ? value : null
}

function nullableBoolean(value) {
  return value === true || value === false ? value : null
}

/** One declared target's provider entries, in inspect order. */
function mapProviders(inspect) {
  const providers = []
  for (const row of list(inspect.providers)) {
    const entry = object(row)
    const name = text(entry.name)
    if (name === null) continue
    const models = []
    for (const model of list(entry.models)) {
      const selection = object(model)
      const id = text(selection.id)
      if (id === null) continue
      models.push({ id, protocol: text(selection.protocol) })
    }
    providers.push({
      name,
      sourceProvider: text(object(entry.source).provider),
      relayed: entry.relayed === true,
      models,
    })
  }
  return providers
}

/** One declared target's blockers: inspect's own list, or the failed-read code. */
function mapBlockers(inspect) {
  const blockers = []
  for (const row of list(inspect.blockers)) {
    const blocker = object(row)
    const code = text(blocker.code)
    if (code === null) continue
    blockers.push({ code, message: text(blocker.message) })
  }
  return blockers
}

/**
 * One Harness row. The list facts stand even when the inspection failed; the
 * row then carries the typed failure as its blocker and leaves the inspected
 * facts null instead of inventing them.
 */
function mapHarness(id, listed, inspectRead) {
  const inspected = inspectRead?.ok === true ? object(inspectRead.data) : null
  const providers = inspected === null ? [] : mapProviders(inspected)
  const countedModels = inspected === null
    ? null
    : providers.reduce((total, entry) => total + entry.models.length, 0)
  const defaultSelection = inspected === null ? null : object(inspected.default)
  return {
    id,
    agent: text(listed.agent) ?? text(inspected?.agent),
    path: text(listed.path) ?? text(inspected?.path),
    providerCount: integer(listed.provider_count) ?? (inspected === null ? null : providers.length),
    modelCount: integer(listed.model_count) ?? countedModels,
    defaultConfigured: nullableBoolean(listed.default_configured)
      ?? (inspected === null ? null : defaultSelection !== null && Object.keys(defaultSelection).length > 0),
    appliedState: inspected === null ? null : text(inspected.applied_state),
    unapplied: inspected === null ? null : nullableBoolean(inspected.unapplied),
    blockers: inspected === null
      ? [{
          code: text(inspectRead?.code) ?? 'inspect_unavailable',
          message: text(inspectRead?.message) ?? 'the declared target could not be inspected',
        }]
      : mapBlockers(inspected),
    default: inspected === null ? null : (Object.keys(defaultSelection).length === 0 ? null : inspected.default),
    providers,
  }
}

/**
 * Read every declared target as one Harness row.
 *
 * The list read is the only failure that makes the section unknown: it is
 * returned as a typed `error` beside an empty list, so the state composer can
 * name the section failure. A single target's inspection failure stays on that
 * target's row.
 *
 * @param {object} config
 * @returns {Promise<{harnesses: object[], error: {code: string, message: string}|null}>}
 */
export async function readHarnesses(config = {}) {
  const listed = await readEnvelope(config, ['agent', 'list', '--format', 'json'], { timeoutMs: READ_TIMEOUT_MS })
  if (listed.ok !== true) {
    return {
      harnesses: [],
      error: {
        code: text(listed.code) ?? 'harnesses_unavailable',
        message: text(listed.message) ?? 'tmax did not report the declared targets',
      },
    }
  }
  const agents = object(listed.data).agents
  if (!Array.isArray(agents)) {
    // A completed read without the target list is unreadable, not empty: an
    // empty list would claim that no target is declared.
    return {
      harnesses: [],
      error: { code: 'invalid_result', message: 'tmax agent list returned no target list' },
    }
  }
  const inspected = await Promise.all(agents.map(row => {
    const target = text(object(row).target)
    if (target === null) {
      return Promise.resolve({
        ok: false,
        code: 'invalid_target',
        message: 'tmax agent list returned a target without an id',
      })
    }
    return readEnvelope(config, ['agent', 'inspect', target, '--format', 'json'], { timeoutMs: INVENTORY_TIMEOUT_MS })
  }))
  const harnesses = []
  agents.forEach((row, index) => {
    const listedRow = object(row)
    const id = text(listedRow.target)
    if (id === null) return
    harnesses.push(mapHarness(id, listedRow, inspected[index]))
  })
  return { harnesses, error: null }
}
