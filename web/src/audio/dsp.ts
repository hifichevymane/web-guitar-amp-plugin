import init, { dsp_latency_samples, param_max, param_min, preset_values } from 'amp-dsp'
import wasmUrl from 'amp-dsp/amp_dsp_bg.wasm?url'

export interface Dsp {
  module: WebAssembly.Module
  presetValues(p: number): number[]
  paramRange(i: number): [number, number]
  latencySamples: number
}

let cached: Promise<Dsp> | null = null

/** Compiles the .wasm once; the same Module is handed to the AudioWorklet. */
export function loadDsp(): Promise<Dsp> {
  cached ??= load().catch((e) => {
    cached = null // let a later call retry
    throw e
  })
  return cached
}

async function load(): Promise<Dsp> {
  const res = await fetch(wasmUrl)
  if (!res.ok) throw new Error(`failed to fetch DSP: ${res.status} ${res.statusText}`)
  const module = await WebAssembly.compile(await res.arrayBuffer())
  await init({ module_or_path: module })
  return {
    module,
    presetValues: (p) => Array.from(preset_values(p)),
    paramRange: (i) => [param_min(i), param_max(i)],
    latencySamples: dsp_latency_samples(),
  }
}
