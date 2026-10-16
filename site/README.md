# HEX marketing site (.dev)

`bun run dev` starts Vite. `bun run build` checks TypeScript and builds the site;
`wrangler deploy` publishes it to `hex.kitlangton.dev`. Bump `MAC_VERSION` in
`src/App.tsx` after each macOS release.

## The hero

`src/hero/` is a WebGPU swarm read by an instrument display, adapted from the
MAGI hero in the knobs examples (`hero-magi*.ts`, `controls-post.wgsl`).

- `take.ts` holds the scripted sample dictations: word timings, a synthetic speech
  envelope, and a director that plays a take the way Hex handles one. A pause of
  600 ms after 3 s of speech (or 300 ms after 8 s) cuts a piece, release pastes
  once, and a capture under 300 ms is discarded. There is no microphone.
- `sim.wgsl` moves 16,384 agents (8,192 under 900 px) through splat, field and
  advance passes. Each agent keeps a lane in a band round the ring, and the band
  widens wherever the take's envelope was loud.
- `draw.wgsl` draws the agents into a phosphor trail and the instrument over them:
  the envelope gauge, cut marks, the write head, brackets on pieces, the level
  scope and the trace to the text field. `post.wgsl` is the knobs catalog's bloom
  and tone pass.
- `engine.ts` owns the GPU resources; `Hero.tsx` owns layout (measured from the
  DOM), input, the frame loop and the React readouts.
- `still.ts` draws a 2D still when WebGPU is missing (or with `?nogpu`).
- `tuning.ts` holds the swarm's values. Press `` ` `` or open `?tune` to load a
  knobs panel for them; nothing from knobs loads otherwise.

Rendering stops offscreen and in hidden tabs, and the device pixel ratio is capped
at 2 (1.5 on narrow screens). With reduced motion, nothing plays on its own and the
loop stops a few seconds after each take. `window.__hexHero.timings()` returns
average GPU milliseconds per stage where timestamp queries exist.

## Screenshots and the social image

Headless Chromium on Apple silicon (`channel: "chromium"`) has Metal WebGPU, so
these scripts capture the live hero. They are review aids, not tests.

```sh
bunx playwright install chromium
HEX_SITE_URL=http://127.0.0.1:5173 bun scripts/shots.ts   # desktop/mobile, live and still
HEX_SITE_URL=http://127.0.0.1:5173 bun scripts/frames.ts  # one take, moment by moment
HEX_SITE_URL=http://127.0.0.1:5173 bun scripts/check.ts   # reduced motion and the panel
HEX_SITE_URL=http://127.0.0.1:5173 bun run generate:og    # public/og.png
```

`HEX_SHOTS` chooses the output folder. The separate `.com` site lives in
`/Users/kit/code/projects/hex-marketing-site` and deploys through Vercel.
