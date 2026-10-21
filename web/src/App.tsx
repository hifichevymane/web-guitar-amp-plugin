import { useEffect, useRef, useState } from 'react'
import './App.css'
import { AmpAudio, PARAMS } from './audio/AmpAudio'
import type { LatencyReport } from './audio/AmpAudio'
import { loadDsp } from './audio/dsp'
import type { Dsp } from './audio/dsp'
import { useMicAccess } from './audio/useMicAccess'
import { AmpFace } from './components/AmpFace'
import { LevelMeter } from './components/LevelMeter'
import { TopBar } from './components/TopBar'

const DISTORTION = 3

const message = (e: unknown) => (e instanceof Error ? e.message : String(e))

export default function App() {
  const mic = useMicAccess()
  const [dsp, setDsp] = useState<Dsp>()
  const amp = useRef<AmpAudio | null>(null)
  const [preset, setPreset] = useState(DISTORTION)
  const [values, setValues] = useState<number[]>([])
  const [deviceId, setDeviceId] = useState<string>() // undefined = not chosen yet; '' = system default
  const [jack, setJack] = useState<number>() // 0-based input channel; undefined = derived from the device
  const [running, setRunning] = useState(false)
  const [busy, setBusy] = useState(false)
  const busyRef = useRef(false) // sync guard: two clicks can land before React re-renders the disabled button
  const [error, setError] = useState<string>()
  const [latency, setLatency] = useState<LatencyReport | null>(null)
  const [level, setLevel] = useState(0)

  async function run(task: (a: AmpAudio) => Promise<void>) {
    const a = amp.current
    if (!a || busyRef.current) return
    busyRef.current = true
    setBusy(true)
    try {
      await task(a)
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  async function startWith(a: AmpAudio, id: string, ch: number) {
    setError(undefined)
    setDeviceId(id) // pin: a later devicechange must not make the picker show another device
    try {
      await a.start(id || undefined, preset, values, ch)
      setRunning(true)
    } catch (e) {
      setRunning(false)
      setLevel(0)
      setError(`Could not start audio: ${message(e)}`)
    }
  }

  async function stopWith(a: AmpAudio) {
    await a.stop()
    setRunning(false)
    setLevel(0)
  }

  // Latest handlers for AmpAudio's callbacks, which are bound once at construction.
  const onEnded = useRef<() => void>(() => {})
  const onError = useRef<(msg: string) => void>(() => {})
  useEffect(() => {
    onEnded.current = () =>
      void run(async (a) => {
        await stopWith(a)
        setError('Input device disconnected.')
      })
    onError.current = (msg) => {
      setRunning(false)
      setLevel(0)
      setError(msg)
    }
  })

  useEffect(() => {
    let cancelled = false
    loadDsp().then(
      (d) => {
        if (cancelled) return
        amp.current = new AmpAudio(d, setLevel, () => onEnded.current(), (m) => onError.current(m))
        setValues(d.presetValues(DISTORTION))
        setDsp(d)
      },
      (e) => !cancelled && setError(`Could not load the audio engine: ${message(e)}`),
    )
    return () => {
      cancelled = true
      void amp.current?.stop()
      amp.current = null
    }
  }, [])

  // Until the user picks one: the Scarlett if present (labels appear after permission), else system default.
  const device = deviceId ?? mic.devices.find((d) => d.label.includes('Scarlett'))?.deviceId ?? ''
  // On a Scarlett Solo the instrument jack is input 2 (input 1 is the XLR mic). '' = Chrome's 'default' entry.
  const jackFor = (id: string) =>
    mic.devices.find((d) => d.deviceId === (id || 'default'))?.label.includes('Solo') ? 1 : 0
  const channel = jack ?? jackFor(device)

  useEffect(() => {
    if (!running) return
    const poll = () => setLatency(amp.current?.latency() ?? null)
    poll()
    const t = setInterval(poll, 1000)
    return () => clearInterval(t)
  }, [running])

  function choosePreset(p: number) {
    if (!dsp) return
    setPreset(p)
    setValues(dsp.presetValues(p))
    amp.current?.setPreset(p)
  }

  function changeParam(i: number, v: number) {
    setValues((vs) => vs.map((x, j) => (j === i ? v : x)))
    amp.current?.setParam(i, v)
  }

  function chooseDevice(id: string) {
    setDeviceId(id)
    setJack(undefined) // re-derive the jack for the new device
    if (running) void run(async (a) => {
      await stopWith(a)
      await startWith(a, id, jackFor(id))
    })
  }

  function chooseChannel(ch: number) {
    try {
      amp.current?.setChannel(ch) // live, no restart
    } catch (e) {
      setError(message(e)) // keep playing the current jack
      return
    }
    setError(undefined)
    setJack(ch)
  }

  const granted = mic.status === 'granted'
  return (
    <main className="amp">
      <TopBar status={mic.status} message={mic.message} error={error} devices={mic.devices}
        deviceId={device} deviceDisabled={!granted || busy} channel={channel}
        latency={running ? latency : null} onDevice={chooseDevice} onChannel={chooseChannel} onRetry={mic.retry} />
      {dsp ? (
        <AmpFace preset={preset} values={values} presetDefaults={dsp.presetValues(preset)}
          ranges={PARAMS.map((_, i) => dsp.paramRange(i))} disabled={!granted || busy}
          onPreset={choosePreset} onParam={changeParam} />
      ) : (
        <section className="face loading">Loading audio engine…</section>
      )}
      <div className="row">
        <button type="button" className={'power' + (running ? ' on' : '')} disabled={!granted || !dsp || busy}
          onClick={() => void run((a) => (running ? stopWith(a) : startWith(a, device, channel)))}>
          {running ? '■ Stop' : '▶ Start'}
        </button>
        <LevelMeter peak={level} />
      </div>
    </main>
  )
}
