import { getMessages } from '../i18n'

export type SceneIndex = 0 | 1 | 2 | 3

const copy = getMessages()

type Props = {
  scene: SceneIndex
  compact?: boolean
}

export function TerminalScene({ scene, compact = false }: Props) {
  return (
    <div className={`terminal-window${compact ? ' terminal-window-compact' : ''}`}>
      <div className="terminal-topbar">
        <span className="window-dots" aria-hidden="true"><i /><i /><i /></span>
        <span>{copy.demo.title}</span>
        <span className="terminal-topbar-path">{copy.demo.path}</span>
      </div>
      <div className="terminal-scene" key={scene}>
        {scene === 0 && <div className="capture-scene">
          <div className="scene-meta"><span className="live-dot" />{copy.demo.capture.shell}<span>{copy.demo.capture.time}</span></div>
          <div className="shell-command"><span className="shell-prompt">❯</span><span>{copy.demo.capture.command}</span><span className="terminal-caret" /></div>
          <div className="shell-output">{copy.demo.capture.output.map((line, index) => <div className={index === 3 ? 'output-success' : ''} key={line}>{line}</div>)}</div>
          <div className="capture-save"><span>↳ {copy.demo.capture.saved}</span><span>{copy.demo.capture.duration} · {copy.demo.capture.exit}</span></div>
          <div className="capture-caption">{copy.demo.capture.caption}</div>
        </div>}

        {scene === 1 && <div className="search-scene">
          <div className="tui-title">{copy.demo.search.title}</div>
          <div className="tui-search"><span>❯</span><strong>{copy.demo.search.query}</strong><span className="terminal-caret" /></div>
          <div className="result-count">{copy.demo.search.count}</div>
          <div className="result-list">
            <div className="result-row selected"><span className="result-arrow">›</span><div><strong>{copy.demo.search.firstCommand}</strong><small>{copy.demo.search.firstPreview}</small></div><span className="result-badge">0</span></div>
            <div className="result-row"><span className="result-arrow">›</span><div><strong>{copy.demo.search.secondCommand}</strong><small>{copy.demo.search.secondPreview}</small></div><span className="result-badge">0</span></div>
            <div className="result-row"><span className="result-arrow">›</span><div><strong>{copy.demo.search.thirdCommand}</strong><small>{copy.demo.search.thirdPreview}</small></div><span className="result-badge">0</span></div>
          </div>
          <div className="tui-hint">{copy.demo.search.hint}</div>
        </div>}

        {scene === 2 && <div className="inspect-scene">
          <div className="tui-title">{copy.demo.inspect.title}</div>
          <div className="inspect-command"><span>❯</span>{copy.demo.inspect.command}</div>
          <div className="inspect-grid">
            <div className="inspect-output"><div className="panel-label">{copy.demo.inspect.labelOutput}</div>{copy.demo.inspect.output.map((line, index) => <div className={index === 4 ? 'output-success' : ''} key={line}>{line}</div>)}</div>
            <div className="inspect-metadata">
              <div><small>{copy.demo.inspect.labelCwd}</small><strong>{copy.demo.path}</strong></div>
              <div><small>{copy.demo.inspect.labelTime}</small><strong>{copy.demo.inspect.time}</strong></div>
              <div><small>{copy.demo.inspect.labelDuration}</small><strong>{copy.demo.inspect.duration}</strong></div>
              <div><small>{copy.demo.inspect.labelStatus}</small><strong className="output-success">● {copy.demo.inspect.status}</strong></div>
            </div>
          </div>
        </div>}

        {scene === 3 && <div className="continue-scene">
          <div className="tui-title">{copy.demo.continue.title}</div>
          <div className="continue-selected"><div className="panel-label">{copy.demo.continue.selected}</div><strong>❯ {copy.demo.continue.command}</strong></div>
          <div className="continue-arrow" aria-hidden="true">↓</div>
          <div className="continue-prompt"><span>❯</span>{copy.demo.continue.command}<span className="terminal-caret" /></div>
          <p>{copy.demo.continue.prompt}</p>
          <div className="keyboard-grid"><span>{copy.demo.continue.edit}</span><span>{copy.demo.continue.run}</span><span>{copy.demo.continue.copyCommand}</span><span>{copy.demo.continue.copyOutput}</span></div>
          <div className="tui-hint">{copy.demo.continue.hint}</div>
        </div>}
      </div>
    </div>
  )
}
