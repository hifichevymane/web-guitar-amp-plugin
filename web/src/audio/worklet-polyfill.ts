// amp_dsp.js calls `new TextDecoder()` at module top level, and some browsers' AudioWorklet
// scope lacks it. Must be imported before 'amp-dsp'. Only used for Rust panic/error strings.
const g = globalThis as { TextDecoder?: unknown }
g.TextDecoder ??= class {
  decode(bytes?: Uint8Array): string {
    let s = ''
    for (const b of bytes ?? []) s += String.fromCharCode(b) // shortcut: Latin-1 only, fine for error text
    return s
  }
}
