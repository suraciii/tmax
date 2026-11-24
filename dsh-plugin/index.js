// Host half: project tmax's DSH gateway access into the live LLM registry, and
// serve the page's read model and explicit operations.
//
// The plugin is deliberately thin: tmax owns provider/model/alias/quota facts;
// this module reads the target inventory (`tmax agent list`/`agent inspect`,
// `gateway status`, `status`), decides which plugin owns the routes, and
// exposes exact HTTP routes, one read tool, one skill, and one read command.
// No credential value ever enters this module, and no GET observes a provider.
import { createActionService } from './host/actions.js'
import { CONNECT_TIMEOUT_MS, readEnvelope, resolveHome, usableBaseUrl, usableCredentialRef } from './host/cli.js'
import { createHistoryService, HISTORY_READ_TIMEOUT_MS } from './host/history.js'
import { readInventory, resolveTarget } from './host/inventory.js'
import { createObservations } from './host/observations.js'
import {
  hasAppliedProviders,
  liveProviderIds,
  readProfileDocuments,
  resolveOwnership,
} from './host/ownership.js'
import { createRoutes, deriveBlockers } from './host/routes.js'
import { createSnapshotService } from './host/snapshot.js'

export const name = 'tmax'
export const inject = ['llm', 'settings', 'credentials']

// ---------------------------------------------------------------------------
// `/tmax` read command.
//
// A human-facing read surface, never a model turn: the handler reads the host
// snapshot (or `tmax history list`) and formats bounded text. It does not
// observe, refresh, connect, or mutate tmax. The formatting is pure and
// exported so the wording contract can be unit tested without a loader.

export const COMMAND_SUBCOMMANDS = Object.freeze(['status', 'providers', 'routing', 'gateway', 'diagnostics', 'history'])
export const COMMAND_ROUTE_LIMIT = 20
export const COMMAND_PROVIDER_LIMIT = 40
export const COMMAND_ROUTING_LIMIT = 40
export const COMMAND_ERROR_LIMIT = 20
export const HISTORY_RECORD_LIMIT = 10

// The `/tmax history` command and GET /_tmax/history share one bounded read
// deadline; it is owned by the history service.
export { HISTORY_READ_TIMEOUT_MS }

const COMMAND_CELL_LIMIT = 120
const COMMAND_BLOCKER_LIMIT = 10
const COMMAND_USAGE = `usage: /tmax [${COMMAND_SUBCOMMANDS.join('|')}]`

/** One bounded, single-line cell; empty and unreadable values fall back later. */
function commandText(value, limit = COMMAND_CELL_LIMIT) {
  if (typeof value !== 'string') return ''
  const text = value.replace(/\s+/gu, ' ').trim()
  if (text === '') return ''
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text
}

/** One human-readable cell that distinguishes unknown from zero. */
function commandCell(value, fallback = 'not reported') {
  if (value === null || value === undefined || value === '') return fallback
  if (typeof value === 'number') return String(value)
  if (typeof value === 'boolean') return value ? 'yes' : 'no'
  return commandText(String(value)) || fallback
}

function commandTriState(value) {
  return value === true ? 'yes' : value === false ? 'no' : 'unknown'
}

function commandObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {}
}

function commandRows(value) {
  return Array.isArray(value) ? value : []
}

/** The first state error for one section, or null. */
function sectionError(state, section) {
  const errors = commandRows(commandObject(state).errors)
  return errors.find(error => commandObject(error).section === section) ?? null
}

/** A bounded `unavailable (code: message)` line instead of a zero/success claim. */
function unavailableLine(label, error, fallbackCode) {
  const row = commandObject(error)
  return `${label}: unavailable (${commandCell(row.code, fallbackCode)}: ${commandCell(row.message, 'not reported')})`
}

/** Parse one closed `/tmax` subcommand; unknown input never reaches a read. */
export function parseTmaxCommand(rawInput) {
  const text = typeof rawInput === 'string' ? rawInput.trim() : ''
  if (text === '') return { ok: true, subcommand: 'status' }
  const words = text.split(/\s+/u)
  if (words.length !== 1 || !COMMAND_SUBCOMMANDS.includes(words[0])) {
    return {
      ok: false,
      code: 'unknown_subcommand',
      message: `unknown /tmax subcommand "${commandText(text, 40) || '?'}"; ${COMMAND_USAGE}`,
    }
  }
  return { ok: true, subcommand: words[0] }
}

function effectiveDefaultText(value) {
  const selection = commandObject(value)
  if (Object.keys(selection).length === 0) return 'not reported'
  const source = commandText(selection.source ?? '')
  return `${commandCell(selection.provider, 'unknown')}/${commandCell(selection.model, 'unknown')}${source === '' ? '' : ` (${source})`}`
}

function pendingText(value) {
  if (value === null || value === undefined) return 'none reported'
  if (typeof value === 'object') return commandText(JSON.stringify(value)) || 'reported'
  return commandCell(value, 'reported')
}

/** Summary: target, route/credential readiness, gateway, provider counts, blockers. */
export function formatTmaxStatus(state, { blockers = [] } = {}) {
  const source = commandObject(state)
  const target = commandObject(source.target)
  const gateway = commandObject(source.gateway)
  const providers = commandObject(source.providers)
  const projectionError = sectionError(source, 'projection')
  const providersError = sectionError(source, 'providers')
  const routes = Array.isArray(source.routes) ? source.routes : undefined
  const lines = [
    `tmax state: ${source.ok === true ? 'readable' : 'unavailable'}; generated: ${commandCell(source.generatedAt, 'not reported')}; revision: ${commandCell(source.configurationRevision, 'not reported')}`,
    `target: ${commandCell(target.id, 'not reported')}${typeof target.path === 'string' && target.path !== '' ? ` (${commandText(target.path)})` : ''}`,
    `  effective default: ${effectiveDefaultText(target.effectiveDefault)}`,
    `  applied: ${commandCell(target.appliedState, 'unknown')}; pending native changes: ${pendingText(target.pendingNativeChanges)}`,
  ]
  // A failed section is never rendered as a successful zero count.
  if (projectionError !== null) {
    lines.push(unavailableLine('routes', projectionError, 'projection_unavailable'))
  } else if (routes === undefined) {
    lines.push('routes: unavailable (projection_unavailable: the routes section was not reported)')
  } else {
    lines.push(`routes: ${routes.length}`)
    for (const route of routes.slice(0, COMMAND_ROUTE_LIMIT)) {
      const row = commandObject(route)
      const credential = row.credentialConfigured === true
        ? 'credential configured'
        : row.credentialConfigured === false ? 'credential missing' : 'credential unknown'
      lines.push(`  - ${commandCell(row.provider ?? row.name, 'unnamed route')}: ${row.active === true ? 'active' : 'not active'}; ${credential}; ${commandRows(row.models).length} model(s)`)
    }
    if (routes.length > COMMAND_ROUTE_LIMIT) lines.push(`  ${routes.length - COMMAND_ROUTE_LIMIT} more route(s) omitted`)
  }
  lines.push(`gateway: configured ${commandTriState(gateway.configured)}; ready ${commandTriState(gateway.ready)}`)
  if (providersError !== null) {
    lines.push(unavailableLine('providers', providersError, 'providers_unavailable'))
  } else if (!Array.isArray(providers.tmax) || !Array.isArray(providers.agent)) {
    lines.push('providers: unavailable (providers_unavailable: the providers section was not reported)')
  } else {
    lines.push(`providers: ${providers.tmax.length} tmax, ${providers.agent.length} agent-configured`)
  }
  const blockerRows = commandRows(blockers)
  lines.push(`blockers: ${blockerRows.length}`)
  for (const blocker of blockerRows.slice(0, COMMAND_BLOCKER_LIMIT)) {
    const row = commandObject(blocker)
    lines.push(`  - ${commandCell(row.source, 'unknown')}/${commandCell(row.code, 'unknown')}: ${commandCell(row.message, 'not reported')}`)
  }
  if (blockerRows.length > COMMAND_BLOCKER_LIMIT) lines.push(`  ${blockerRows.length - COMMAND_BLOCKER_LIMIT} more blocker(s) omitted`)
  return lines.join('\n')
}

/** A quota row label may be a plain string or the projected `{key}` object. */
function quotaLabelText(label) {
  if (label === null || label === undefined) return ''
  if (typeof label === 'string') return commandText(label)
  if (typeof label === 'number') return String(label)
  const row = commandObject(label)
  return commandText(row.label ?? row.key ?? '')
}

function appendProvider(lines, value) {
  const provider = commandObject(value)
  const id = commandCell(provider.id, 'unknown provider')
  const name = commandText(provider.name ?? '')
  lines.push(`- ${id}${name === '' || name === id ? '' : ` (${name})`}`)
  lines.push(`  enabled: ${commandTriState(provider.enabled)}; authentication: ${commandCell(provider.authentication, 'unknown')}; integration: ${commandCell(provider.integration, 'unknown')}`)
  const billing = commandObject(provider.billing)
  lines.push(`  quota: ${commandCell(billing.state, 'unknown')}; kind: ${commandCell(billing.kind, 'unknown')}; freshness: ${commandCell(billing.freshness, 'missing')}; observed: ${commandCell(billing.observedAt, 'not observed')}`)
  if (typeof billing.reasonCode === 'string' && billing.reasonCode !== '') lines.push(`  quota reason: ${commandText(billing.reasonCode)}`)
  if (typeof billing.lastAttempt === 'string' && billing.lastAttempt !== '' && billing.lastAttempt !== 'completed') {
    lines.push(`  last attempt: ${commandText(billing.lastAttempt)}`)
  }
  const rows = commandRows(billing.rows)
  if (rows.length > 0) {
    const first = commandObject(rows[0])
    const counters = commandObject(first.counters)
    if (typeof first.percent === 'number') {
      const label = quotaLabelText(first.label)
      lines.push(`  quota reading: ${first.percent}%${label === '' ? '' : ` (${label})`}`)
    } else if (typeof counters.remaining === 'string' || typeof counters.remaining === 'number') {
      lines.push(`  quota reading: remaining ${commandCell(counters.remaining)} of ${commandCell(counters.limit, 'unknown limit')}`)
    } else {
      lines.push(`  quota reading: ${rows.length} row(s) reported`)
    }
  }
  const suspension = commandObject(provider.suspension)
  const suspensionDetails = []
  if (typeof suspension.reasonCode === 'string' && suspension.reasonCode !== '') suspensionDetails.push(commandText(suspension.reasonCode))
  if (typeof suspension.resetAt === 'string' && suspension.resetAt !== '') suspensionDetails.push(`reset ${commandText(suspension.resetAt)}`)
  lines.push(`  suspension: ${commandCell(suspension.state, 'unknown')}${suspensionDetails.length === 0 ? '' : ` (${suspensionDetails.join(', ')})`}`)
  const failure = commandObject(provider.failure)
  const failureDetails = []
  if (Number.isInteger(failure.consecutiveFailures) && failure.consecutiveFailures > 0) failureDetails.push(`${failure.consecutiveFailures} consecutive failure(s)`)
  if (typeof failure.expiresAt === 'string' && failure.expiresAt !== '') failureDetails.push(`until ${commandText(failure.expiresAt)}`)
  if (typeof failure.reasonCode === 'string' && failure.reasonCode !== '') failureDetails.push(commandText(failure.reasonCode))
  lines.push(`  failure: ${commandCell(failure.state, 'unknown')}${failureDetails.length === 0 ? '' : ` (${failureDetails.join(', ')})`}`)
  if (provider.error !== null && provider.error !== undefined) {
    const error = commandObject(provider.error)
    lines.push(`  error: ${commandCell(error.code, 'provider_unavailable')}: ${commandCell(error.message, 'provider inspect failed')}`)
  }
  if (typeof provider.accountIdentity === 'string' && provider.accountIdentity !== '') lines.push(`  account: ${commandText(provider.accountIdentity)}`)
}

/** One line per provider with explicit unknown-vs-zero quota and repair state. */
export function formatTmaxProviders(state) {
  const source = commandObject(state)
  const providers = commandObject(source.providers)
  const providersError = sectionError(source, 'providers')
  const tmaxRows = Array.isArray(providers.tmax) ? providers.tmax : undefined
  const lines = []
  // A failed providers read is stated, never counted as zero providers.
  if (providersError !== null) {
    lines.push(unavailableLine('tmax providers', providersError, 'providers_unavailable'))
  } else if (tmaxRows === undefined) {
    lines.push('tmax providers: unavailable (providers_unavailable: the providers section was not reported)')
  } else {
    lines.push(`tmax providers: ${tmaxRows.length}`)
    for (const row of tmaxRows.slice(0, COMMAND_PROVIDER_LIMIT)) appendProvider(lines, row)
    if (tmaxRows.length > COMMAND_PROVIDER_LIMIT) lines.push(`${tmaxRows.length - COMMAND_PROVIDER_LIMIT} more provider(s) omitted`)
  }
  const agentRows = Array.isArray(providers.agent) ? providers.agent : []
  const agentError = commandObject(providersError).code === 'agent_settings_unavailable' ? providersError : null
  if (agentRows.length > 0) {
    lines.push(`agent-configured providers: ${agentRows.length}`)
    for (const row of agentRows.slice(0, COMMAND_PROVIDER_LIMIT)) {
      const provider = commandObject(row)
      const details = [`state ${commandCell(provider.state, 'unknown')}`]
      if (typeof provider.reasonCode === 'string' && provider.reasonCode !== '') details.push(commandText(provider.reasonCode))
      if (typeof provider.observedAt === 'string' && provider.observedAt !== '') details.push(`observed ${commandText(provider.observedAt)}`)
      lines.push(`- ${commandCell(provider.provider, 'unknown provider')}: ${details.join('; ')}`)
    }
    if (agentRows.length > COMMAND_PROVIDER_LIMIT) lines.push(`${agentRows.length - COMMAND_PROVIDER_LIMIT} more agent provider(s) omitted`)
  } else if (agentError !== null) {
    lines.push(unavailableLine('agent-configured providers', agentError, 'agent_settings_unavailable'))
  }
  return lines.join('\n')
}

/** One row per projected model with its selection, candidate count, and errors. */
export function formatTmaxRouting(state) {
  const source = commandObject(state)
  const routingError = sectionError(source, 'routing')
  const rows = Array.isArray(source.routing) ? source.routing : undefined
  if (routingError !== null) return unavailableLine('routing', routingError, 'routing_unavailable')
  if (rows === undefined) return 'routing: unavailable (routing_unavailable: the routing section was not reported)'
  const lines = [`routing: ${rows.length} projected model(s)`]
  for (const row of rows.slice(0, COMMAND_ROUTING_LIMIT)) {
    const routing = commandObject(row)
    lines.push(`- ${commandCell(routing.model, 'unknown model')} (${commandCell(routing.protocol, 'unknown protocol')}; ${commandCell(routing.kind, 'unknown kind')}; availability ${commandCell(routing.availability, 'unknown')}; candidate count ${commandCell(routing.candidateCount, 'unknown')})`)
    if (routing.selection === null || routing.selection === undefined) {
      lines.push('  selection: none reported')
    } else {
      const selection = commandObject(routing.selection)
      lines.push(`  selection: ${commandCell(selection.qualifiedModel, 'unknown')} (priority ${commandCell(selection.priority, 'unknown')})`)
    }
    const candidates = commandRows(routing.candidates)
    const selected = candidates.filter(candidate => commandObject(candidate).selected === true).length
    lines.push(`  candidates: ${candidates.length} (${selected} selected); excluded: ${commandRows(routing.excluded).length}`)
    if (routing.error !== null && routing.error !== undefined) {
      const error = commandObject(routing.error)
      lines.push(`  error: ${commandCell(error.code, 'model_explain_unavailable')}: ${commandCell(error.message, 'not reported')}`)
    }
  }
  if (rows.length > COMMAND_ROUTING_LIMIT) lines.push(`${rows.length - COMMAND_ROUTING_LIMIT} more model(s) omitted`)
  return lines.join('\n')
}

/** Gateway readiness and supervision facts, unknown when not reported. */
export function formatTmaxGateway(state) {
  const source = commandObject(state)
  if (source.gateway === null || source.gateway === undefined) return 'gateway: unavailable (no gateway section was reported)'
  const gateway = commandObject(source.gateway)
  const supervision = commandObject(gateway.supervision)
  const autostart = commandObject(gateway.autostart)
  const sharedKey = commandObject(gateway.sharedKey)
  return [
    `gateway: configured ${commandTriState(gateway.configured)}; ready ${commandTriState(gateway.ready)}`,
    `listen: ${commandCell(gateway.listen, 'not reported')}`,
    `daemon: ${commandCell(gateway.daemonDiscoveryState, 'unknown')}${typeof gateway.reasonCode === 'string' && gateway.reasonCode !== '' ? ` (${commandText(gateway.reasonCode)})` : ''}`,
    `supervision: ${commandCell(supervision.state, 'unknown')}${typeof supervision.reasonCode === 'string' && supervision.reasonCode !== '' ? ` (${commandText(supervision.reasonCode)})` : ''}`,
    `autostart: saved intent ${commandTriState(autostart.savedIntent)}; registration ${commandCell(autostart.registration, 'unknown')}; unit matches ${commandTriState(autostart.unitMatches)}`,
    `shared key: ${commandCell(sharedKey.loadedState, 'unknown')}${typeof sharedKey.reasonCode === 'string' && sharedKey.reasonCode !== '' ? ` (${commandText(sharedKey.reasonCode)})` : ''}`,
  ].join('\n')
}

/** Read-model provenance, history availability, and every section error. */
export function formatTmaxDiagnostics(state) {
  const source = commandObject(state)
  const diagnostics = commandObject(source.diagnostics)
  const history = commandObject(diagnostics.history)
  const errors = commandRows(source.errors)
  const lines = [
    `read model: ${commandCell(diagnostics.readModel, 'unknown')}`,
    `generated: ${commandCell(source.generatedAt, 'not reported')}; revision: ${commandCell(source.configurationRevision, 'not reported')}; tmax version: ${commandCell(source.tmaxVersion, 'not reported')}`,
    `history: ${commandCell(history.state, 'unavailable')}${typeof history.reasonCode === 'string' && history.reasonCode !== '' ? ` (${commandText(history.reasonCode)})` : ''}`,
    `last inventory read at: ${commandCell(diagnostics.lastProjectionAt, 'not reported')}`,
    `last error: ${commandCell(diagnostics.lastError, 'none reported')}`,
    `errors: ${errors.length}`,
  ]
  for (const error of errors.slice(0, COMMAND_ERROR_LIMIT)) {
    const row = commandObject(error)
    lines.push(`  - ${commandCell(row.section, 'unknown')}/${commandCell(row.code, 'unknown')}: ${commandCell(row.message, 'not reported')}`)
  }
  if (errors.length > COMMAND_ERROR_LIMIT) lines.push(`  ${errors.length - COMMAND_ERROR_LIMIT} more error(s) omitted`)
  return lines.join('\n')
}

function appendHistoryCoverage(lines, value) {
  const coverage = commandObject(value)
  if (Object.keys(coverage).length === 0) {
    lines.push('coverage: not reported')
    return
  }
  if (typeof coverage.process_instance === 'string' && coverage.process_instance !== '') lines.push(`process: ${commandText(coverage.process_instance)}`)
  const oldest = commandText(coverage.oldest ?? '') || 'unknown'
  const newest = commandText(coverage.newest ?? '') || 'unknown'
  lines.push(`retained: ${commandCell(coverage.retained_count, 'unknown')}; oldest: ${oldest}; newest: ${newest}`)
  const limits = commandObject(coverage.limits)
  if (limits.records !== undefined) lines.push(`limits: ${commandCell(limits.records, 'unknown')} record(s)`)
  if (coverage.evicted !== undefined) lines.push(`evicted: ${commandCell(coverage.evicted, 'unknown')}; incomplete capture: ${commandCell(coverage.incomplete_capture, 'unknown')}`)
  lines.push(`prior process coverage: ${commandCell(coverage.prior_process_coverage, 'unknown')}; direct traffic coverage: ${commandCell(coverage.direct_traffic_coverage, 'unknown')}`)
}

/**
 * Format one `tmax history list` read. An unavailable daemon/history is stated
 * with its reason, never rendered as an empty history.
 */
export function formatTmaxHistory(read) {
  const envelope = read !== null && typeof read === 'object' ? read.envelope : undefined
  if (envelope === null || envelope === undefined) {
    return `history unavailable: ${commandCell(read?.message, 'tmax did not answer')}`
  }
  const result = commandObject(read)
  const data = commandObject(result.data)
  if (result.ok !== true) {
    const lines = [`history unavailable: ${commandCell(result.code, 'unknown')}${typeof result.message === 'string' && result.message !== '' ? ` — ${commandText(result.message)}` : ''}`]
    if (typeof data.reason_code === 'string' && data.reason_code !== '') lines.push(`reason: ${commandText(data.reason_code)}`)
    appendHistoryCoverage(lines, data.coverage)
    return lines.join('\n')
  }
  // A completed read may still report that no daemon history exists; that is
  // an availability fact, not a zero-record claim.
  if (data.reason_code === 'no_daemon_history' || data.snapshot_state === 'absent') {
    const lines = ['history unavailable: no retained daemon history']
    if (typeof data.reason_code === 'string' && data.reason_code !== '') lines.push(`reason: ${commandText(data.reason_code)}`)
    appendHistoryCoverage(lines, data.coverage)
    return lines.join('\n')
  }
  if (data.records === undefined && data.coverage === undefined && data.matched_retained_count === undefined) {
    return 'history unavailable: tmax returned no history data'
  }
  const lines = []
  appendHistoryCoverage(lines, data.coverage)
  const range = commandObject(data.requested_range)
  const rangeParts = []
  if (typeof range.since === 'string' && range.since !== '') rangeParts.push(`since ${commandText(range.since)}`)
  if (typeof range.until === 'string' && range.until !== '') rangeParts.push(`until ${commandText(range.until)}`)
  lines.push(`requested range: ${rangeParts.length === 0 ? 'no explicit bounds' : rangeParts.join(', ')}`)
  lines.push(`records: ${commandCell(data.matched_retained_count, 'unknown')} matched; ${commandCell(data.omitted_by_limit, '0')} omitted by limit; in-progress included ${commandTriState(data.includes_in_progress)}`)
  const records = commandRows(data.records)
  lines.push(`retained records listed: ${Math.min(records.length, HISTORY_RECORD_LIMIT)}${records.length > HISTORY_RECORD_LIMIT ? ` of ${records.length}` : ''}`)
  for (const record of records.slice(0, HISTORY_RECORD_LIMIT)) {
    const row = commandObject(record)
    const details = [
      `${commandCell(row.time, 'time unknown')} (${commandCell(row.time_basis, 'basis unknown')})`,
      commandCell(row.outcome, 'outcome unknown'),
      `${commandCell(row.provider, 'provider unknown')}/${commandCell(row.model, 'model unknown')} [${commandCell(row.protocol, 'protocol unknown')}]`,
    ]
    if (typeof row.target === 'string' && row.target !== '') details.push(`target ${commandText(row.target)}`)
    if (typeof row.agent_provider === 'string' && row.agent_provider !== '') details.push(`agent ${commandText(row.agent_provider)}`)
    if (typeof row.reason_code === 'string' && row.reason_code !== '') details.push(`reason ${commandText(row.reason_code)}`)
    lines.push(`- ${details.join('; ')}`)
  }
  return lines.join('\n')
}

/** Format one non-history subcommand; history has its own read path. */
export function formatTmaxSection(subcommand, state, { blockers = [] } = {}) {
  switch (subcommand) {
    case 'status': return formatTmaxStatus(state, { blockers })
    case 'providers': return formatTmaxProviders(state)
    case 'routing': return formatTmaxRouting(state)
    case 'gateway': return formatTmaxGateway(state)
    case 'diagnostics': return formatTmaxDiagnostics(state)
    default: return `unsupported /tmax subcommand; ${COMMAND_USAGE}`
  }
}

export async function apply(ctx, config = {}) {
  const tree = ctx.fiber.entry?.parent.tree
  if (tree === undefined) throw new Error('tmax requires a DSH Loader entry')

  const [, llm, credentials, , , , , toolsModule] = await Promise.all([
    tree.import('@deepseek-ai/dsh-llm-pi-ai'),
    tree.import('@deepseek-ai/dsh-llm'),
    tree.import('@deepseek-ai/dsh-credentials'),
    tree.import('@earendil-works/pi-ai'),
    tree.import('@earendil-works/pi-ai/api/openai-completions.lazy'),
    tree.import('@earendil-works/pi-ai/api/openai-responses.lazy'),
    tree.import('@earendil-works/pi-ai/api/anthropic-messages.lazy'),
    tree.import('@deepseek-ai/dsh-tools'),
  ])
  const log = message => ctx.logger?.warn?.(`tmax: ${message}`)
  /** The protocols the qualified DSH adapter serves. */
  const DSH_PROTOCOLS = new Set(['chat-completions', 'responses', 'messages'])
  const prefix = typeof config.providerPrefix === 'string' ? config.providerPrefix : ''
  let ownershipDecision
  let skippedBlockers = []
  let refreshing
  let lastInventory
  let lastError
  let disposed = false
  let timer

  const observations = createObservations({ ctx })

  function getOwnership() {
    return ownershipDecision
  }

  const snapshot = createSnapshotService({
    config,
    ctx,
    observations,
    getOwnership,
    getRouteBlockers: () => skippedBlockers,
    getLiveNames: () => liveProviderIds(ctx),
  })

  /** Teardown: stop timers and release routes exactly once. */
  function cleanup() {
    disposed = true
    if (timer !== undefined) {
      clearInterval(timer)
      timer = undefined
    }
  }

  /**
   * Whether this home already carries applied `tmax-<entry>` providers in a
   * profile's settings. DSH serves those itself, so this is what the page
   * reports as the route owner.
   * @returns {Promise<boolean>} true when an applied provider document exists
   */
  async function readAppliedProviders() {
    return hasAppliedProviders(await readProfileDocuments(resolveHome(config)))
  }

  /** Re-read the ownership facts without re-reading tmax. */
  async function refreshOwnershipFacts() {
    if (disposed) return ownershipDecision
    try {
      ownershipDecision = resolveOwnership({
        projection: lastInventory,
        liveNames: liveProviderIds(ctx),
        prefix,
        settingsProviders: await readAppliedProviders(),
      })
    } catch (error) {
      log(`ownership re-evaluation failed: ${error instanceof Error ? error.message : String(error)}`)
    }
    return ownershipDecision
  }

  /**
  /**
   * Report every route tmax cannot serve as the applied settings declare it: an
   * unsupported protocol, an unknown context capacity, no usable model, a
   * missing or malformed credential reference, or a missing base URL. The state
   * read model stays complete; these are the blockers a reader acts on.
   */
  function projectRoutes(inventory) {
    const skipped = []
    const targetId = typeof inventory?.target?.id === 'string' && inventory.target.id !== ''
      ? inventory.target.id
      : null
    const applyHint = targetId === null
      ? 'run `tmax agent apply` for this DSH target'
      : `run \`tmax agent apply ${targetId}\` for this DSH target`
    for (const route of inventory.routes || []) {
      if (!DSH_PROTOCOLS.has(route.protocol)) {
        log(`route "${route.name}" uses unsupported protocol "${route.protocol}"`)
        skipped.push({
          code: 'route_protocol_unsupported',
          message: `Route "${route.name}" uses unsupported protocol "${route.protocol}"`,
          route: route.name,
          models: [],
        })
        continue
      }
      const providerName = `${prefix}${route.name}`
      const declaredModels = Array.isArray(route.models) ? route.models : []
      const hiddenModels = []
      const usableModels = declaredModels.filter(model => {
        const contextWindow = model.capabilities?.contextWindow
        if (!Number.isInteger(contextWindow) || contextWindow <= 0) {
          hiddenModels.push(typeof model.id === 'string' ? model.id : String(model.id))
          log(`hiding model "${model.id}" on route "${route.name}": contextWindow is unknown`)
          return false
        }
        return true
      })
      if (hiddenModels.length > 0) {
        skipped.push({
          code: 'route_model_context_window_unknown',
          message: `Route "${providerName}" hides ${hiddenModels.length} model(s) with an unknown context capacity: ${hiddenModels.join(', ')}`,
          route: route.name,
          models: hiddenModels,
        })
      }
      const modelIds = usableModels.map(model => model.id)
      if (modelIds.length === 0) {
        skipped.push({
          code: 'route_no_usable_models',
          message: `Route "${providerName}" declares no model the adapter can serve`,
          route: route.name,
          models: declaredModels.map(model => (typeof model.id === 'string' ? model.id : String(model.id))),
        })
        continue
      }
      const credentialRef = usableCredentialRef(route)
      if (credentialRef === undefined) {
        skipped.push({
          code: 'route_credential_missing',
          message: `Route "${providerName}" declares no credential reference; ${applyHint} before it can serve models`,
          route: route.name,
          models: modelIds,
        })
        continue
      }
      if (usableBaseUrl(route) === undefined) {
        skipped.push({
          code: 'route_base_url_missing',
          message: `Route "${providerName}" declares no base URL; ${applyHint} before it can serve models`,
          route: route.name,
          models: modelIds,
        })
        continue
      }
      // The DSH credentials service throws for a reference outside its grammar.
      // A stale or malformed reference is reported against that route alone; it
      // must never hide every other route.
      try {
        const apiKeyEnv = credentials.credentialRef(credentialRef)
        if (typeof apiKeyEnv !== 'string' || apiKeyEnv === '') throw new Error('empty reference')
      } catch (error) {
        const detail = error instanceof Error ? error.message : String(error)
        log(`route "${providerName}" declares an unusable credential reference (${detail})`)
        skipped.push({
          code: 'route_credential_invalid',
          message: `Route "${providerName}" declares a credential reference DSH cannot use; ${applyHint} to regenerate it`,
          route: route.name,
          models: modelIds,
        })
      }
    }
    return { skipped }
  }

  /**
   * Read the inventory and report what it declares. The providers themselves
   * are a profile setting DSH serves; this service only says which routes exist
   * and what is wrong with them.
   */
  async function refresh() {
    if (disposed) return lastInventory
    if (refreshing !== undefined) return refreshing
    refreshing = (async () => {
      const inventory = await readInventory(config)
      if (disposed) return inventory
      lastInventory = inventory
      lastError = undefined
      skippedBlockers = projectRoutes(inventory).skipped
      // The state reports who serves these routes, so the decision is part of
      // the same read rather than a later correction.
      await refreshOwnershipFacts()
      return inventory
    })().catch(error => {
      lastError = error instanceof Error ? error.message : String(error)
      throw error
    }).finally(() => {
      refreshing = undefined
    })
    return refreshing
  }

  ctx.effect(() => () => cleanup(), 'tmax: refresh')

  try {
    await refresh()
  } catch (error) {
    lastError = error instanceof Error ? error.message : String(error)
    log(`initial inventory unavailable: ${lastError}`)
  }
  // The plugin was unloaded while the initial read was in flight.
  if (disposed) return

  const seconds = Number(config.refreshSeconds)
  const interval = Number.isFinite(seconds) && seconds >= 10 ? seconds : 60
  timer = setInterval(() => {
    refresh().catch(error => log(error instanceof Error ? error.message : String(error)))
  }, interval * 1000)
  timer.unref?.()

  if (typeof ctx.on === 'function') {
    // Another adapter appearing or leaving is a fresh ownership fact.
    ctx.effect(() => {
      const off = ctx.on('llm/adapters-updated', () => {
        refresh().catch(error => log(error instanceof Error ? error.message : String(error)))
      })
      return () => {
        if (typeof off === 'function') off()
      }
    }, 'tmax: ownership re-evaluation')
  }

  /**
   * Apply the target's native configuration through `tmax agent apply`. The
   * target is resolved the same way the inventory resolves it; a preview uses
   * `--dry-run` and returns `dry_run_ready` as a completed preview.
   */
  async function connectTmax({ dryRun }) {
    const target = await resolveTarget(config)
    const args = ['agent', 'apply', target]
    if (dryRun) args.push('--dry-run')
    const result = await readEnvelope(config, args, { timeoutMs: CONNECT_TIMEOUT_MS })
    const envelope = result.envelope
    if (envelope !== undefined && (envelope.status === 'success' || (dryRun && envelope.status === 'dry_run'))) {
      return { ...result, ok: true }
    }
    return result
  }

  // The allowlisted action service shares the snapshot service, so every action
  // validates against the same state the page reads and returns fresh state.
  // One lock set spans the legacy routes and the action service: a connect or
  // an observation is the same operation whichever form requested it.
  const locks = new Set()
  const actions = createActionService({ config, snapshot, locks, log, refreshOwnership: refreshOwnershipFacts })
  // The history service answers GET /_tmax/history; the `/tmax history` command
  // uses the same bounded `history list` read.
  const history = createHistoryService({ config, log })

  // Optional services are acquired through `ctx.inject`, not `ctx.get`: the
  // callback runs when the service exists, is torn down when it goes away, and
  // runs again for a replacement, so a late web server, tool registry, skill
  // registry, or command registry still receives its registration.
  ctx.inject(['webServer'], webCtx => {
    const routes = createRoutes({
      config,
      snapshot,
      observations,
      actions,
      history,
      locks,
      connectTmax,
      refreshOwnership: refreshOwnershipFacts,
      getDeclaredBlockers: () => skippedBlockers,
      log,
    })
    for (const route of routes) {
      webCtx.effect(() => webCtx.webServer.register({
        kind: 'exact',
        path: route.path,
        handler: route.handler,
      }), `tmax: ${route.method} ${route.path}`)
    }
  })

  /** Compact, read-only state for the model-facing tool. Never observes. */
  function compactStatus(state) {
    return {
      ok: state.ok,
      generatedAt: state.generatedAt,
      configurationRevision: state.configurationRevision,
      errors: state.errors ?? [],
      target: state.target,
      ownership: state.ownership,
      gateway: state.gateway,
      routes: (state.routes ?? []).map(route => ({
        name: route.name,
        provider: route.provider,
        protocol: route.protocol,
        scope: route.scope,
        credentialConfigured: route.credentialConfigured,
        models: (route.models ?? []).map(model => model.id),
      })),
      providers: {
        tmax: (state.providers?.tmax ?? []).map(provider => ({
          id: provider.id,
          name: provider.name,
          enabled: provider.enabled,
          authentication: provider.authentication,
          affectedTargets: provider.affectedTargets,
          accountIdentity: provider.accountIdentity,
          subscription: provider.subscription,
          integration: provider.integration,
          billing: provider.billing,
          suspension: provider.suspension,
          failure: provider.failure,
          error: provider.error,
        })),
        agent: state.providers?.agent ?? [],
      },
      routing: state.routing,
      diagnostics: state.diagnostics,
      blockers: deriveBlockers(state, skippedBlockers),
    }
  }

  ctx.inject(['tools'], toolsCtx => {
    toolsCtx.effect(() => toolsCtx.tools.register(toolsModule.defineTool({
      name: 'tmax_status',
      description: 'Read tmax model-access state: target, route ownership, gateway readiness, every provider (including unknown quota), routing explanations, diagnostics, and blockers. Reads the host state, composing effect-free tmax reads on a cache miss; it never contacts a provider or refreshes observations.',
      parameters: {},
      output: {
        schema: { type: 'object', additionalProperties: true },
        render: (_args, value) => [{ type: 'text', text: JSON.stringify(value, null, 2) }],
      },
      async execute() {
        try {
          return compactStatus(await snapshot.state())
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error)
          return {
            ok: false,
            target: null,
            ownership: null,
            gateway: null,
            routes: [],
            providers: { tmax: [], agent: [] },
            routing: [],
            diagnostics: { history: { state: 'unavailable', reasonCode: 'tmax_unavailable' }, lastProjectionAt: null, lastError: message, readModel: 'inventory' },
            errors: [{ section: 'projection', code: 'tmax_unavailable', message }],
            blockers: [{ source: 'section', code: 'tmax_unavailable', message }],
          }
        }
      },
      presentCall: () => ({ card: 'generic', title: 'tmax status', kind: 'read', rawInput: {} }),
    })), 'tmax: status tool')
  })

  ctx.inject(['skills'], skillsCtx => {
    skillsCtx.effect(() => skillsCtx.skills.register({
      name: 'tmax',
      description: 'Use tmax-managed model access and inspect tmax status, subscriptions, quotas, aliases, and routing evidence.',
      content: [
        'When the user asks about model access, subscriptions, quotas, accounts, aliases, routing, or tmax configuration, start with `tmax_status`.',
        'The tool reads tmax\'s local state through the host read model. It never contacts a provider, never resolves a credential, and never refreshes an observation.',
        'A quota reading is a timestamped observation. Report it with its observation time and source; when there is no reading, say unknown or not observed yet instead of zero, absent, or unlimited.',
        'Only an explicit user action may observe providers or apply the DSH target. If the user asks for that, name the operation and its effect; the page offers Refresh (observe) and Apply (native target application through `tmax agent apply`). Never claim either happened unless a result confirmed it.',
        'tmax CLI help is authoritative for syntax and operands. Prefer explicit single operations; preview with the same command\'s `--dry-run` when the user asks to change something.',
        'Never put keys or tokens in tool arguments, commands, logs, or chat. Credentials stay a user action through tmax protected input or sign-in; this plugin only uses DSH credential references, and a reference name is not a secret.',
        'Do not infer quota or availability from a missing value. If tmax is unavailable or the gateway is not ready, report the blocker and the next action; do not claim success or silently switch models.',
      ].join('\n'),
      invocation: { modelInvocable: true, userInvocable: true },
    }), 'tmax: tmax skill')
  })

  // `/tmax` is an optional human-facing read command: registered once when the
  // commands service exists, it reads the host state or `tmax history list`
  // and never observes, refreshes, connects, or mutates tmax.
  ctx.inject(['commands'], commandsCtx => {
    commandsCtx.effect(() => commandsCtx.commands.register({
      name: 'tmax',
      description: 'Inspect tmax model access: target, providers, routing, gateway, diagnostics, and retained request history. Read-only.',
      input: { hint: '[status|providers|routing|gateway|diagnostics|history]' },
      async handler(invocation) {
        const parsed = parseTmaxCommand(invocation?.rawInput)
        if (!parsed.ok) return { kind: 'error', text: parsed.message }
        try {
          if (parsed.subcommand === 'history') {
            // A read-only retained-metadata read; it never starts a service.
            const read = await readEnvelope(config, ['history', 'list'], { timeoutMs: HISTORY_READ_TIMEOUT_MS })
            return { kind: 'success', text: formatTmaxHistory(read) }
          }
          const state = await snapshot.state({ force: true })
          const blockers = deriveBlockers(state, skippedBlockers)
          return { kind: 'success', text: formatTmaxSection(parsed.subcommand, state, { blockers }) }
        } catch (error) {
          return {
            kind: 'error',
            text: `tmax ${parsed.subcommand} failed: ${commandText(error instanceof Error ? error.message : String(error))}`,
          }
        }
      },
    }), 'tmax: /tmax command')
  })

  ctx.logger?.info?.(
    `tmax: ${lastInventory?.routes?.length || 0} route(s), ${lastInventory?.routes?.reduce((count, route) => count + (route.models?.length ?? 0), 0) || 0} model(s)${lastError ? `; last error: ${lastError}` : ''}`,
  )
}
