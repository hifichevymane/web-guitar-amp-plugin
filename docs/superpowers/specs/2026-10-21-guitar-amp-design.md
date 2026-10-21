# Guitar Amp Web Plugin: Design

Date: 2026-10-21
Base requirements: [PROMPT.md](../../../PROMPT.md). This document adds what came out of the brainstorm. Where the two disagree, this document wins.

## Intent

A browser guitar amp the user actually wants to play metal through. Their rig is an **Ibanez GIO** (hot humbuckers) going into a **Focusrite Scarlett 2i2** interface (guitar in input 1) on a **Mac**, using Chrome, with headphones. The sound that matters most is the **`distortion` preset voiced for metal**: tight, high-gain, and amp-like rather than fizzy. The other three presets have to be correct and sound good, but `distortion` gets the most tuning effort.

Success means: open the app, pick the Scarlett as the input, press Start, and play tight palm-muted riffs that stay punchy, with silence between riffs and no hiss or hum.

## Goals

- Everything in PROMPT.md: four presets, Rust/WASM DSP in an AudioWorklet, React GUI, mic permission flow, low latency, reported latency, click-free and alias-free controls, tests.
- A **metal-voiced `distortion` preset** (details below).
- A **noise gate** with a user-facing threshold knob. The user asked for this; it is not in PROMPT.md.
- An **input device picker**, so the user can choose the Scarlett over the Mac's built-in mic.
- An **amp-head look** for the GUI (layout "A", chosen by the user).

## Non-goals

- No output device picker. Output is always the **Mac's default output device** (`AudioContext.destination`), as the user confirmed. Don't assume it's the Scarlett.
- No cabinet IR loading or user IRs. A built-in filter-based cab is enough.
- No saving or exporting presets, no extra effects (delay, reverb), no stereo processing.
- No UI library beyond React.

## Anti-goals (failures even if it technically works)

- A `distortion` preset that sounds like a buzzy fuzz pedal, or has flubby, muddy low end on palm mutes.
- Audible hiss or hum between riffs at `distortion` gain levels.
- Clicks, zipper noise, or dropouts when knobs move or presets change.
- Latency high enough that playing feels detached.

## Constraints

- Stack, layout, and API are fixed by PROMPT.md: `dsp/` Rust crate with `AmpEngine { new, set_preset, set_param, process }` plus a `wasm-bindgen` wrapper; `web/` Vite + React + TypeScript; graph is `MediaStreamSource → AudioWorkletNode → destination`; params go over `port.postMessage`.
- Mac + Chrome is the primary test target. Other browsers listed in PROMPT.md should work but are secondary.
- Toolchain on this machine: `cargo` (Homebrew) and `rustup` with `wasm32-unknown-unknown` installed, Node 24. **`wasm-pack` is not installed.** The builder either installs it (`cargo install wasm-pack`) or uses `wasm-bindgen-cli` directly. Check that the `cargo` in use can see the rustup wasm target, since Homebrew cargo and rustup cargo can conflict.

## Decided

### Distortion preset (metal voicing)

- **Tight low end:** a high-pass / low cut *before* the clipping stages (Tube Screamer-into-high-gain-amp style, roughly 100–200 Hz corner) so palm mutes stay punchy.
- **High gain from 2–3 cascaded soft-ish clipping stages** with inter-stage filtering, not a single hard waveshaper. PROMPT.md asks for "hard clipping"; read that as the dense, compressed character of a high-gain amp, while staying amp-like and not buzzy. Oversampled (at least 4x for this preset).
- **Default EQ:** deep bass, slightly scooped (not dead) mids, controlled treble. Mid can be turned up for leads.
- **Cab:** closed-back 4x12 character: resonant low-mid bump (around 100–120 Hz), low-pass around 5–6 kHz.
- The app **opens on `distortion`** (my call, since it's the sound the user wants).

### Noise gate

- Sits at the **input, before the gain stages**, so it sees the guitar's dynamics and not the clipped signal.
- One user knob, **Gate** (threshold in dB; lowest position = gate off). It has attack/hold/release and hysteresis so it doesn't chatter on decaying notes or cut sustain abruptly. The exact times are up to the builder.
- Each preset sets its own gate default: off or very low for `clean`, firmest for `distortion`.
- Gate threshold is part of the preset values and changes with the preset like the other knobs.

### Tuning reference: the user's DI recording

The user supplied `guitar-clean-riff.wav` in the repo root: a clean DI riff from their GIO through the Scarlett 2i2. It's 7.8 s, 48 kHz, 16-bit, 2 channels, both channels identical. Measured: peak −17.7 dBFS, RMS −36.3 dBFS, no DC, quietest 50 ms window around −52 dBFS.

- Build an **offline render tool** in `dsp/` (e.g., a cargo example: `cargo run --example render -- <in.wav> <preset> <out.wav>`) that runs the file through `AmpEngine` in 128-sample blocks and writes a WAV per preset. Use it for all tone tuning, and give the user the rendered files to listen to.
- **Calibrate gain staging to this file:** treat a peak around −18 dBFS as "normal picking" for this rig. At that level, `clean` must not clip, `crunch` should just break up on the hardest notes, and `distortion` should be fully saturated.
- **Calibrate the gate to this file:** the `distortion` preset's default gate should silence the gaps in the riff without chopping note tails. The noise floor is roughly −52 dBFS.
- Tuning metrics (spectral balance, aliasing checks) are fine as supporting evidence, but the user's ear on the rendered files has the final say.
- The test suite may use this file as a fixture in addition to the synthetic pluck. Read it with a small WAV dependency such as `hound`, as a dev-dependency only.

### Other presets

As in PROMPT.md: `clean` keeps headroom, `crunch` breaks up on hard picking, `overdrive` sustains smoothly. They need to be voiced so a hot humbucker through a Scarlett at normal interface gain lands in the intended range. Calibrate input levels to that, not to a weak single-coil.

### Input handling (my calls)

- `getUserMedia` with `echoCancellation`, `noiseSuppression`, `autoGainControl` all **off**, and `deviceId` from the picker.
- Use **channel 1 only** of the input, not a stereo downmix. The user confirmed a **Scarlett 2i2** with the guitar in input 1. Force a mono input to the worklet. No channel selector is needed.
- **Input picker:** lists audio inputs via `enumerateDevices()` (labels only appear after permission). Changing it while running restarts the input stream with the new device.
- Output: default `AudioContext.destination`, `latencyHint: 'interactive'`.

### Screen (layout A: amp head)

- **Top bar:** mic status (pending / granted / denied; the denied state explains how to re-enable mic access in browser site settings), the input picker, and a latency readout (`baseLatency + outputLatency` in ms, plus the DSP block size in samples and ms).
- **Faceplate:** dark amp-head panel with a gold/tan trim. Four preset buttons (`clean`, `crunch`, `overdrive`, `distortion`), with the active one highlighted. Eight round knobs: **Input, Drive, Bass, Mid, Treble, Presence, Master, Gate**. Drag up or down to turn a knob. Double-click resets it to the current preset's value (my call). Knobs must also work from the keyboard (arrow keys) and have accessible labels.
- **Bottom:** Start/Stop button and an input level meter.
- Start/Stop and the knobs that affect sound are disabled until mic permission is granted. Stop stops all tracks and closes the `AudioContext`, so the browser's recording indicator turns off.
- The reference mockup is `.superpowers/brainstorm/*/content/layout.html`, card A.

### README

Tells the user to use headphones (feedback risk), to set the Scarlett as the input in the app's picker, and to serve over localhost. Also covers how to build (`cargo test`, the wasm build, `npm run dev`).

## Left to the builder

- Exact DSP topology per preset: number of stages, clipping curves, filter types, tone-stack model (a TMB-style passive model or a well-behaved approximation, as long as the controls interact realistically).
- Oversampling method and factor per preset (at least 2x for crunch/overdrive, 4x recommended for distortion), within the latency target.
- Parameter ranges and units beyond what's stated, smoothing times (10–20 ms per PROMPT.md), and the gate's timing constants.
- How the WASM gets into the AudioWorklet: for example, fetch bytes on the main thread, post them to the worklet, and instantiate synchronously there. Watch for `TextDecoder`/`TextEncoder` missing in `AudioWorkletGlobalScope`, which breaks the stock wasm-bindgen glue; polyfill it or avoid it.
- Level meter implementation (e.g., the worklet posts peak/RMS at a throttled rate).
- Visual styling details within the amp-head look.
- Test structure, as long as PROMPT.md's required tests are covered: preset switching, param ranges, no NaN/Inf, bounded output, no DC offset, a decaying-pluck test showing crunch distorts more with louder input, and also a gate test (silence/low noise in → silence out; a note in → passes).
