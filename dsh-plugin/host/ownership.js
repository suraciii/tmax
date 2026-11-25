// Route ownership: DSH serves the providers tmax applies, so the page owns
// nothing and reports what the profile settings declare.
//
// `tmax agent apply` writes one `tmax-<entry>` provider into a profile's
// settings document, and the client's own adapter mounts it. A page that
// registered the same upstreams would offer every model twice, so ownership is
// a fact read from those documents rather than a race between two plugins.
import { readdir, readFile, stat } from 'node:fs/promises'
import { join } from 'node:path'

/**
 * Provider names the profile settings declare under the `llm-pi-ai` row.
 *
 * The document is a YAML list of loader patch entries, so the row and its
 * `config.providers` mapping are read structurally with DSH's own parser: a
 * comment, a field of another row, or a substring is never mistaken for a
 * provider, and a document that fails to parse contributes nothing.
 * @param {string[]} documents - profile `cordis.patch.yml` texts, if any
 * @param {(text: string) => unknown} parse - DSH's entry-list parser
 * @returns {Set<string>} every provider name any document declares
 */
export function appliedProviderNames(documents, parse) {
  const names = new Set()
  if (typeof parse !== 'function') return names
  for (const text of documents || []) {
    if (typeof text !== 'string') continue
    let rows
    try {
      rows = parse(text)
    } catch {
      continue
    }
    for (const row of Array.isArray(rows) ? rows : []) {
      const config = row !== null && typeof row === 'object' && row.id === 'llm-pi-ai' ? row.config : null
      const providers = config !== null && typeof config === 'object' ? config.providers : null
      if (providers === null || typeof providers !== 'object' || Array.isArray(providers)) continue
      for (const name of Object.keys(providers)) if (name !== '') names.add(name)
    }
  }
  return names
}

/** Route ids the registry reports, from either strings or `{id}` objects. */
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
 * The state's ownership section.
 *
 * `settings` means the applied profile settings carry every route the target
 * declares and DSH serves them; `unapplied` means apply has not written them
 * all; `none` means the target declares no route. Ownership and conflict are
 * decided per route: a route that is live without being in the applied
 * settings is served by someone else, whatever the other routes say.
 * @param {object} input
 * @param {object} [input.projection] - the target inventory, when one was read
 * @param {string[]} [input.liveNames] - provider ids the registry reports
 * @param {Set<string>} [input.applied] - provider names the settings declare
 * @param {boolean} [input.parsed] - false when no parser was available
 * @returns {object} the ownership decision the state exposes
 */
export function resolveOwnership({ projection, liveNames = [], applied, parsed = true } = {}) {
  const declared = []
  for (const route of Array.isArray(projection?.routes) ? projection.routes : []) {
    // The registry names providers as the settings keys spell them, without
    // the page's display prefix.
    if (route !== null && typeof route === 'object' && typeof route.name === 'string' && route.name !== '' && !declared.includes(route.name)) {
      declared.push(route.name)
    }
  }
  if (parsed !== true || !(applied instanceof Set)) {
    return { owner: 'unknown', scope: null, reason: 'settings_unreadable', conflict: null }
  }
  const live = new Set(Array.isArray(liveNames) ? liveNames : [])
  const inSettings = declared.filter(name => applied.has(name))
  const foreign = declared.filter(name => live.has(name) && !applied.has(name))
  const allApplied = declared.length > 0 && inSettings.length === declared.length
  // A target whose every route is served by someone else, with nothing of its
  // own applied, belongs to that other owner outright.
  const owner = allApplied
    ? 'settings'
    : declared.length === 0
      ? 'none'
      : inSettings.length === 0 && foreign.length === declared.length ? 'other' : 'unapplied'
  return {
    owner,
    scope: allApplied ? 'profile_scoped' : null,
    reason: owner === 'unapplied' && inSettings.length === 0 ? 'apply_not_run' : null,
    conflict: foreign.length === 0
      ? null
      : {
        code: 'route_owner_conflict',
        message: `These routes are live but not in the applied settings, so another owner serves them: ${foreign.join(', ')}`,
        routes: foreign,
      },
  }
}

/**
 * Read a home's profile settings documents, if any.
 * @param {string} home - resolved DSH home
 * @returns {Promise<string[]>} every readable `profiles/<name>/cordis.patch.yml`
 */
export async function readProfileDocuments(home) {
  const root = join(home, 'profiles')
  let names
  try {
    names = await readdir(root)
  } catch {
    return []
  }
  const documents = []
  for (const name of names) {
    try {
      if (!(await stat(join(root, name))).isDirectory()) continue
    } catch {
      continue
    }
    try {
      documents.push(await readFile(join(root, name, 'cordis.patch.yml'), 'utf8'))
    } catch {
      // A profile without a settings document has no applied providers.
    }
  }
  return documents
}
