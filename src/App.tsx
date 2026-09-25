import { useEffect, useRef, useState } from 'react'
import { CopyCommand } from './components/CopyCommand'
import { TerminalScene, type SceneIndex } from './components/TerminalScene'
import { getMessages } from './i18n'

const copy = getMessages()
const scenes: SceneIndex[] = [0, 1, 2, 3]
const github = 'https://github.com/wendaining/recall'

export default function App() {
  const [playback, setPlayback] = useState<{ scene: SceneIndex; progress: number }>({ scene: 0, progress: 0 })
  const [reducedMotion, setReducedMotion] = useState(false)
  const storySections = useRef<(HTMLElement | null)[]>([])
  const activeScene = playback.scene

  useEffect(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)')
    const updatePreference = () => setReducedMotion(preference.matches)
    updatePreference()
    preference.addEventListener('change', updatePreference)
    return () => preference.removeEventListener('change', updatePreference)
  }, [])

  useEffect(() => {
    let frame = 0

    function updatePlayback() {
      frame = 0
      const isNarrow = window.matchMedia('(max-width: 980px)').matches
      const activationLine = isNarrow ? Math.min(window.innerHeight * .72, 520) : window.innerHeight * .48
      let next: SceneIndex = 0

      for (const scene of scenes) {
        const section = storySections.current[scene]
        if (section && section.getBoundingClientRect().top <= activationLine) next = scene
      }

      const section = storySections.current[next]
      const rect = section?.getBoundingClientRect()
      const rawProgress = rect ? (activationLine - rect.top) / rect.height : 0
      const progress = Math.round(Math.max(0, Math.min(1, rawProgress)) * 240) / 240
      setPlayback(previous => previous.scene === next && previous.progress === progress ? previous : { scene: next, progress })
    }

    function scheduleUpdate() {
      if (!frame) frame = window.requestAnimationFrame(updatePlayback)
    }

    window.addEventListener('scroll', scheduleUpdate, { passive: true })
    window.addEventListener('resize', scheduleUpdate)
    scheduleUpdate()

    return () => {
      window.removeEventListener('scroll', scheduleUpdate)
      window.removeEventListener('resize', scheduleUpdate)
      if (frame) window.cancelAnimationFrame(frame)
    }
  }, [])

  return (
    <div className="site-shell">
      <a className="skip-link" href="#main">{copy.nav.skip}</a>
      <header className="site-header">
        <a className="brand" href="#top" aria-label={copy.nav.home}>
          <img src="./recall-logo.png" alt="" />
          <span>recall<span className="brand-cursor">_</span></span>
        </a>
        <nav aria-label={copy.nav.main}>
          <a href="#story">{copy.nav.product}</a>
          <a href="#features">{copy.nav.features}</a>
          <a href="#install">{copy.nav.install}</a>
        </nav>
        <a className="header-github" href={github} target="_blank" rel="noreferrer" aria-label={copy.nav.github}>
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" focusable="false">
            <path d="M10.226 17.284c-2.965-.36-5.054-2.493-5.054-5.256 0-1.123.404-2.336 1.078-3.144-.292-.741-.247-2.314.09-2.965.898-.112 2.111.36 2.83 1.01.853-.269 1.752-.404 2.853-.404 1.1 0 1.999.135 2.807.382.696-.629 1.932-1.1 2.83-.988.315.606.36 2.179.067 2.942.72.854 1.101 2 1.101 3.167 0 2.763-2.089 4.852-5.098 5.234.763.494 1.28 1.572 1.28 2.807v2.336c0 .674.561 1.056 1.235.786 4.066-1.55 7.255-5.615 7.255-10.646C23.5 6.188 18.334 1 11.978 1 5.62 1 .5 6.188.5 12.545c0 4.986 3.167 9.12 7.435 10.669.606.225 1.19-.18 1.19-.786V20.63a2.9 2.9 0 0 1-1.078.224c-1.483 0-2.359-.808-2.987-2.313-.247-.607-.517-.966-1.034-1.033-.27-.023-.359-.135-.359-.27 0-.27.45-.471.898-.471.652 0 1.213.404 1.797 1.235.45.651.921.943 1.483.943.561 0 .92-.202 1.437-.719.382-.381.674-.718.944-.943" />
          </svg>
        </a>
      </header>

      <main id="main">
        <section className="hero" id="top" aria-labelledby="hero-title">
          <div className="hero-glow" aria-hidden="true" />
          <div className="hero-copy">
            <p className="eyebrow"><span className="eyebrow-mark" />{copy.hero.eyebrow}</p>
            <h1 id="hero-title">{copy.hero.titleStart}<br /><em>{copy.hero.titleEnd}</em></h1>
            <p className="hero-description">{copy.hero.description}</p>
            <div className="hero-actions">
              <a className="button button-primary" href="#install">{copy.hero.install}<span aria-hidden="true">↗</span></a>
              <a className="button button-quiet" href="#story">{copy.hero.explore}<span aria-hidden="true">↓</span></a>
            </div>
            <p className="hero-note"><span aria-hidden="true">✳</span>{copy.hero.note}</p>
          </div>
          <div className="hero-art" aria-hidden="true">
            <div className="hero-art-line hero-art-line-one" />
            <div className="hero-art-line hero-art-line-two" />
            <div className="hero-terminal-wrap"><TerminalScene scene={0} progress={.76} compact /></div>
            <div className="hero-art-tag">{copy.hero.artTag} <span>●</span></div>
          </div>
          <div className="hero-scroll-label" aria-hidden="true">{copy.hero.scroll} <span>↓</span></div>
        </section>

        <section className="story-section" id="story" aria-labelledby="story-title">
          <div className="section-intro">
            <p className="eyebrow"><span className="eyebrow-mark" />{copy.story.eyebrow}</p>
            <h2 id="story-title">{copy.story.heading}</h2>
            <p>{copy.story.lead}</p>
            <span className="sr-only">{copy.story.demoNote}</span>
          </div>

          <div className="story-grid">
            <div className="story-stage" aria-hidden="true">
              <div className="stage-heading"><span>{copy.story.currentScene} <strong>0{activeScene + 1} / 04</strong></span><span className="stage-dashes">— — — —</span></div>
              <TerminalScene scene={activeScene} progress={reducedMotion ? 1 : playback.progress} />
              <div className="stage-foot"><span>{copy.story.scrollCue}</span><span className="stage-note">{copy.story.demoNote}</span></div>
              <div className="stage-progress-track"><span style={{ width: `${((activeScene + playback.progress) / scenes.length) * 100}%` }} /></div>
              <div className="stage-mobile-copy">
                <span>{copy.story.steps[activeScene].number} / {copy.story.steps[activeScene].eyebrow}</span>
                <h3>{copy.story.steps[activeScene].heading}</h3>
                <p>{copy.story.steps[activeScene].description}</p>
              </div>
            </div>

            <div className="story-steps">
              {scenes.map(scene => {
                const step = copy.story.steps[scene]
                return <article
                  className={`story-step${scene === activeScene ? ' is-active' : ''}`}
                  key={step.number}
                  ref={element => { storySections.current[scene] = element }}
                >
                  <div className="step-rule"><span>{step.number}</span><span>{step.eyebrow}</span></div>
                  <h3>{step.heading}</h3>
                  <p>{step.description}</p>
                </article>
              })}
            </div>
          </div>
        </section>

        <section className="features-section" id="features" aria-labelledby="features-title">
          <div className="section-intro">
            <p className="eyebrow"><span className="eyebrow-mark" />{copy.features.eyebrow}</p>
            <h2 id="features-title">{copy.features.heading}</h2>
            <p>{copy.features.lead}</p>
          </div>
          <div className="feature-cards">
            {copy.features.items.map(item => <article className="feature-card" key={item.number}>
              <span className="feature-card-number">{item.number}</span>
              <span className="feature-card-symbol" aria-hidden="true">{item.number.startsWith('01') ? '⌘' : item.number.startsWith('02') ? '↶' : '◇'}</span>
              <h3>{item.title}</h3>
              <p>{item.description}</p>
            </article>)}
          </div>
        </section>

        <section className="install-section" id="install" aria-labelledby="install-title">
          <div className="install-copy">
            <p className="eyebrow"><span className="eyebrow-mark" />{copy.install.eyebrow}</p>
            <h2 id="install-title">{copy.install.heading}</h2>
            <p>{copy.install.description}</p>
          </div>
          <div className="install-panel">
            <CopyCommand label={copy.install.unixLabel} command={copy.install.unixCommand} />
            <CopyCommand label={copy.install.windowsLabel} command={copy.install.windowsCommand} />
            <div className="install-links">
              <a href={`${github}#install`} target="_blank" rel="noreferrer">{copy.install.docs} <span aria-hidden="true">↗</span></a>
              <a href={`${github}/releases`} target="_blank" rel="noreferrer">{copy.install.releases} <span aria-hidden="true">↗</span></a>
            </div>
          </div>
        </section>
      </main>

      <footer className="site-footer">
        <div><a className="brand footer-brand" href="#top"><img src="./recall-logo.png" alt="" /><span>recall<span className="brand-cursor">_</span></span></a><p>{copy.footer.tagline}</p></div>
        <div className="footer-links"><a href={github} target="_blank" rel="noreferrer">{copy.footer.source} ↗</a><a href={`${github}/blob/master/LICENSE`} target="_blank" rel="noreferrer">{copy.footer.license} ↗</a></div>
        <small>{copy.footer.note}</small>
      </footer>
    </div>
  )
}
