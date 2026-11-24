// The target inventory behind the adapter and the page read model.
//
// `tmax dsh projection` no longer exists. The same facts are composed from the
// commands that describe one declared DSH target:
//
//   tmax agent list            — resolve the target from the DSH home
//   tmax agent inspect TARGET  — native entries, relayed delivery, capabilities
//   tmax gateway status        — daemon identity, gateway readiness, listen
//   tmax status                — authority (data directory) and CLI version
//
// The result is projection-shaped (`schema_version`, `target`, `gateway`,
// `routes`, `blockers`, `configuration_revision`, `generated_at`, `authority`,
// `tmax_version`) so the state composer and the adapter read the fields they
// always have. No credential value ever enters this module.
import { realpathSync } from 'node:fs'
import { dirname } from 'node:path'

import { explicitTarget, INVENTORY_TIMEOUT_MS, readEnvelope, resolveHome, TmaxError } from './cli.js'

/** The inventory consumer contract version; independent of any tmax schema. */
export const INVENTORY_SCHEMA_VERSION = 1

function list(value) {
  return Array.isArray(value) ? value : []
}

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {}
}

function text(value) {
  return typeof value === 'string' && value !== '' ? value : null
}

/** One comparable path: the real path when it exists, else separators and trailing slashes removed. */
function normalizePath(value) {
  let path = String(value)
  // Only an existing home can be resolved; an absent one stays comparable by spelling.
  try { path = realpathSync(path) } catch {}
  return path.replaceAll('\\', '/').replace(/\/+$/u, '')
}

/** The typed error for one failed tmax read, with its own code and message. */
function commandError(command, result) {
  const detail = result.message === undefined ? '' : ` (${result.message})`
  return new TmaxError(`tmax ${command} failed: ${result.code || 'unknown'}${detail}`, result.code || 'unknown')
}

/**
 * Resolve the declared DSH target: config.target, or the one target matching
 * the DSH home in `tmax agent list`.
 * @param {object} config
 * @returns {Promise<string>}
 * @throws {TmaxError} with `unknown_resource` when no target matches and
 *   `invalid_arguments` when more than one does.
 */
export async function resolveTarget(config = {}) {
  const configured = explicitTarget(config)
  if (configured !== undefined) return configured
  const home = resolveHome(config)
  const listed = await readEnvelope(config, ['agent', 'list'])
  if (!listed.ok) throw commandError('agent list', listed)
  const wanted = normalizePath(home)
  const matches = list(object(listed.data).agents).filter(agent =>
    agent?.agent === 'dsh' && typeof agent.path === 'string' && normalizePath(agent.path) === wanted)
  if (matches.length === 0) {
    throw new TmaxError(
      `tmax agent list failed: unknown_resource (No declared DSH target matches the DSH home ${home})`,
      'unknown_resource',
    )
  }
  if (matches.length > 1) {
    throw new TmaxError(
      'tmax agent list failed: invalid_arguments (Multiple DSH targets match the DSH home; set target or home)',
      'invalid_arguments',
    )
  }
  return String(matches[0].target)
}

/**
 * One route's model rows: tmax's native projection when inspect carries it,
 * else the bare selections. `capabilities` stays `null` when inspect could not
 * describe a model, never an invented descriptor.
 */
function projectModels(projected, selected) {
  const models = Array.isArray(projected?.models)
    ? projected.models
    : selected.map(model => ({ id: model?.id, protocol: model?.protocol, capabilities: null }))
  return models.map(model => ({
    id: text(model?.id),
    protocol: text(model?.protocol),
    display_name: text(model?.display_name) ?? text(model?.id),
    capabilities: object(model?.capabilities) && Object.keys(object(model?.capabilities)).length > 0
      ? object(model.capabilities)
      : null,
    availability: 'unverified',
  }))
}

/**
 * Read the raw facts one inventory composition needs. The gateway and status
 * reads are soft: a failing one leaves those facts unknown instead of erasing
 * the routes the desired configuration already describes.
 *
 * @returns {Promise<{target: string, inspect: object, gateway: object, status: object}>}
 *   `gateway` and `status` are `readEnvelope` results (ok may be false).
 */
export async function readInventoryFacts(config = {}) {
  const target = await resolveTarget(config)
  const timeoutMs = INVENTORY_TIMEOUT_MS
  const [inspectRead, gatewayRead, statusRead] = await Promise.all([
    readEnvelope(config, ['agent', 'inspect', target], { timeoutMs }),
    readEnvelope(config, ['gateway', 'status'], { timeoutMs }),
    readEnvelope(config, ['status'], { timeoutMs }),
  ])
  if (!inspectRead.ok) throw commandError('agent inspect', inspectRead)
  const inspect = object(inspectRead.data)
  if (inspect.agent !== 'dsh') {
    throw new TmaxError(
      'tmax agent inspect failed: invalid_arguments (The selected target is not a DSH target)',
      'invalid_arguments',
    )
  }
  // An older tmax predates the relayed source and native capability fields.
  // Fail loudly instead of projecting a target whose routes cannot be read.
  const providers = list(inspect.providers)
  const projected = list(inspect.native_projection)
  const outdated = providers.some(entry => typeof entry?.relayed !== 'boolean')
    || projected.some(entry => list(entry?.models).some(model =>
      model === null || typeof model !== 'object' || !('capabilities' in model)))
  if (outdated) {
    throw new TmaxError(
      'tmax agent inspect failed: unsupported_tmax (inspect does not report relayed provider entries and native model capabilities; update tmax)',
      'unsupported_tmax',
    )
  }
  return { target, inspect, gateway: gatewayRead, status: statusRead }
}

/** The tmax data directory from `tmax status`, or null when it did not answer. */
function authorityOf(statusRead) {
  if (statusRead?.ok !== true) return null
  const configPath = text(object(statusRead.data).config_path)
  return configPath === null ? null : dirname(configPath)
}

/** The tmax CLI version that answered a status read, or null. */
function versionOf(...reads) {
  for (const read of reads) {
    if (read?.ok !== true) continue
    const version = text(object(object(read.data).daemon).caller_version)
    if (version !== null) return version
  }
  return null
}

/**
 * Compose the projection-shaped inventory from already-read facts.
 * @param {{target: string, inspect: object, gateway: object, status: object}} facts
 */
export function composeInventory(facts) {
  const inspect = object(facts?.inspect)
  const statusRead = facts?.gateway
  const status = statusRead?.ok === true ? object(statusRead.data) : undefined
  const daemonState = status === undefined ? null : text(object(status.daemon).discovery_state)
  const daemonReason = status === undefined ? null : text(object(status.daemon).reason_code)
  const reported = status?.gateway !== null && typeof status?.gateway === 'object' ? status.gateway : undefined
  const providers = list(inspect.providers)
  const projected = list(inspect.native_projection)
  const relayed = providers.filter(entry => entry?.relayed === true)
  // A relayed entry only exists while the derived gateway provider exists, so
  // either fact proves a configured gateway even when no daemon answers.
  const configured = relayed.length > 0 || reported?.key_ref != null || reported?.listen != null
  // With no running daemon, status reports no listener; a relayed entry still
  // carries the endpoint the target is configured to use, which is the
  // configured listen address (tmax builds it as `http://{gateway.listen}`).
  let configuredListen = null
  for (const entry of relayed) {
    const match = /^https?:\/\/([^/]+)/u.exec(text(entry.base_url) ?? '')
    if (match !== null) {
      configuredListen = match[1]
      break
    }
  }
  let ready = null
  let reasonCode = null
  if (!configured) {
    reasonCode = 'gateway_unconfigured'
  } else if (daemonState === 'compatible') {
    ready = reported?.ready === true
    reasonCode = ready ? null : 'gateway_not_ready'
  } else if (daemonState === 'absent') {
    // A configured gateway with no running daemon is not ready, not unknown:
    // tmax status reports readiness false for the same state.
    ready = false
    reasonCode = 'gateway_not_ready'
  } else {
    reasonCode = daemonReason || 'daemon_identity_unavailable'
  }
  const blockers = []
  const routes = []
  const names = []
  if (!configured) {
    blockers.push({
      code: 'gateway_unconfigured',
      message: 'Configure and explicitly start the tmax gateway before this target can use it',
    })
  }
  for (const entry of relayed) {
    const name = String(entry.name)
    const selected = list(entry.models)
    const protocols = [...new Set(selected.map(model => model?.protocol).filter(Boolean))]
    if (selected.length === 0) {
      blockers.push({ code: 'target_incomplete', message: `Gateway route "${name}" has no selected models` })
      continue
    }
    if (protocols.length !== 1) {
      blockers.push({ code: 'incompatible', message: `Gateway route "${name}" mixes protocols; split it into one route per protocol` })
      continue
    }
    names.push(name)
    routes.push({
      name,
      protocol: protocols[0],
      base_url: text(entry.base_url),
      credential_ref: text(entry.credential_ref),
      models: projectModels(projected.find(item => item?.name === name), selected),
    })
  }
  if (routes.length === 0 && blockers.length === 0) {
    blockers.push({ code: 'gateway_entry_missing', message: 'This DSH target has no gateway provider entry' })
  }
  return {
    schema_version: INVENTORY_SCHEMA_VERSION,
    configuration_revision: text(inspect.configuration_revision),
    generated_at: new Date().toISOString(),
    authority: authorityOf(facts?.status),
    tmax_version: versionOf(statusRead, facts?.status),
    target: {
      id: String(inspect.target ?? facts?.target),
      path: text(inspect.path),
      default: inspect.default ?? null,
      routes: names,
    },
    gateway: {
      configured,
      ready,
      listen: text(reported?.listen) ?? configuredListen,
      key_ref: text(reported?.key_ref),
      config_revision: text(reported?.config_revision),
      daemon_discovery_state: daemonState,
      reason_code: reasonCode,
    },
    routes,
    blockers,
  }
}

/**
 * Read the target inventory: the one projection-shaped read the adapter and
 * the state composer share.
 * @param {object} config
 * @returns {Promise<object>}
 * @throws {TmaxError} when the target or its agent dossier cannot be read.
 */
export async function readInventory(config = {}) {
  return composeInventory(await readInventoryFacts(config))
}
