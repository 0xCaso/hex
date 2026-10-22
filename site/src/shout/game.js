// PROTOTYPE (site/voice-game): "Play with your voice". Notes fly at the visitor; saying a ghosted
// word makes Bean shout it into the most urgent note that has it. Filled notes explode, missed
// ones hit the glass. Loaded only when the visitor opts in.
import { norm, score, stem, words as split } from "./match.js"

const ROUND = 50 // seconds
const Z_FAR = 2.3 // a note starts at 1 / Z_FAR of its final size

const TEAM = ["Dax", "Jay", "Frank"]
const PEOPLE = ["Dax", "Jay", "Frank", "Ana Silva", "Sam Ortiz", "Priya Das", "Lee Park", "hex-ci"]
const NOTES = [
  // One word to say.
  ["[ship] it", "can we [ship] today", "[merge] my PR", "run the [tests]", "who broke [main]", "the build is [red]", "[deploy] on friday?", "[fix] the build", "add more [agents]", "is it [done] yet", "[review] please", "write the [docs]", "too many [tabs]", "need [coffee]", "[lunch]? it's 4pm", "hold [option] to talk", "[paste] it anywhere", "it runs [local]", "your voice stays [private]", "just [dictate] it", "[rebase] onto main", "file a [bug]"],
  // Two.
  ["[ship] the [fix]", "[merge] it [now]", "[talk] to your [terminal]", "[spawn] more [agents]", "[revert] that [commit]", "[close] the [issue]", "the [tests] are [green]", "[faster] than [typing]", "[fix] the [bug]", "[review] my [code]"],
  // Three.
  ["[speak] it [appears] [instantly]", "[hold] [option] and [talk]", "[ship] the [fix] [today]", "[merge] [deploy] [repeat]"],
]
const JUNK = new Set(["you", "thankyou", "thanks", "bye", "uh", "um", "hmm", "mm", "oh", "ah", "so", "the", "a"])

export function createGame(api, { reduced, onEvent }) {
  const { rand, rnd, cards } = api
  const notes = []
  let phase = "ready" // ready → count → play → over | quit
  let t = 0 // seconds into the round
  let spawnIn = 0
  let level = 0
  let mic = 0
  let stats = { score: 0, combo: 0, best: 0, words: 0, broken: 0, missed: 0 }
  let lastEmit = 0
  const segs = new Map()
  const lat = []

  const emit = (force = false) => {
    const now = performance.now()
    if (!force && now - lastEmit < 90) return
    lastEmit = now
    onEvent?.({ type: "state", phase, left: Math.max(0, ROUND - t), ...stats })
  }

  const vp = () => ({ x: api.W() * 0.5, y: api.H() * 0.46 })
  const finalScale = () => api.cardScale() * (api.W() < 900 ? 1.05 : 1.32)

  // Test hook: ?deck=ship,tests,… limits notes to those words (for scripted microphone runs).
  const deck = new URLSearchParams(location.search).get("deck")?.split(",").filter(Boolean)
  function pickPhrase() {
    if (deck) {
      const onScreen = new Set(notes.map((c) => norm(c.reply.words[c.targets[0]].text)))
      const free = deck.filter((w) => !onScreen.has(norm(w)))
      const w = (free.length ? free : deck)[Math.floor(rand() * (free.length || deck.length))]
      return NOTES[0].find((p) => p.includes(`[${w}]`)) ?? `say [${w}]`
    }
    const k = t / ROUND
    const tier = k < 0.22 ? 0 : k < 0.55 ? (rand() < 0.6 ? 1 : 0) : rand() < 0.3 ? 2 : 1
    const onScreen = new Set(notes.flatMap((c) => c.targets.map((i) => norm(c.reply.words[i].text))))
    for (let n = 0; n < 12; n++) {
      const p = NOTES[tier][Math.floor(rand() * NOTES[tier].length)]
      const fresh = [...p.matchAll(/\[(.+?)\]/g)].every((m) => !onScreen.has(norm(m[1])))
      if (fresh || n === 11) return p
    }
  }

  // Where a note will be when it reaches the glass: spread out, clear of Bean's face and of
  // where the other notes are heading.
  function landing(hw, hh) {
    const W = api.W()
    const H = api.H()
    const hp = api.headPos()
    const R = api.headR()
    let best = null
    let bestCost = Infinity
    for (let n = 0; n < 40; n++) {
      const x = rnd(Math.min(hw + 16, W / 2), Math.max(W - hw - 16, W / 2))
      const y = rnd(Math.min(hh + 70, H / 2), Math.max(H - hh - 24, H / 2))
      let cost = 0
      const dx = Math.max(0, Math.abs(x - hp.x) - hw)
      const dy = Math.max(0, Math.abs(y - hp.y) - hh)
      if (Math.hypot(dx, dy) < R * 1.2) cost += 1000
      for (const o of notes) {
        const d = Math.hypot((o.ex - x) / (hw * 2), (o.ey - y) / (hh * 2.4))
        cost += d < 1 ? 200 * (1 - d) * (1 - o.u) : 0
      }
      cost += Math.abs(x - W / 2) < W * 0.12 ? 20 : 0
      if (cost < bestCost) {
        best = { x, y }
        bestCost = cost
      }
    }
    return best
  }

  function spawn() {
    const c = cards.make("note", rand, [pick(t < 8 ? TEAM : PEOPLE)])
    if (!cards.alloc(c)) return
    cards.planNote(c, pickPhrase())
    const s = finalScale()
    const hw = (c.w / 2) * s
    const hh = (c.h / 2) * s
    const e = landing(hw, hh)
    const life = reduced ? lerp(8.5, 6, level) : lerp(8.5, 4.8, level)
    Object.assign(c, {
      game: true,
      state: "typing",
      typing: false,
      loud: true,
      heat: 0,
      words: c.targets.length,
      landed: 0,
      claimed: new Set(),
      left: c.targets.length,
      ex: e.x,
      ey: e.y,
      u: 0,
      life,
      born: api.time(),
      age: 0,
      rot0: rnd(-0.07, 0.07),
      s1: s,
      x: e.x,
      y: e.y,
      scale: 0,
      alpha: 0,
      rot: 0,
      flash: 0,
      vx: 0,
      vy: 0,
      seed: rand() * 100,
      kx: { x: 0, v: 0 },
      ky: { x: 0, v: 0 },
      sw: c.sw,
      sh: c.sh,
      hw,
      hh,
      timer: reduced ? 0 : null,
    })
    cards.draw(c)
    api.live.push(c)
    notes.push(c)
    api.audio.sfx("pop", { gain: 0.35, rate: rnd(0.95, 1.08), pan: (e.x / api.W()) * 1.4 - 0.7 })
  }

  function place(c, dt) {
    const p = vp()
    // Once every blank is claimed the note waits for its words instead of hitting the glass.
    if (c.claimed.size < c.targets.length || c.state !== "typing") c.u = Math.min(1, c.u + dt / c.life)
    c.age += dt
    const fade = clamp(c.age / 0.22, 0, 1)
    let x
    let y
    let scale
    if (reduced) {
      x = c.ex
      y = c.ey
      scale = c.s1 * 0.82
      const shown = Math.round(c.u * 20)
      if (shown !== c.shownTimer) {
        c.shownTimer = shown
        c.timer = c.u
        cards.draw(c)
      }
    } else {
      // A straight line toward the camera: perspective from the vanishing point.
      const z = lerp(Z_FAR, 1, c.u)
      x = p.x + (c.ex - p.x) / z + Math.sin(api.time() * 1.3 + c.seed) * 3
      y = p.y + (c.ey - p.y) / z + Math.cos(api.time() * 1.1 + c.seed) * 3
      scale = (c.s1 / z) * lerp(0.96, 1, fade)
    }
    // Landing words knock the note; it springs back.
    knock(c.kx, dt)
    knock(c.ky, dt)
    x += c.kx.x * 0.4
    y += c.ky.x * 0.4
    if (dt > 0) {
      c.vx = (x - c.x) / dt
      c.vy = (y - c.y) / dt
    }
    c.x = x
    c.y = y
    c.scale = scale
    c.alpha = fade
    c.rot = c.rot0 * (1 - c.u * 0.5)
  }

  // A note reached the glass with blanks left.
  function hitGlass(c) {
    const W = api.W()
    stats.missed++
    stats.combo = 0
    const p = vp()
    c.hitDir = { x: c.x - p.x, y: c.y - p.y }
    api.screenBreak.addCrack(c.x, c.y, 0.8, false, 0.42)
    api.audio.sfx(rand() < 0.5 ? "glass-crack-1" : "glass-crack-2", { bus: "impacts", gain: 0.42, rate: rnd(0.95, 1.06), pan: (c.x / W) * 1.2 - 0.6, dur: 0.24 })
    if (reduced) c.state = "leaving"
    else api.explode(c, 0.7)
    if (!reduced) {
      api.cam.trauma = Math.min(1, api.cam.trauma + 0.45)
      api.hitStop.remaining = Math.max(api.hitStop.remaining, 0.03)
    }
    emit(true)
  }

  // Best unclaimed blank for a heard word: prefer sure matches, then the note closest to the glass.
  function find(heard, next) {
    let best = null
    for (const c of notes) {
      if (c.state !== "typing") continue
      for (const i of c.targets) {
        if (c.claimed.has(i)) continue
        const target = c.reply.words[i].text
        let s = score(heard, target)
        let pair = false
        if (next && s < 0.9) {
          const joined = score(heard + next, target)
          if (joined > s) {
            s = joined
            pair = true
          }
        }
        if (s < 0.6) continue
        const rank = s + c.u * 0.5
        if (!best || rank > best.rank) best = { c, i, s, rank, pair }
      }
    }
    return best
  }

  function shout(text, match, meta) {
    stats.words++
    if (match) {
      const { c, i } = match
      c.claimed.add(i)
      c.left = c.targets.length - c.claimed.size
      cards.draw(c)
      const intensity = clamp(0.45 + level * 0.4 + stats.combo * 0.03, 0, 1)
      if (reduced) fillNow(c, i)
      else api.launch(c.reply.words[i].text, c, { slot: i, i: intensity, size: lerp(34, 60, intensity) })
      stats.score += 25 * (1 + Math.min(stats.combo, 9))
    } else {
      // Not on any note: Bean still shouts it, and it pops in the air.
      const hp = api.headPos()
      const R = api.headR()
      const a = Math.PI / 2 + rnd(-1.15, 1.15)
      const d = R * rnd(1.9, 2.8)
      if (!reduced) api.launch(clean(text), null, { point: { x: hp.x + Math.cos(a) * d * 1.4, y: hp.y + Math.sin(a) * d }, i: 0.3, size: 26 })
    }
    if (meta?.speechEnd != null) lat.push({ text, matched: !!match, ...meta, launchedAt: performance.now() })
    mic = Math.max(mic, 0.5)
    emit()
  }

  // A transcript (interim or final) for one utterance. Words already shouted for this utterance
  // are matched by content, so a revised hypothesis never shouts the same word twice.
  function hear(text, { seg = "typed", final = true, meta } = {}) {
    if (phase !== "play" && phase !== "ready" && phase !== "count") return
    const list = split(text)
    if (!list.length) return
    if (final && list.length <= 2 && JUNK.has(norm(list.join("")))) return
    const st = segs.get(seg) ?? { done: new Map() }
    segs.set(seg, st)
    const seen = new Map(st.done)
    for (let k = 0; k < list.length; k++) {
      const w = list[k]
      const key = stem(norm(w))
      if (!key || key.length > 18) continue
      if ((seen.get(key) ?? 0) > 0) {
        seen.set(key, seen.get(key) - 1)
        continue
      }
      const last = k === list.length - 1
      const m = phase === "play" ? find(w, list[k + 1]) : null
      // Interim words fly only when they fill a blank; the rest wait for the final transcript.
      if (!final && (!m || (last && m.s < 0.9))) continue
      st.done.set(key, (st.done.get(key) ?? 0) + 1)
      if (m?.pair) {
        const k2 = stem(norm(list[k + 1]))
        st.done.set(k2, (st.done.get(k2) ?? 0) + 1)
        k++
      }
      shout(m?.pair ? m.c.reply.words[m.i].text : w, m, meta)
    }
    if (final) segs.delete(seg)
    onEvent?.({ type: "heard", text: list.map(clean).join(" "), final, seg })
  }

  // Reduced motion: the word appears in its blank and a finished note fades, nothing travels.
  function fillNow(c, i) {
    c.lw[i] = { fade: 1, heat: 0, cool: 0.3, age: 1 }
    c.landed++
    cards.draw(c)
    if (c.landed >= c.words) {
      broke(c)
      c.state = "leaving"
    }
  }

  function broke(c) {
    stats.combo++
    stats.best = Math.max(stats.best, stats.combo)
    stats.broken++
    stats.score += 100 * c.targets.length * Math.min(1 + stats.combo, 10)
    emit(true)
  }

  function countdown() {
    phase = "count"
    const hp = api.headPos()
    ;["3", "2", "1", "Go!"].forEach((w, k) =>
      setTimeout(() => {
        if (phase !== "count") return
        if (!reduced) api.launch(w, null, { point: { x: hp.x + (k - 1.5) * 90, y: hp.y + api.headR() * 2.4 }, i: k === 3 ? 0.9 : 0.4, size: k === 3 ? 56 : 40 })
        if (k === 3) {
          phase = "play"
          t = 0
          spawnIn = 0.4
          emit(true)
        }
      }, k * 650),
    )
    emit(true)
  }

  function finishRound() {
    phase = "over"
    // The glass clears as the score comes up.
    const glass = api.screenBreak.canvas
    glass.style.transition = "opacity 500ms ease"
    glass.style.opacity = "0"
    setTimeout(() => {
      api.screenBreak.reset()
      glass.style.opacity = ""
      setTimeout(() => (glass.style.transition = ""), 50)
    }, 1400)
    // Whatever is left is cleared with a last volley, one note at a time.
    notes
      .filter((c) => c.state === "typing")
      .forEach((c, k) =>
        setTimeout(() => {
          if (c.state !== "typing") return
          if (reduced) return void (c.state = "leaving")
          c.state = "done"
          c.hitDir = { x: c.x - api.headPos().x, y: c.y - api.headPos().y }
          api.explode(c, 0.8)
        }, 120 + k * 140),
      )
    emit(true)
    onEvent?.({ type: "over", ...stats, latency: lat.slice() })
  }

  function clear() {
    for (const c of notes) if (c.state !== "dead" && c.state !== "dying") c.state = "leaving"
  }

  const api2 = {
    hear,
    broke,
    level: () => mic,
    intensity: () => (phase === "play" ? clamp(lerp(0.22, 0.62, level) + Math.min(0.12, stats.combo * 0.02), 0, 0.74) : phase === "over" ? 0.15 : 0.12),
    focus: () => {
      let f = null
      for (const c of notes) if (c.state === "typing" && (!f || c.u > f.u)) f = c
      return f
    },
    setMic(v) {
      // Fast attack, slower release, like a VU meter.
      mic = v > mic ? v : mic + (v - mic) * 0.35
    },
    begin() {
      if (phase === "ready") countdown()
    },
    restart() {
      clear()
      api.screenBreak.reset()
      stats = { score: 0, combo: 0, best: 0, words: 0, broken: 0, missed: 0 }
      lat.length = 0
      segs.clear()
      t = 0
      level = 0
      countdown()
    },
    quit() {
      phase = "quit"
      clear()
      api.screenBreak.reset()
      setTimeout(() => api.detach(), 400)
    },
    get phase() {
      return phase
    },
    latency: () => lat.slice(),
    update(dt, realDt) {
      mic *= Math.exp(-realDt * 2.2)
      if (phase === "play") {
        t += realDt
        level = clamp(t / ROUND, 0, 1)
        spawnIn -= realDt
        const alive = notes.filter((c) => c.state === "typing").length
        if ((spawnIn <= 0 && alive < 6) || alive === 0) {
          spawn()
          spawnIn = lerp(2.5, 1.25, Math.pow(level, 0.8))
        }
        if (t >= ROUND) finishRound()
        emit()
      }
      for (let n = notes.length - 1; n >= 0; n--) {
        const c = notes[n]
        if (c.state === "dead") {
          notes.splice(n, 1)
          continue
        }
        if (c.state === "leaving") {
          c.alpha = Math.max(0, c.alpha - realDt / 0.18)
          c.scale *= 1 - realDt * 0.25
          if (c.alpha <= 0) api.retire(c)
          continue
        }
        if (c.state === "typing" || c.state === "done") place(c, dt)
        if (c.state === "typing" && c.u >= 1 && phase === "play") hitGlass(c)
      }
      // Nearer notes draw on top.
      api.live.sort((a, b) => (a.u ?? 0) - (b.u ?? 0))
      api.head.target = api2.focus()
    },
  }
  return api2
}

function knock(s, dt) {
  const w = 2 * Math.PI * 7
  const n = Math.max(1, Math.ceil(dt * 240))
  const h = dt / n
  for (let i = 0; i < n; i++) {
    s.v += (-w * w * s.x - 2 * 0.4 * w * s.v) * h
    s.x += s.v * h
  }
}
// The recognizer hears swears in "ship" and "fix"; Bean doesn't shout them on the homepage.
const RUDE = /^(shit|fuck|damn|bitch|crap|cunt|dick|piss|bastard|asshole)/i
const clean = (w) => (RUDE.test(w) ? "#@!%" : w)
const clamp = (x, a, b) => Math.max(a, Math.min(b, x))
const lerp = (a, b, t) => a + (b - a) * t
const pick = (a) => a[Math.floor(Math.random() * a.length)]
