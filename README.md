# Guitar Amp (browser)

A guitar amp simulator that runs in the browser. The DSP is Rust compiled to WebAssembly and runs in an
AudioWorklet; the amp-head GUI is React (Vite). Four presets: `clean`, `crunch`, `overdrive`, `distortion`
(the default, voiced for metal), eight knobs (Input, Drive, Bass, Mid, Treble, Presence, Master, Gate).

> **Use headphones.** With speakers, the processed sound feeds back into the guitar pickups or a mic and
> can howl loudly.

## Prerequisites

- **Rust via rustup.** Homebrew's `rustc` has no `wasm32` target, so put the rustup toolchain first on the
  PATH in every shell that builds:

  ```sh
  export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
  rustup target add wasm32-unknown-unknown   # once
  ```

- **wasm-pack:** `brew install wasm-pack`
- **Node.js** (with npm)

## Test the DSP

```sh
cd dsp && cargo test
```

## Run the app

```sh
cd web
npm run wasm && npm install   # first time: amp-dsp is a file: dependency on dsp/pkg, which wasm builds
npm run dev
```

Then open http://localhost:5173. `npm run dev` and `npm run build` both run wasm-pack first
(`npm run wasm`), which builds `dsp/` into `dsp/pkg`. The page must be served from `localhost` or HTTPS;
browsers block microphone access elsewhere.

During release builds wasm-pack may print `rust-objcopy failed: SIGABRT`. On this machine that warning is
harmless: the `.wasm` is still built and works.

## Interface setup (Scarlett Solo / 2i2)

- **Output:** set the macOS output to the Scarlett too and plug wired headphones into the interface.
  Never use Bluetooth/AirPods: they add 100–250 ms.
- **Same sample rate in and out:** in Audio MIDI Setup set the Scarlett's input and output (and any other
  output you use) to the same rate, e.g. 48 kHz. The app runs at the input's rate; a different output rate
  gets resampled (more latency), and in Firefox Start can fail.
- **Direct Monitor off** on the Solo, otherwise the dry DI signal mixes into the amp sound.
- **Input 2 (instrument jack):** press **INST**, then set the gain so the halo stays green on your hardest
  picking.

## Playing

1. Allow microphone access when the browser asks. If you blocked it, click the icon left of the address
   bar, set Microphone to Allow, and reload.
2. Pick your interface (for example the **Scarlett Solo** or **2i2**) in the **Input** picker, and in **Jack** pick the
   input the guitar is plugged into. A Scarlett is selected automatically if one is found. Jack defaults to
   Input 2 on a Scarlett **Solo** (its instrument jack; input 1 is the XLR mic), also when System default
   is the Solo, and Input 1 otherwise. Only that one channel is processed; Jack can be changed while
   playing. If the browser delivers only one channel, Input 2 shows an error: choose Input 1.
3. Press **Start**, pick a preset, and play. Drag a knob up or down (Shift for fine control), use the arrow
   keys / PageUp / PageDown / Home / End, or double-click it to reset to the preset's value.
4. **Stop** fades out, releases the microphone and closes the audio context.

## Latency

While running, the top bar shows the total: the browser's `baseLatency + outputLatency` ("out"), plus the
128-sample AudioWorklet block and the DSP's oversampler delay. The input side is not included, because
browsers do not report it. To keep it low: use the interface's own output for headphones, close other
audio apps, and set a small buffer size in the interface's settings where possible. The AudioContext uses
`latencyHint: 'interactive'` and runs at the input device's sample rate.

## Offline render

Render a WAV through every preset (or name one) for listening:

```sh
cd dsp
cargo run --release --example render -- ../guitar-clean-riff.wav            # all presets → renders/
cargo run --release --example render -- ../guitar-clean-riff.wav distortion out
```
