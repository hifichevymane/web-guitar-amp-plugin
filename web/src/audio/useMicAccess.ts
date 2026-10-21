import { useCallback, useEffect, useState } from 'react'

export type MicStatus = 'pending' | 'granted' | 'denied' | 'unavailable'

const NO_INPUT = 'No audio input found. Connect your interface and press Retry.'
const INSECURE = 'Open this page on localhost or HTTPS.'

export function useMicAccess(): { status: MicStatus; message?: string; devices: MediaDeviceInfo[]; retry(): void } {
  const supported = window.isSecureContext && !!navigator.mediaDevices
  const [status, setStatus] = useState<MicStatus>(supported ? 'pending' : 'unavailable')
  const [message, setMessage] = useState(supported ? undefined : window.isSecureContext ? NO_INPUT : INSECURE)
  const [devices, setDevices] = useState<MediaDeviceInfo[]>([])
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    let cancelled = false
    if (!supported) return
    const md = navigator.mediaDevices
    const refresh = async () => {
      const all = await md.enumerateDevices()
      if (!cancelled) setDevices(all.filter((d) => d.kind === 'audioinput'))
    }
    md.getUserMedia({ audio: true })
      .then(async (stream) => {
        stream.getTracks().forEach((t) => t.stop()) // only asking permission; Start opens the real stream
        await refresh()
        if (!cancelled) setStatus('granted')
      })
      .catch((e: unknown) => {
        if (cancelled) return
        const name = e instanceof DOMException ? e.name : ''
        if (name === 'NotAllowedError' || name === 'SecurityError') setStatus('denied')
        else {
          setStatus('unavailable')
          setMessage(NO_INPUT)
        }
      })
    const onChange = () => void refresh().catch(() => {}) // keep the last list if enumeration fails
    md.addEventListener('devicechange', onChange)
    return () => {
      cancelled = true
      md.removeEventListener('devicechange', onChange)
    }
  }, [attempt, supported])

  const retry = useCallback(() => {
    if (!supported) return // nothing changes until the page is reloaded in a secure context
    setStatus('pending')
    setMessage(undefined)
    setAttempt((n) => n + 1)
  }, [supported])
  return { status, message, devices, retry }
}
