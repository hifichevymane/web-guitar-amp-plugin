import type { Dsp } from './dsp'
import processorUrl from './amp-processor.ts?worker&url'

export const PRESETS = ['clean', 'crunch', 'overdrive', 'distortion'] as const
export const PARAMS = ['Input', 'Drive', 'Bass', 'Mid', 'Treble', 'Presence', 'Master', 'Gate'] as const

const BLOCK = 128
const READY_TIMEOUT_MS = 3000
const FADE_WAIT_MS = 30 // the worklet's output fade (20 ms) plus margin
const MONO_INPUT = 'This input delivers one channel; Input 2 is unavailable. Choose Input 1.'

export interface LatencyReport {
  sampleRate: number
  baseMs: number
  outputMs: number
  blockSamples: 128
  blockMs: number
  dspSamples: number
  dspMs: number
  totalMs: number
}

export class AmpAudio {
  private readonly dsp: Dsp
  private readonly onLevel: (peak: number) => void
  private readonly onEnded: () => void
  private readonly onError?: (msg: string) => void
  private ctx: AudioContext | null = null
  private stream: MediaStream | null = null
  private source: MediaStreamAudioSourceNode | null = null
  private node: AudioWorkletNode | null = null
  private channels = 2 // channels the current input track delivers

  constructor(
    dsp: Dsp,
    onLevel: (peak: number) => void,
    onEnded: () => void,
    onError?: (msg: string) => void,
  ) {
    this.dsp = dsp
    this.onLevel = onLevel
    this.onEnded = onEnded
    this.onError = onError
  }

  get running(): boolean {
    return this.ctx !== null
  }

  async start(deviceId: string | undefined, preset: number, values: number[], channel: number): Promise<void> {
    await this.stop()
    try {
      const stream = (this.stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          deviceId: deviceId ? { exact: deviceId } : undefined,
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
          channelCount: { ideal: 2 }, // otherwise Chrome may hand over only channel 0
        },
      }))
      const track = stream.getAudioTracks()[0]
      // shortcut: unknown when the browser omits channelCount; the worklet then plays silence on Input 2.
      this.channels = track.getSettings().channelCount ?? 2
      if (channel > 0 && this.channels < 2) throw new Error(MONO_INPUT)
      // Match the mic's rate: Firefox throws on connecting mismatched rates.
      const sampleRate = track.getSettings().sampleRate
      const ctx = (this.ctx = new AudioContext(
        sampleRate ? { latencyHint: 'interactive', sampleRate } : { latencyHint: 'interactive' },
      ))
      await ctx.audioWorklet.addModule(processorUrl)
      const node = (this.node = new AudioWorkletNode(ctx, 'amp-processor', {
        outputChannelCount: [2],
        processorOptions: { module: this.dsp.module, preset, values, channel },
      }))
      // The processor posts 'ready' once WASM is instantiated; a constructor throw fires processorerror.
      const ready = new Promise<void>((resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error('Audio engine did not start (worklet timed out)')),
          READY_TIMEOUT_MS,
        )
        node.port.onmessage = (e: MessageEvent<{ type: string; peak: number }>) => {
          if (e.data.type === 'level') this.onLevel(e.data.peak)
          else if (e.data.type === 'ready') {
            clearTimeout(timer)
            resolve()
          }
        }
        node.onprocessorerror = () => {
          clearTimeout(timer)
          reject(new Error('Audio engine failed to start in the worklet'))
        }
      })
      ready.catch(() => {}) // avoid an unhandled rejection if a step below throws first
      try {
        const source = (this.source = ctx.createMediaStreamSource(stream))
        source.connect(node).connect(ctx.destination)
      } catch (e) {
        throw new Error(
          'Could not connect the input, likely because input and output sample rates differ. ' +
            `Set both to the same rate (e.g. 48 kHz) in Audio MIDI Setup. (${e instanceof Error ? e.message : String(e)})`,
        )
      }
      await ctx.resume() // the processor may not be constructed until the context runs
      await ready
      node.onprocessorerror = () => {
        void this.stop()
        if (this.onError) this.onError('Audio engine crashed')
        else this.onEnded()
      }
      track.addEventListener('ended', this.onEnded)
      if (track.readyState === 'ended') throw new Error('Input device disconnected.') // unplugged mid-start
    } catch (e) {
      await this.stop().catch(() => {}) // don't mask the original error
      throw e
    }
  }

  async stop(): Promise<void> {
    const { ctx, stream, source, node } = this
    this.ctx = this.stream = this.source = this.node = null
    if (node) {
      node.port.onmessage = null
      node.onprocessorerror = null
    }
    // a normal Stop must not report a disconnect
    stream?.getTracks().forEach((t) => t.removeEventListener('ended', this.onEnded))
    if (node && ctx?.state === 'running') {
      node.port.postMessage({ type: 'mute' }) // closing mid-signal clicks
      await new Promise((r) => setTimeout(r, FADE_WAIT_MS))
    }
    source?.disconnect()
    node?.disconnect()
    stream?.getTracks().forEach((t) => t.stop())
    if (ctx && ctx.state !== 'closed') await ctx.close()
  }

  setPreset(p: number): void {
    this.node?.port.postMessage({ type: 'preset', preset: p })
  }

  /** Input channel (0-based) the amp listens to. Throws if the running input has no such channel. */
  setChannel(ch: number): void {
    if (this.node && ch > 0 && this.channels < 2) throw new Error(MONO_INPUT)
    this.node?.port.postMessage({ type: 'channel', channel: ch })
  }

  setParam(i: number, v: number): void {
    this.node?.port.postMessage({ type: 'param', param: i, value: v })
  }

  latency(): LatencyReport | null {
    const ctx = this.ctx
    if (!ctx) return null
    const sr = ctx.sampleRate
    const ms = (samples: number) => (samples / sr) * 1000
    const baseMs = ctx.baseLatency * 1000
    const outputMs = (ctx.outputLatency || 0) * 1000 // undefined in older Safari
    const blockMs = ms(BLOCK)
    const dspSamples = this.dsp.latencySamples
    const dspMs = ms(dspSamples)
    return {
      sampleRate: sr, baseMs, outputMs, blockSamples: BLOCK, blockMs, dspSamples, dspMs,
      totalMs: baseMs + outputMs + blockMs + dspMs,
    }
  }
}
