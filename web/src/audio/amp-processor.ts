import './worklet-polyfill'
import { initSync, WasmAmp } from 'amp-dsp'

declare const sampleRate: number
declare function registerProcessor(name: string, ctor: unknown): void
declare class AudioWorkletProcessor {
  readonly port: MessagePort
}

const MAX_BLOCK = 1024 // = wasm::MAX_BLOCK
const LEVEL_EVERY = 16 // render quanta; level = input peak (input meter)
const JACK_FADE_S = 0.0025 // fade out, switch, fade in: ~5 ms total, no click on a live Jack change
const MUTE_FADE_S = 0.02 // output fade before Stop closes the context

interface Options {
  processorOptions: { module: WebAssembly.Module; preset: number; values: number[]; channel: number }
}

type Msg =
  | { type: 'preset'; preset: number }
  | { type: 'param'; param: number; value: number }
  | { type: 'channel'; channel: number }
  | { type: 'mute' }

class AmpProcessor extends AudioWorkletProcessor {
  private memory: WebAssembly.Memory
  private amp: WasmAmp
  private inView!: Float32Array
  private outView!: Float32Array
  private quanta = 0
  private peak = 0
  private channel: number
  private nextChannel: number
  private inGain = 1
  private outGain = 1
  private outTarget = 1
  private readonly jackStep = 1 / (JACK_FADE_S * sampleRate)
  private readonly muteStep = 1 / (MUTE_FADE_S * sampleRate)

  constructor(options: Options) {
    super()
    const { module, preset, values, channel } = options.processorOptions
    this.channel = this.nextChannel = channel
    this.memory = initSync({ module }).memory
    this.amp = new WasmAmp(sampleRate)
    this.amp.set_preset(preset)
    values.forEach((v, i) => this.amp.set_param(i, v))
    this.buildViews()
    this.port.onmessage = (e: MessageEvent<Msg>) => {
      const m = e.data
      if (m.type === 'preset') this.amp.set_preset(m.preset)
      else if (m.type === 'param') this.amp.set_param(m.param, m.value)
      else if (m.type === 'channel') this.nextChannel = m.channel
      else if (m.type === 'mute') this.outTarget = 0
    }
    this.port.postMessage({ type: 'ready' })
  }

  private buildViews() {
    this.inView = new Float32Array(this.memory.buffer, this.amp.input_ptr(), MAX_BLOCK)
    this.outView = new Float32Array(this.memory.buffer, this.amp.output_ptr(), MAX_BLOCK)
  }

  process(inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    // memory.grow detaches the old buffer
    if (this.inView.buffer !== this.memory.buffer) this.buildViews()
    const out = outputs[0]
    const n = Math.min(out[0].length, MAX_BLOCK)
    // Switch jacks only once the old one has faded to silence.
    if (this.channel !== this.nextChannel && this.inGain <= 0) this.channel = this.nextChannel
    const step = this.channel === this.nextChannel ? this.jackStep : -this.jackStep
    // The selected jack; missing (Input 2 on a mono device) = silence, never another jack's signal.
    const x = inputs[0]?.[this.channel]
    let peak = this.peak
    let g = this.inGain
    const inV = this.inView
    const m = x ? Math.min(n, x.length) : 0
    for (let i = 0; i < n; i++) {
      g = Math.min(1, Math.max(0, g + step))
      const v = i < m ? x![i] : 0
      const a = Math.abs(v)
      if (a > peak) peak = a
      inV[i] = v * g
    }
    this.inGain = g

    this.amp.process_block(n)

    const y = this.outView
    if (this.outGain > this.outTarget || this.outGain === 0) {
      let og = this.outGain
      for (let i = 0; i < n; i++) {
        og = Math.max(this.outTarget, og - this.muteStep)
        y[i] *= og
      }
      this.outGain = og
    }
    for (let c = 0; c < out.length; c++) {
      const ch = out[c]
      for (let i = 0; i < n; i++) ch[i] = y[i]
    }

    if (++this.quanta >= LEVEL_EVERY) {
      this.port.postMessage({ type: 'level', peak })
      this.quanta = 0
      peak = 0
    }
    this.peak = peak
    return true
  }
}

registerProcessor('amp-processor', AmpProcessor)
