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
 * Whether the given settings documents declare one of tmax's providers.
 *
 * A provider carries its entry's name, so the names come from the inventory
 * rather than from a naming convention: a name that appears as a mapping key
 * in the profile's settings is a provider the client already serves.
 * @param {string[]} documents - profile `cordis.patch.yml` texts, if any
 * @param {string[]} names - provider names the target declares
 * @returns {boolean} true when at least one applied provider is present
 */
export function hasAppliedProviders(documents, names) {
  const texts = (documents || []).filter(text => typeof text === 'string')
  if (texts.length === 0) return false
  return (names || []).some(name => {
    if (typeof name !== 'string' || name === '') return false
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')
    const key = new RegExp(`(?:^|[\\s,{[])\\s*["']?${escaped}["']?\\s*:`, 'u')
    return texts.some(text => key.test(text))
  })
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
 * `settings` means the applied profile settings carry tmax's providers and DSH
 * serves them; `none` means apply has not written them yet, which is the same
 * thing the state's route list reports as not live.
 * @param {object} input
 * @param {object} [input.projection] - the target inventory, when one was read
 * @param {boolean} [input.settingsProviders] - applied providers exist
 * @returns {object} the ownership decision the state exposes
 */
export function resolveOwnership({ projection, liveNames = [], prefix = '', settingsProviders = false } = {}) {
  const declared = Array.isArray(projection?.routes)
    ? projection.routes.filter(route => route !== null && typeof route === 'object' && typeof route.name === 'string')
    : []
  const live = new Set(Array.isArray(liveNames) ? liveNames : [])
  // Another plugin serving these names while tmax's own settings carry none is
  // a real duplicate-route risk, not an empty state.
  const foreign = settingsProviders
    ? []
    : declared.map(route => `${prefix}${route.name}`).filter(name => live.has(name))
  const owner = foreign.length > 0 ? 'other' : settingsProviders ? 'settings' : declared.length === 0 ? 'none' : 'unapplied'
  return {
    owner,
    scope: settingsProviders ? 'profile_scoped' : null,
    reason: owner === 'unapplied' ? 'apply_not_run' : null,
    conflict: foreign.length === 0
      ? null
      : {
        code: 'route_owner_conflict',
        message: `These routes are already registered by another provider: ${foreign.join(', ')}`,
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
