import { PARAMS, PRESETS } from '../audio/AmpAudio'
import { Knob } from './Knob'

const GATE = PARAMS.indexOf('Gate')
const formatKnob = (v: number) => (v * 10).toFixed(1)
const formatGate = (v: number) => (Math.round(v) <= -96 ? 'off' : `${Math.round(v)} dB`)

export function AmpFace(p: {
  preset: number
  values: number[]
  presetDefaults: number[]
  ranges: [number, number][]
  disabled: boolean
  onPreset(p: number): void
  onParam(i: number, v: number): void
}) {
  return (
    <section className="face" aria-label="Amp controls">
      <div className="brand">RUST<span>·</span>AMP</div>
      <div className="presets" role="group" aria-label="Preset">
        {PRESETS.map((name, i) => (
          <button key={name} type="button" className={i === p.preset ? 'on' : undefined}
            aria-pressed={i === p.preset} disabled={p.disabled} onClick={() => p.onPreset(i)}>
            {name}
          </button>
        ))}
      </div>
      <div className="knobs">
        {PARAMS.map((label, i) => (
          <Knob key={label} label={label} value={p.values[i]} min={p.ranges[i][0]} max={p.ranges[i][1]}
            resetValue={p.presetDefaults[i]} format={i === GATE ? formatGate : formatKnob}
            disabled={p.disabled} onChange={(v) => p.onParam(i, v)} />
        ))}
      </div>
    </section>
  )
}
