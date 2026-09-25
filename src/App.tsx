import { useEffect, useRef, useState } from 'react'
import { CopyCommand } from './components/CopyCommand'
import { TerminalScene, type SceneIndex } from './components/TerminalScene'
import { getMessages } from './i18n'

const copy = getMessages()
const scenes: SceneIndex[] = [0, 1, 2, 3]
const github = 'https://github.com/wendaining/recall'

export default function App() {
  const [activeScene, setActiveScene] = useState<SceneIndex>(0)
  const storySections = useRef<(HTMLElement | null)[]>([])

  useEffect(() => {
    let frame = 0

    function updateActiveScene() {
      frame = 0
      const activationLine = window.innerHeight * 0.48
      let next: SceneIndex = 0

      for (const scene of scenes) {
        const section = storySections.current[scene]
        if (section && section.getBoundingClientRect().top <= activationLine) next = scene
      }

      setActiveScene(previous => previous === next ? previous : next)
    }

    function scheduleUpdate() {
      if (!frame) frame = window.requestAnimationFrame(updateActiveScene)
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
        <a className="header-github" href={github} target="_blank" rel="noreferrer">{copy.nav.github} <span aria-hidden="true">↗</span></a>
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
            <div className="hero-terminal-wrap"><TerminalScene scene={0} compact /></div>
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
              <TerminalScene scene={activeScene} />
              <div className="stage-foot"><span>{copy.story.demoNote}</span><div className="stage-progress">{scenes.map(scene => <i key={scene} className={scene === activeScene ? 'active' : ''} />)}</div></div>
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
                  <div className="mobile-demo" aria-hidden="true"><TerminalScene scene={scene} compact /><small>{copy.story.demoNote}</small></div>
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
