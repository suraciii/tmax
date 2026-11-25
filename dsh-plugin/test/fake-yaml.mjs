/** DSH's entry-list parser for the test fakes: block YAML as the fixtures write it. */
/** A block-YAML reader for the fixtures: `- key: value` items, nested mappings
 * and sequences by indentation, and plain or quoted scalars. */
export function parseBlocks(text) {
  const lines = []
  for (const raw of text.split('\n')) {
    if (raw.includes('\t')) throw new Error('a tab is not YAML indentation')
    const trimmed = raw.trim()
    if (trimmed === '' || trimmed.startsWith('#')) continue
    lines.push({ indent: raw.length - raw.trimStart().length, text: trimmed })
  }
  let pos = 0
  const scalar = value => {
    const trimmed = value.trim()
    if (/^".*"$/.test(trimmed)) return JSON.parse(trimmed)
    if (/^'.*'$/.test(trimmed)) return trimmed.slice(1, -1)
    return /^-?\d+(\.\d+)?$/.test(trimmed) ? Number(trimmed) : trimmed
  }
  const pair = text => {
    const match = /^([^:]+):(?:\s*(.*))?$/.exec(text)
    return { key: match[1].trim(), rest: match[2] ?? '' }
  }
  const assign = (map, { key, rest }, indent) => {
    if (rest.trim() !== '') { map[key] = scalar(rest); return }
    const next = lines[pos]
    map[key] = next && next.indent > indent ? parseBlock(next.indent) : null
  }
  const parseMap = indent => {
    const map = {}
    while (pos < lines.length && lines[pos].indent === indent && !lines[pos].text.startsWith('- ')) {
      const entry = pair(lines[pos].text)
      pos += 1
      assign(map, entry, indent)
    }
    return map
  }
  const parseSeq = indent => {
    const items = []
    while (pos < lines.length && lines[pos].indent === indent && lines[pos].text.startsWith('- ')) {
      const first = pair(lines[pos].text.slice(2).trim())
      pos += 1
      const map = {}
      assign(map, first, indent + 2)
      while (pos < lines.length && lines[pos].indent === indent + 2 && !lines[pos].text.startsWith('- ')) {
        const entry = pair(lines[pos].text)
        pos += 1
        assign(map, entry, indent + 2)
      }
      items.push(map)
    }
    return items
  }
  const parseBlock = indent => (lines[pos]?.text.startsWith('- ') ? parseSeq(indent) : parseMap(indent))
  return parseBlock(lines[0]?.indent ?? 0)
}
