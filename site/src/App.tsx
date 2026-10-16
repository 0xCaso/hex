import { useState, type ReactNode } from "react"
import Hero from "./hero/Hero"

const MAC_VERSION = "2.2.0"
const DOWNLOAD_URL =
  `https://downloads.hex.kitlangton.dev/releases/HEX-${MAC_VERSION}-arm64.dmg`
const GITHUB_DOWNLOAD_URL =
  `https://github.com/anomalyco/hex/releases/download/app-v${MAC_VERSION}/HEX-${MAC_VERSION}-arm64.dmg`
const LINUX_INSTALL_URL = "https://github.com/anomalyco/hex/blob/main/docs/linux.md"
const REPO_URL = "https://github.com/anomalyco/hex"
const SDK_URL = "https://github.com/anomalyco/hex/blob/main/sdk/typescript/README.md"
const BREW = "brew install --cask anomalyco/tap/hex"

function HexMark() {
  return (
    <svg aria-hidden="true" viewBox="0 0 64 64" className="wordmark__icon">
      <path d="M32 4 57 18.5v27L32 60 7 45.5v-27Z" />
      <path d="M32 15 48 24.25v15.5L32 49l-16-9.25v-15.5Z" />
      <path className="wordmark__core" d="m32 24 7.5 4.25v8.5L32 41l-7.5-4.25v-8.5Z" />
    </svg>
  )
}

function AppleMark() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="button__icon">
      <path d="M16.68 12.9c.02 2.3 2.02 3.06 2.04 3.07-.02.06-.32 1.1-1.05 2.17-.63.92-1.29 1.84-2.32 1.86-1.01.02-1.34-.6-2.5-.6-1.15 0-1.51.58-2.47.62-.99.04-1.74-.99-2.38-1.91-1.3-1.88-2.3-5.32-.96-7.65a3.7 3.7 0 0 1 3.15-1.91c.98-.02 1.91.66 2.5.66.6 0 1.72-.82 2.9-.7.49.02 1.88.2 2.77 1.5-.07.04-1.65.96-1.68 2.89ZM14.81 7.27c.53-.64.89-1.53.79-2.41-.76.03-1.68.51-2.23 1.15-.49.57-.92 1.47-.8 2.34.85.07 1.71-.43 2.24-1.08Z" />
    </svg>
  )
}

function Brew({ compact = false }: { compact?: boolean }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(BREW)
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch {
      setCopied(false)
    }
  }
  return (
    <div className={compact ? "brew brew--compact" : "brew"}>
      <code>
        <span className="brew__prompt" aria-hidden="true">$</span>
        {BREW}
      </code>
      <button type="button" className="brew__copy" onClick={copy} aria-label="Copy the Homebrew command">
        <span className="brew__label" data-shown={!copied}>Copy</span>
        <span className="brew__label" data-shown={copied} aria-live="polite">{copied ? "Copied" : ""}</span>
      </button>
    </div>
  )
}

function DownloadButton({ isLinux }: { isLinux: boolean }) {
  return isLinux ? (
    <a className="button button--primary" href={LINUX_INSTALL_URL}>
      <span>Install on Linux</span>
      <span aria-hidden="true">&#8599;</span>
    </a>
  ) : (
    <a className="button button--primary" href={DOWNLOAD_URL}>
      <AppleMark />
      <span>Download for Mac</span>
    </a>
  )
}

const STEPS = [
  {
    key: "Hold",
    title: "Hold Option",
    body: "Hex starts listening while you hold the shortcut. Double-tap it to keep recording hands-free, or pick another key in Settings.",
  },
  {
    key: "Speak",
    title: "Say what you mean",
    body: "Talk normally. When you pause, Hex transcribes what you've said so far in the background.",
  },
  {
    key: "Release",
    title: "Let go",
    body: "Hex transcribes the rest and pastes the text into the app you're using. Escape cancels, and Option-Shift-V pastes the last dictation again.",
  },
]

const FEATURES: { name: string; tag?: string; body: ReactNode }[] = [
  {
    name: "Dictation",
    body: "Hex pastes into whichever app has focus. Transcribe during pauses is on by default, so a long dictation doesn't wait for one big transcription at the end.",
  },
  {
    name: "Modes",
    body: "Corrections fix names and spellings before Hex pastes. A mode can also rewrite the text with OpenCode or run transformations such as Lowercase, and it can switch by app or by website in Brave.",
  },
  {
    name: "Voice Action",
    tag: "Opt-in",
    body: "Hold Option-Command and describe what you want. Hex sends the instruction and any selected text to OpenCode and pastes the result where you are.",
  },
  {
    name: "History",
    body: "Search, copy or delete recent dictations. Hex keeps the text for seven days by default and never stores audio. You can turn it off or clear it.",
  },
  {
    name: "Voice commands",
    tag: "Opt-in",
    body: "Speak commands without touching the shortcut. A separate local model listens for them, phrases can depend on the app or website, and you can write your own in TypeScript.",
  },
  {
    name: "Meetings",
    tag: "Beta",
    body: (
      <>
        Record a call and get a local transcript with You and Them turns. Apple Intelligence writes a title and a
        short summary on your Mac, and <code>hex mcp</code> lets coding agents search your meetings. Recording never
        starts on its own.
      </>
    ),
  },
  {
    name: "Linux",
    tag: "Beta",
    body: "Hotkey dictation on x86_64 Linux with i3 on X11 or a wlroots Wayland compositor. For now you build it from source or install it with Nix.",
  },
  {
    name: "TypeScript SDK",
    body: (
      <>
        <code>@kitlangton/hex</code> adds local transcription to your own app through Promise and Effect APIs. You
        supply the audio and, for now, a compatible Hex executable.
      </>
    ),
  },
]

export default function App() {
  const isLinux = /\bLinux\b/i.test(navigator.userAgent) && !/\bAndroid\b/i.test(navigator.userAgent)

  return (
    <div className="page">
      <header className="header">
        <a className="wordmark" href="/" aria-label="HEX home">
          <HexMark />
          <span>HEX</span>
        </a>
        <nav className="nav" aria-label="Site">
          <a href="#features">Features</a>
          <a href={REPO_URL}>GitHub</a>
          <a className="nav__download" href="#download">Download</a>
        </nav>
      </header>

      <main>
        <Hero>
          <h1>Speak. It appears.</h1>
          <p className="lede">
            Hold {isLinux ? "Alt-Space" : "Option"}, say what you mean, and let go. Hex transcribes on your{" "}
            {isLinux ? "computer" : "Mac"} and pastes the text where you're typing.
          </p>
          <div className="hero__actions">
            <DownloadButton isLinux={isLinux} />
            <p className="meta">
              {isLinux ? "x86_64 Linux beta" : `Version ${MAC_VERSION} · Apple silicon · macOS 15+`}
            </p>
          </div>
          {!isLinux && <Brew compact />}
        </Hero>

        <section className="section" aria-labelledby="how">
          <div className="section__inner">
            <p className="eyebrow" id="how">How it works</p>
            <ol className="steps">
              {STEPS.map((s, i) => (
                <li className="step" key={s.key}>
                  <span className="step__index">{String(i + 1).padStart(2, "0")}</span>
                  <h2>{s.title}</h2>
                  <p>{s.body}</p>
                </li>
              ))}
            </ol>
          </div>
        </section>

        <section className="section" aria-labelledby="local">
          <div className="section__inner facts">
            <div className="fact">
              <p className="eyebrow" id="local">On your Mac</p>
              <h2>Your voice stays on this computer.</h2>
              <p>
                Speech recognition runs locally with a model Hex downloads during setup. Hex doesn't upload your
                audio, and it doesn't save it by default. The optional OpenCode features send text to the model
                provider you set up, and they're off until you turn them on.
              </p>
              <p className="fact__figure">
                <span>0 bytes</span>
                <span className="fact__caption">of audio uploaded</span>
              </p>
            </div>
            <div className="fact">
              <p className="eyebrow">Fast</p>
              <h2>Text arrives as you let go.</h2>
              <p>
                Models run on the Mac's GPU through Metal. Because Hex transcribes during pauses, only the last few
                seconds are left when you release. In a benchmark on a Mac Studio, dictations of 77 to 99 seconds were
                ready 0.05 to 0.2 seconds after release, compared with 0.7 to 3.3 seconds for the whole clip at once.
              </p>
              <p className="fact__figure">
                <span>0.05–0.2 s</span>
                <span className="fact__caption">release to text, 77–99 s dictations</span>
              </p>
            </div>
            <div className="fact fact--wide">
              <p className="eyebrow">Models</p>
              <p>
                Pick a model for your language. For English, Hex recommends Parakeet Unified English, and Cohere
                Transcribe is its most accurate choice. Whisper large-v3-turbo covers about 100 languages, and Parakeet
                v3, Qwen3-ASR and SenseVoice cover others. You can switch between downloaded models from the menu bar.
              </p>
            </div>
          </div>
        </section>

        <section className="section" aria-labelledby="features">
          <div className="section__inner">
            <p className="eyebrow" id="features">In Hex {MAC_VERSION}</p>
            <ul className="features">
              {FEATURES.map((f) => (
                <li className="feature" key={f.name}>
                  <h3>
                    {f.name}
                    {f.tag && <span className="tag">{f.tag}</span>}
                  </h3>
                  <p>{f.body}</p>
                </li>
              ))}
            </ul>
            <p className="features__more">
              Voice Action, voice commands and Meetings start off, and each one is a separate switch.{" "}
              <a href={SDK_URL}>Read the SDK guide</a> or <a href={LINUX_INSTALL_URL}>the Linux guide</a>.
            </p>
          </div>
        </section>

        <section className="section section--download" id="download" aria-labelledby="download-title">
          <div className="section__inner download">
            <div>
              <h2 id="download-title">Download Hex</h2>
              <p>
                Hex {MAC_VERSION} runs on Apple silicon Macs with macOS 15 or later. Setup asks for Microphone, Input
                Monitoring and Accessibility access, then downloads a speech model.
              </p>
            </div>
            <div className="download__actions">
              <a className="button button--primary" href={DOWNLOAD_URL}>
                <AppleMark />
                <span>Download for Mac</span>
              </a>
              <Brew />
              <p className="download__alt">
                Download blocked? <a href={GITHUB_DOWNLOAD_URL}>Get the same signed DMG from GitHub</a>.
              </p>
              <p className="download__alt">
                On Linux, follow the <a href={LINUX_INSTALL_URL}>installation guide</a>.
              </p>
            </div>
          </div>
        </section>
      </main>

      <footer className="footer">
        <div className="footer__inner">
          <span className="wordmark wordmark--small">
            <HexMark />
            <span>HEX</span>
          </span>
          <nav aria-label="Footer">
            <a href={REPO_URL}>GitHub</a>
            <a href={`${REPO_URL}/issues/new?template=bug_report.yml`}>Report a bug</a>
            <a href="https://www.npmjs.com/package/@kitlangton/hex">npm</a>
            <a href={`${REPO_URL}/blob/main/LICENSE`}>MIT License</a>
          </nav>
        </div>
      </footer>
    </div>
  )
}
