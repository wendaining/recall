import { useLayoutEffect, useRef } from 'react'
import { getMessages } from '../i18n'

export type SceneIndex = 0 | 1 | 2 | 3

const copy = getMessages()

type Props = {
  scene: SceneIndex
  progress?: number
  compact?: boolean
}

function clamp(value: number) {
  return Math.max(0, Math.min(1, value))
}

function typedText(text: string, progress: number, start: number, end: number) {
  if (progress <= start) return ''
  if (progress >= end) return text
  return text.slice(0, Math.ceil(text.length * clamp((progress - start) / (end - start))))
}

function OutputLines({ lines, progress, start, end }: {
  lines: readonly string[]
  progress: number
  start: number
  end: number
}) {
  const span = (end - start) / lines.length

  return <div className="playback-output">
    {lines.map((line, index) => {
      const lineStart = start + index * span
      if (progress < lineStart) return null
      const text = typedText(line, progress, lineStart, lineStart + span * .82)
      return <div className={`playback-line${index === lines.length - 1 ? ' output-success' : ''}`} key={index}>{text || '\u00a0'}</div>
    })}
  </div>
}

export function TerminalScene({ scene, progress = 1, compact = false }: Props) {
  const viewport = useRef<HTMLDivElement>(null)
  const position = clamp(progress)

  useLayoutEffect(() => {
    const element = viewport.current
    if (element) element.scrollTop = element.scrollHeight
  }, [scene, position])

  return (
    <div className={`terminal-window terminal-playback${compact ? ' terminal-window-compact' : ''}`} data-scene={scene} data-progress={position.toFixed(3)}>
      <div className="terminal-topbar">
        <span className="window-dots" aria-hidden="true"><i /><i /><i /></span>
        <span>{copy.demo.title}</span>
        <span className="terminal-topbar-path">{copy.demo.path}</span>
      </div>
      <div className="terminal-playback-viewport" ref={viewport}>
        {scene === 0 && <div className="terminal-playback-screen">
          <div className="playback-shell-label">{copy.demo.capture.shell}<span>{copy.demo.capture.time}</span></div>
          <div className="playback-command"><span>❯</span><strong>{typedText(copy.demo.capture.command, position, .01, .18)}</strong>{position < .19 && <i className="terminal-caret" />}</div>
          <OutputLines lines={copy.demo.capture.output} progress={position} start={.2} end={.96} />
        </div>}

        {scene === 1 && <div className="terminal-playback-screen">
          {position < .14 ? <div className="playback-command"><span>❯</span><strong>{typedText(copy.demo.search.launchCommand, position, .01, .12)}</strong><i className="terminal-caret" /></div> : <div className="playback-tui">
            <div className="tui-title">{copy.demo.search.title}</div>
            <div className="tui-search"><span>❯</span><strong>{typedText(copy.demo.search.query, position, .16, .48)}</strong><i className="terminal-caret" /></div>
            {position >= .49 && <div className="result-count">{copy.demo.search.count}</div>}
            <div className="result-list">
              {position >= .52 && <div className="result-row selected"><span className="result-arrow">›</span><div><strong>{copy.demo.search.firstCommand}</strong><small>{copy.demo.search.firstPreview}</small></div><span className="result-badge">0</span></div>}
              {position >= .65 && <div className="result-row"><span className="result-arrow">›</span><div><strong>{copy.demo.search.secondCommand}</strong><small>{copy.demo.search.secondPreview}</small></div><span className="result-badge">0</span></div>}
              {position >= .78 && <div className="result-row"><span className="result-arrow">›</span><div><strong>{copy.demo.search.thirdCommand}</strong><small>{copy.demo.search.thirdPreview}</small></div><span className="result-badge">0</span></div>}
            </div>
            {position >= .9 && <div className="tui-hint">{copy.demo.search.hint}</div>}
          </div>}
        </div>}

        {scene === 2 && <div className="terminal-playback-screen playback-tui">
          <div className="tui-title">{copy.demo.inspect.title}</div>
          <div className="inspect-command"><span>❯</span>{typedText(copy.demo.inspect.command, position, .02, .16)}</div>
          {position >= .18 && <div className="inspect-grid">
            <div className="inspect-output"><div className="panel-label">{copy.demo.inspect.labelOutput}</div><OutputLines lines={copy.demo.inspect.output} progress={position} start={.26} end={.94} /></div>
            <div className="inspect-metadata">
              <div><small>{copy.demo.inspect.labelCwd}</small><strong>{copy.demo.path}</strong></div>
              {position >= .36 && <div><small>{copy.demo.inspect.labelTime}</small><strong>{copy.demo.inspect.time}</strong></div>}
              {position >= .53 && <div><small>{copy.demo.inspect.labelDuration}</small><strong>{copy.demo.inspect.duration}</strong></div>}
              {position >= .71 && <div><small>{copy.demo.inspect.labelStatus}</small><strong className="output-success">● {copy.demo.inspect.status}</strong></div>}
            </div>
          </div>}
        </div>}

        {scene === 3 && <div className="terminal-playback-screen">
          {position < .24 ? <div className="playback-tui">
            <div className="tui-title">{copy.demo.continue.title}</div>
            <div className="continue-selected"><div className="panel-label">{copy.demo.continue.selected}</div><strong>❯ {copy.demo.continue.command}</strong></div>
            <div className="tui-hint">{copy.demo.continue.edit}</div>
          </div> : <>
            <div className="playback-shell-label">{copy.demo.capture.shell}<span>{copy.demo.capture.time}</span></div>
            <div className="playback-command"><span>❯</span><strong>{typedText(copy.demo.continue.command, position, .25, .47)}</strong>{position < .48 && <i className="terminal-caret" />}</div>
            {position >= .48 && <OutputLines lines={copy.demo.continue.output} progress={position} start={.49} end={.96} />}
          </>}
        </div>}
      </div>
    </div>
  )
}
