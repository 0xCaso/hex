// PROTOTYPE (site/voice-game): forgiving word matching for the voice game. Speech recognizers
// return plurals, tenses, homophones and near misses; any of those should still count.

export const norm = (w) =>
  w
    .toLowerCase()
    .replace(/[’']/g, "")
    .replace(/[^a-z0-9]+/g, "")

// Words the recognizer tends to hear instead of the card word.
const SOUNDS_LIKE = {
  red: ["read", "redd"],
  main: ["maine", "mane", "mean"],
  ship: ["shipped", "chip", "sheep", "shipit"],
  merge: ["merged", "murch", "merch"],
  fix: ["fixed", "fixes", "fics", "fixit"],
  tests: ["test", "testing", "tested", "taste", "tastes"],
  docs: ["doc", "docks", "dock", "ducks"],
  tabs: ["tab", "tubs"],
  hex: ["hacks", "hecks", "x", "heck", "hexes"],
  done: ["dun", "dan"],
  green: ["greene", "grin"],
  agents: ["agent", "asians"],
  revert: ["reverts", "reverted"],
  commit: ["comet", "commits", "committed"],
  rebase: ["re-base", "rebased", "rebates"],
  option: ["options", "auction"],
  paste: ["pace", "paced", "pasted", "taste"],
  local: ["locally", "vocal"],
  dictate: ["dictated", "dictation"],
  deploy: ["deployed", "deploys"],
  review: ["reviewed", "reviews"],
  coffee: ["copy", "coffees"],
  lunch: ["launch"],
  speak: ["speaks", "speech", "spoke"],
  appears: ["appear", "appeared", "a peers"],
  talk: ["torque", "talks", "talking"],
  typing: ["typed", "type", "tiping"],
  issue: ["issues", "tissue"],
  close: ["closed", "clothes"],
  spawn: ["spawned", "spon", "spun", "spawns"],
  friday: ["fridays"],
  build: ["built", "billed", "builds"],
  now: ["know", "no"],
  bug: ["bugs", "buck"],
  private: ["privately"],
  fast: ["faster", "fest"],
  terminal: ["terminals"],
}
const ALIASES = new Map()
for (const [k, list] of Object.entries(SOUNDS_LIKE)) for (const w of list) ALIASES.set(norm(w), k)

// Drop common endings so "merged", "merging" and "merges" meet at "merg".
export function stem(w) {
  let s = w
  for (const end of ["ing", "ied", "ies", "es", "ed", "ly", "s"]) {
    if (s.length > end.length + 2 && s.endsWith(end)) {
      s = s.slice(0, -end.length)
      break
    }
  }
  if (s.length > 3 && s[s.length - 1] === s[s.length - 2]) s = s.slice(0, -1) // shipp → ship
  if (s.length > 3 && s.endsWith("e")) s = s.slice(0, -1)
  return s
}

function lev(a, b) {
  if (a === b) return 0
  const m = a.length
  const n = b.length
  if (!m || !n) return m || n
  let prev = Array.from({ length: n + 1 }, (_, j) => j)
  for (let i = 1; i <= m; i++) {
    const cur = [i]
    for (let j = 1; j <= n; j++) cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1))
    prev = cur
  }
  return prev[n]
}

// A rough phonetic key: first letter plus consonant classes, vowels and repeats removed.
function sound(w) {
  const map = { b: 1, f: 1, p: 1, v: 1, c: 2, g: 2, j: 2, k: 2, q: 2, s: 2, x: 2, z: 2, d: 3, t: 3, l: 4, m: 5, n: 5, r: 6 }
  const s = w.replace(/^kn|^wr/, (m) => m[1]).replace(/ph/g, "f").replace(/ck/g, "k").replace(/sh|ch/g, "s")
  let out = s[0] ?? ""
  let last = map[s[0]]
  for (let i = 1; i < s.length; i++) {
    const code = map[s[i]]
    if (code && code !== last) out += code
    last = code
  }
  return out
}

// 1 for a sure match, down to ~0.6 for a plausible one, 0 for none.
export function score(heard, target) {
  const h = norm(heard)
  const t = norm(target)
  if (!h || !t) return 0
  if (h === t) return 1
  if (ALIASES.get(h) === t) return 0.95
  if (stem(h) === stem(t)) return 0.92
  if (t.length < 4) return 0
  const d = lev(h, t)
  const max = t.length >= 7 ? 2 : 1
  if (d <= max) return 0.85 - d * 0.08
  if (t.length >= 5 && sound(h) === sound(t) && Math.abs(h.length - t.length) <= 2) return 0.62
  return 0
}

// Splits a transcript into words, keeping joins like "re base" → "rebase" available as pairs.
export const words = (text) =>
  text
    .split(/\s+/)
    .map((w) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter(Boolean)
