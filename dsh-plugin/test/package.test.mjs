// What a package-manager install ships: Git and tarball installs copy only the
// package's `files`, and the linked checkout `just dsh-install` uses hides that
// difference, so a runtime module missing from `files` fails only for users who
// installed from a URL. This test walks the bundle's own declarations and
// relative imports and requires `files` to cover every one of them.

import assert from 'node:assert/strict'
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, posix } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../..', import.meta.url))
const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const files = manifest.files ?? []

/** Whether one package-relative path is shipped: a literal file, a directory, or a one-level glob. */
function shipped(file) {
  return files.some((pattern) => {
    if (!pattern.includes('*')) return file === pattern || file.startsWith(`${pattern.replace(/\/+$/u, '')}/`)
    const base = pattern.slice(0, pattern.lastIndexOf('/'))
    const suffix = pattern.slice(pattern.lastIndexOf('*') + 1)
    const name = file.slice(base.length + 1)
    return file.startsWith(`${base}/`) && !name.includes('/') && name.endsWith(suffix)
  })
}

/** Every path an export pattern names, expanding its `*` over the files beside it. */
function expand(pattern) {
  const star = pattern.lastIndexOf('*')
  if (star === -1) return [pattern]
  const directory = pattern.slice(0, pattern.lastIndexOf('/'))
  return readdirSync(join(root, directory))
    .filter(name => name.endsWith(pattern.slice(star + 1)))
    .map(name => `${directory}/${name}`)
}

/** Package-relative paths the bundle declares: exported resources and patch layers. */
function declared() {
  const patch = manifest.dsh?.bundle?.patch
  return [
    ...Object.values(manifest.exports).flatMap(expand),
    ...[patch].flat().filter(file => typeof file === 'string'),
  ].map(file => file.replace(/^\.\//u, ''))
}

/** One module's relative module references, as package-relative paths. */
function imports(file) {
  const reference = /(?:\bfrom\s*|\bimport\s*\(?\s*)['"](\.{1,2}\/[^'"]+)['"]/gu
  const source = readFileSync(join(root, file), 'utf8')
  return [...source.matchAll(reference)].map(match => posix.join(dirname(file), match[1]))
}

/** Every module reachable from the declared runtime entry points. */
function reachable() {
  const visited = new Set()
  const queue = declared().filter(file => file.endsWith('.js'))
  while (queue.length > 0) {
    const file = queue.shift()
    if (visited.has(file)) continue
    visited.add(file)
    for (const next of imports(file)) if (!visited.has(next)) queue.push(next)
  }
  return [...visited]
}

/** Markdown documents the shipped locales name as the bundle's detail page. */
function documents() {
  return declared()
    .filter(file => file.endsWith('.json'))
    .flatMap((file) => {
      const meta = JSON.parse(readFileSync(join(root, file), 'utf8')).meta
      return typeof meta?.detail === 'string' ? [meta.detail] : []
    })
}

test('every declared resource, document, and runtime module is listed in files', () => {
  for (const file of declared().concat(documents())) assert.ok(shipped(file), `${file} is declared but not shipped by files`)
  for (const file of reachable()) assert.ok(shipped(file), `${file} is imported but not shipped by files`)
})

test('every declared resource, document, and runtime module exists in the checkout', () => {
  for (const file of declared().concat(documents(), reachable())) {
    const path = join(root, file)
    assert.ok(existsSync(path) && statSync(path).isFile(), `${file} is declared but missing from the checkout`)
  }
})

test('the bundle declares the resources DSH resolves', () => {
  const exports = manifest.exports ?? {}
  for (const key of ['.', './client', './icon', './locale/*.json']) {
    assert.ok(typeof exports[key] === 'string', `package.json exports ${JSON.stringify(key)} is missing`)
  }
  assert.equal(manifest.dsh?.bundle?.patch, './dsh-plugin/cordis.patch.yml')
  assert.equal(manifest.dsh?.client?.platform, 'web')
  assert.ok(declared().includes('locale/en.json'), 'the English locale DSH reads for display text is declared')
})

test('the package identity is one manifest at the repository root', () => {
  assert.equal(manifest.name, 'tmax')
  assert.equal(manifest.private, true)
  assert.equal(manifest.type, 'module')
  assert.ok(manifest.engines?.dsh, 'the package declares the DSH versions it supports')
  assert.ok(!existsSync(join(root, 'dsh-plugin/package.json')), 'the plugin must not carry a second manifest')
})
