import { useState } from 'react'
import { getMessages } from '../i18n'

const copy = getMessages()

type Props = {
  label: string
  command: string
}

export function CopyCommand({ label, command }: Props) {
  const [status, setStatus] = useState<'idle' | 'copied' | 'error'>('idle')

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(command)
      setStatus('copied')
    } catch {
      setStatus('error')
    }
  }

  return (
    <div className="install-card">
      <div className="install-card-top"><span className="install-os-dot" />{label}</div>
      <div className="install-command"><span aria-hidden="true">$</span><code>{command}</code></div>
      <div className="install-card-bottom">
        <button className="copy-button" type="button" onClick={handleCopy} aria-label={`${copy.install.copy}: ${label}`}>
          <span aria-hidden="true">{status === 'copied' ? '✓' : '▣'}</span>{status === 'copied' ? copy.install.copied : copy.install.copy}
        </button>
        <span className="copy-status" role="status">{status === 'error' ? copy.install.copyFailed : ''}</span>
      </div>
    </div>
  )
}
