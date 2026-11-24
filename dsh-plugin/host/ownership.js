// Deterministic route ownership: the home plugin when it is installed, the
// profile-scoped page otherwise, and never both by accident.
//
// Live route names alone race DSH activation order: a page that registers
// first makes the home plugin fail with DUPLICATE_ADAPTER, and a page that
// defers on a partial match can leave the profile with no routes at all. The
// decision therefore inspects the resolved DSH home first — the `llm-tmax`
// entry in `cordis.patch.yml` and the `tmax-dsh.mjs` artifact — and only then
// combines it with the live provider registry.
import { readFile, stat } from 'node:fs/promises'
import { join } from 'node:path'

/** Ownership modes accepted from plugin configuration. */
export function normalizeManageRoutes(value) {
  if (value === undefined || value === null || value === '' || value === 'auto') return 'auto'
  if (value === true || value === 'true') return true
  if (value === false || value === 'false') return false
  // An unrecognized value must not silently turn into either extreme.
  return 'auto'
}

/**
 * Strip YAML comments while keeping quoted scalars intact.
 *
 * A commented-out entry (`# - id: llm-tmax`) is not an installed entry, and a
 * naive search would count it. Quoted values may legitimately contain `#`
 * (paths, embedded JSON strings), so a `#` only starts a comment at the start
 * of a line or after whitespace, outside single/double quotes.
 */
export function stripYamlComments(text) {
  if (typeof text !== 'string' || text === '') return ''
  const lines = []
  for (const line of text.split('\n')) {
    let kept = ''
    let quote = null
    let escaped = false
    for (let index = 0; index < line.length; index += 1) {
      const char = line[index]
      if (quote === '"') {
        kept += char
        if (escaped) escaped = false
        else if (char === '\\') escaped = true
        else if (char === '"') quote = null
        continue
      }
      if (quote === "'") {
        kept += char
        if (char !== "'") continue
        // YAML single quotes escape by doubling; the second quote is content.
        if (line[index + 1] === "'") {
          kept += line[index + 1]
          index += 1
          continue
        }
        quote = null
        continue
      }
      if (char === '"' || char === "'") {
        quote = char
        kept += char
        continue
      }
      if (char === '#' && (index === 0 || /\s/u.test(line[index - 1]))) break
      kept += char
    }
    lines.push(kept)
  }
  return lines.join('\n')
}

/** Whether a parsed DSH home patch declares the `llm-tmax` entry. */
export function hasHomeEntry(text) {
  if (typeof text !== 'string' || text === '') return false
  // The home patch is a YAML sequence, but tmax writes the managed entry as an
  // embedded JSON object (`- {"insert":[{...,"id":"llm-tmax",...}]}`). Accept
  // both forms: a quoted or bare `id` key immediately followed by llm-tmax.
  // Comments are removed first: a commented-out entry is not an installed one.
  return /(?:^|[\s,{}\[])\s*["']?id["']?\s*:\s*["']?llm-tmax["']?(?=$|[\s,}\]])/m.test(stripYamlComments(text))
}

async function isRegularFile(path) {
  try {
    return (await stat(path)).isFile()
  } catch {
    return false
  }
}

/**
 * Provider route ids from `ctx.llm.listProviders()`.
 * Real DSH returns `{id, name}` records; a plain string list stays accepted so
 * a live-registry double and older assemblies keep working. An entry without a
 * usable id contributes nothing — never a stringified object.
 */
export function providerIds(providers) {
  if (!Array.isArray(providers)) return []
  const ids = []
  for (const entry of providers) {
    if (typeof entry === 'string') {
      if (entry !== '') ids.push(entry)
    } else if (entry !== null && typeof entry === 'object' && typeof entry.id === 'string' && entry.id !== '') {
      ids.push(entry.id)
    }
  }
  return ids
}

/** Live route ids from the plugin context; absence or a throwing service is an empty registry. */
export function liveProviderIds(ctx) {
  try {
    return providerIds(ctx?.llm?.listProviders?.())
  } catch {
    return []
  }
}

/**
 * Inspect the resolved DSH home for the two facts that prove the home plugin
 * is installed: the patch entry and the embedded artifact.
 * @returns {Promise<{present: boolean, entry: boolean, artifact: boolean}>}
 */
export async function inspectHome(home) {
  const entryPath = join(home, 'cordis.patch.yml')
  const artifactPath = join(home, 'tmax-dsh.mjs')
  let entry = false
  try {
    entry = hasHomeEntry(await readFile(entryPath, 'utf8'))
  } catch {
    entry = false
  }
  const artifact = await isRegularFile(artifactPath)
  return { present: entry && artifact, entry, artifact }
}

/**
 * Decide the route owner from the home integration, the live registry, and
 * this plugin's own registration state.
 *
 * @param {object} input
 * @param {'auto'|true|false} input.mode
 * @param {{present: boolean, entry: boolean, artifact: boolean}} input.home
 * @param {object|undefined} input.projection
 * @param {string[]} input.liveNames
 * @param {string[]} input.registeredNames - route names this plugin registered
 * @param {string} [input.prefix]
 * @returns {{
 *   owner: 'home'|'page'|'other'|'none'|'unknown',
 *   scope: 'home_wide'|'profile_scoped'|null,
 *   reason: string|null,
 *   homeIntegration: {present: boolean, entry: boolean, artifact: boolean},
 *   conflict: {code: string, message: string, routes?: string[]}|null,
 *   shouldRegister: boolean,
 *   disposePage: boolean,
 * }}
 */
export function resolveOwnership({ mode, home, projection, liveNames = [], registeredNames = [], prefix = '' }) {
  const homeIntegration = {
    present: home?.present === true,
    entry: home?.entry === true,
    artifact: home?.artifact === true,
  }
  const projectedNames = (projection?.routes || [])
    .filter(route => route !== null && typeof route === 'object' && typeof route.name === 'string' && route.name !== '')
    .map(route => `${prefix}${route.name}`)
  const pageRegistered = registeredNames.length > 0
  const live = new Set(liveNames)
  const registered = new Set(registeredNames)
  const foreignLive = projectedNames.filter(name => live.has(name) && !registered.has(name))
  const anyLive = projectedNames.some(name => live.has(name))
  const conflict = (code, message, routes) => ({ code, message, ...(routes === undefined ? {} : { routes }) })

  const decision = (owner, scope, reason, extras = {}) => ({
    mode,
    owner,
    scope,
    reason,
    homeIntegration,
    conflict: extras.conflict ?? null,
    shouldRegister: extras.shouldRegister === true,
    disposePage: extras.disposePage === true,
  })

  if (mode === false) {
    // The page never registers either way; reporting the home owner (when it is
    // installed) keeps the state truthful instead of claiming no integration.
    if (homeIntegration.present) return decision('home', 'home_wide', null, { disposePage: pageRegistered })
    if (homeIntegration.entry !== homeIntegration.artifact) {
      const missing = homeIntegration.entry ? 'tmax-dsh.mjs artifact' : 'llm-tmax entry'
      return decision('unknown', null, null, {
        disposePage: pageRegistered,
        conflict: conflict(
          'home_integration_partial',
          `The DSH home declares only part of the tmax home plugin (missing ${missing}); routes are not registered`,
        ),
      })
    }
    return decision('none', null, 'route_management_disabled', { disposePage: pageRegistered })
  }

  if (mode === true) {
    if (homeIntegration.present) {
      return decision('page', 'profile_scoped', 'home_integration_present_forced', {
        shouldRegister: true,
        conflict: conflict(
          'home_integration_present',
          'manageRoutes=true forces profile-scoped routes while the home plugin is installed; duplicate registration can result',
        ),
      })
    }
    return decision('page', 'profile_scoped', null, {
      shouldRegister: true,
      ...(foreignLive.length === 0
        ? {}
        : { conflict: conflict('route_owner_conflict', `Routes are already registered by another provider: ${foreignLive.join(', ')}`, foreignLive) }),
    })
  }

  // auto
  if (homeIntegration.present) {
    if (pageRegistered) {
      // Keep the working page routes until the DSH reload that activates the
      // home plugin; dropping them here would strand the profile.
      return decision('page', 'profile_scoped', 'home_integration_pending_reload', { shouldRegister: true })
    }
    return decision('home', 'home_wide', anyLive ? null : 'home_integration_reload_pending')
  }
  if (homeIntegration.entry !== homeIntegration.artifact) {
    const missing = homeIntegration.entry ? 'tmax-dsh.mjs artifact' : 'llm-tmax entry'
    return decision('unknown', null, null, {
      conflict: conflict(
        'home_integration_partial',
        `The DSH home declares only part of the tmax home plugin (missing ${missing}); routes are not registered until it is complete`,
      ),
    })
  }
  if (foreignLive.length > 0) {
    return decision('other', null, null, {
      conflict: conflict(
        'route_owner_conflict',
        `Projected routes are already registered by another provider: ${foreignLive.join(', ')}`,
        foreignLive,
      ),
    })
  }
  return decision('page', 'profile_scoped', null, { shouldRegister: true })
}
