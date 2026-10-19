# `scrawl-core`

Pure, GPUI-free, platform-toolkit-free core library for Scrawl captures inside
Hex. Imported with git history from `/Users/kit/code/open-source/scrawl`.

## Merge-to-`main` Blocker: License File

The upstream Scrawl repository (`/Users/kit/code/open-source/scrawl`) declares
`license = "MIT"` in its `Cargo.toml`, but has no `LICENSE` file in its tree,
while Hex is licensed under MIT in `anomalyco/hex`.

**Do not merge `experiment/scrawl` into `main` until Kit adds the explicit MIT
`LICENSE` file to Scrawl (or confirms the copyright/license header for
`crates/scrawl-core`).**

## Scope

- `model`: `.scrawl` bundle schema (`ScrawlBundle`, `Keyframe`, `Stroke`,
  `Mark`, `InteractionEvent`, `TranscriptData`, `WordTiming`, `RgbaFrame`).
- `keyframes`: keyframe selection, mark grouping, scene-transition diffing,
  spoken-cue snapping, and narration alignment.
- `sketch`: stroke shape classification (circle, box, arrow, underline, line,
  cross, scribble, laser, pin, flag), freehand geometry, and `summary_markdown`.
- `autocrop`: activity-weighted auto-crop solver, content-edge snapping, and the
  `autocrop-corpus.json` regression suite.
- `render`: deterministic CPU RGBA frame annotation, keyframe badge stamping,
  justified contact-sheet packing, zoom-sheet rendering, and PNG encode/decode.
- `hold`: pure `Switcher` and `Reveal` state machines for the `⌥⇧S` tap/hold
  gesture.
- `windows`: `ActivityLog`, `WindowSnapshot`, `CursorSample`, pure window
  filtering, and `WindowLogger` over the `WindowProbe` trait.
- `zoom` & `export`: moment/phrase lookup, zoom window sampling, and pure camera
  / crop / ink vector export planning.
- `traits` & `pipeline`: platform traits (`Clock`, `FrameSource`, `InputSource`,
  `WordTranscriber`, `WindowProbe`) and composable [`BundleBuilder`].
