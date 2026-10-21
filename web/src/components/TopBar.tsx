import type { LatencyReport } from '../audio/AmpAudio'
import type { MicStatus } from '../audio/useMicAccess'

const DENIED =
  'Microphone access is blocked. Click the icon left of the address bar, set Microphone to Allow, then reload the page.'

export function TopBar(p: {
  status: MicStatus
  message?: string
  error?: string
  devices: MediaDeviceInfo[]
  deviceId: string
  deviceDisabled: boolean
  channel: number
  latency: LatencyReport | null
  onDevice(id: string): void
  onChannel(ch: number): void
  onRetry(): void
}) {
  const { status, latency } = p
  const text =
    status === 'granted' ? 'Mic allowed' : status === 'pending' ? 'Waiting for microphone permission…'
    : status === 'denied' ? DENIED : p.message
  return (
    <header className="bar">
      <div className="bar-status">
        <span className={`status ${status}`} role="status">
          <i className="dot" aria-hidden="true" />
          {text}
        </span>
        {status === 'unavailable' && <button type="button" className="retry" onClick={p.onRetry}>Retry</button>}
        {p.error && <span className="error" role="alert">{p.error}</span>}
      </div>
      <label className="input-pick">
        Input
        <select value={p.deviceId} disabled={p.deviceDisabled} onChange={(e) => p.onDevice(e.target.value)}>
          <option value="">System default</option>
          {p.devices.map((d, i) => (
            <option key={d.deviceId || i} value={d.deviceId}>{d.label || `Input ${i + 1}`}</option>
          ))}
        </select>
      </label>
      <label className="input-pick">
        Jack
        <select value={p.channel} disabled={p.deviceDisabled} onChange={(e) => p.onChannel(Number(e.target.value))}>
          <option value={0}>Input 1</option>
          <option value={1}>Input 2</option>
        </select>
      </label>
      {latency && (
        <span className="latency">
          Latency {latency.totalMs.toFixed(1)} ms (out {(latency.baseMs + latency.outputMs).toFixed(1)} + block{' '}
          {latency.blockMs.toFixed(1)} ms / {latency.blockSamples} smp + DSP {latency.dspMs.toFixed(1)} ms)
        </span>
      )}
    </header>
  )
}
