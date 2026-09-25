import { getMessages } from './i18n'

const copy = getMessages()

export default function App() {
  return (
    <div className="site-shell">
      <header className="site-header">
        <a className="brand" href="#top" aria-label="Recall home">
          <img src="./recall-logo.png" alt="" />
          <span>recall<span className="brand-cursor">_</span></span>
        </a>
        <nav aria-label="Main navigation">
          <a href="#story">{copy.nav.product}</a>
          <a href="#features">{copy.nav.features}</a>
          <a href="#install">{copy.nav.install}</a>
        </nav>
        <a className="header-github" href="https://github.com/wendaining/recall">{copy.nav.github} ↗</a>
      </header>
      <main id="top">
        <section className="hero" aria-labelledby="hero-title">
          <p className="eyebrow">{copy.hero.eyebrow}</p>
          <h1 id="hero-title">{copy.hero.titleStart}<br /><em>{copy.hero.titleEnd}</em></h1>
          <p className="hero-description">{copy.hero.description}</p>
          <div className="hero-actions">
            <a className="button button-primary" href="#install">{copy.hero.install}<span aria-hidden="true">↗</span></a>
            <a className="button button-quiet" href="#story">{copy.hero.explore}<span aria-hidden="true">↓</span></a>
          </div>
          <p className="hero-note">{copy.hero.note}</p>
        </section>
      </main>
    </div>
  )
}
