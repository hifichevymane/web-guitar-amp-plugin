# Guitar Amp Web Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Browser guitar amp. A Rust DSP engine compiled to WASM runs in an AudioWorklet, behind a React amp-head GUI, and the `distortion` preset is voiced for metal.

**Architecture:** `dsp/` is a std Rust crate with no allocation in `process`. Each sample goes through: input HP → noise gate → input gain → tight HP → 4x-oversampled 3-stage asymmetric clipper → tone stack → presence + power-amp saturation → cabinet filter → master → DC block → safety limiter. A `wasm` feature adds a thin `wasm-bindgen` wrapper. `web/` is Vite + React + TS. The main thread compiles the `.wasm` once and uses it for preset tables, then passes the compiled `WebAssembly.Module` to the AudioWorklet, which instantiates it with `initSync`. Graph: `MediaStreamSource → AudioWorkletNode → destination`.

**Tech Stack:** Rust (stable, rustup toolchain), wasm-bindgen 0.2, wasm-pack, hound (dev-only), Vite, React 18+, TypeScript.

**Spec:** [docs/superpowers/specs/2026-10-21-guitar-amp-design.md](../specs/2026-10-21-guitar-amp-design.md). Base requirements: [PROMPT.md](../../../PROMPT.md). Executors read both.

**No git:** the user does not want a repository. There are no commit steps.

## Global Constraints

- Toolchain: Homebrew's `rustc` (`/opt/homebrew/bin`) has **no wasm32 target**. The rustup toolchain does. Every shell that runs cargo or wasm-pack first runs `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`.
- `dsp/` API (PROMPT.md): `AmpEngine::new(sample_rate: f32)`, `set_preset(Preset)`, `set_param(Param, f32)`, `process(&[f32], &mut [f32])`, mono, block-based, **no allocation in `process`**.
- Preset names, exactly: `clean`, `crunch`, `overdrive`, `distortion`. The app opens on `distortion`.
- Knobs, in this order everywhere (the Rust `Param` index = the TS array index): Input, Drive, Bass, Mid, Treble, Presence, Master, Gate.
- Output: default `AudioContext.destination`. No output picker. `latencyHint: 'interactive'`.
- Input: `echoCancellation`, `noiseSuppression`, `autoGainControl` all `false`. Channel 1 only (the guitar on the Scarlett 2i2).
- Parameter smoothing: 10–20 ms ramps (use 15 ms linear). Preset switches ramp too. No structural switching between presets.
- Latency report: `baseLatency + outputLatency`, plus the DSP block size (128 samples) and the oversampler delay.
- Runtime dependencies: React only, plus `wasm-bindgen` in Rust. `hound` is a dev-dependency only. No UI library.
- Tuning reference: `guitar-clean-riff.wav` (48 kHz, 16-bit, 2 identical channels, peak −17.7 dBFS, floor about −52 dBFS). A peak around −18 dBFS counts as "normal picking".
- README: use headphones, pick the Scarlett in the input picker, serve on localhost, build steps.

## Review Focus

1. **Mic stream and AudioContext sample rates differ** (Firefox throws on connect, and Safari can resample). Expected: Start works on any browser. Pinned in Task 7 by creating the context at `track.getSettings().sampleRate`.
2. **Start fails halfway** (device busy, unplugged, `addModule` error). Expected: an error message, and the mic indicator turns off. Pinned in Task 7, Step 4.
3. **Interface unplugged while playing** (track `ended`). Expected: the engine stops, the mic is released, and a message says why. Pinned in Task 8, Step 5.
4. **Idle after the gate closes** (filter states decaying into denormals can spike CPU and cause underruns). Expected: output is exactly 0.0 when idle. Pinned by `idle_output_is_exact_zero` in Task 5.
5. **Rapid Start/Stop clicks, or changing device while running.** Expected: never two contexts, never an orphaned mic stream. Pinned in Task 8, Step 5.

---

## File Structure

```text
dsp/
  Cargo.toml                 crate "amp-dsp"; lib crate-type ["cdylib","rlib"]; feature "wasm"
  src/lib.rs                 mod decls + pub use of Preset, Param, AmpEngine, DSP_LATENCY_SAMPLES
  src/params.rs              Preset, Param, ranges, preset knob tables, Voicing table
  src/filters.rs             Biquad (RBJ, TDF-II), OnePole, Smoother
  src/oversample.rs          Oversampler4x, LATENCY_SAMPLES
  src/shaper.rs              clip(), fast_tanh(), safety_limit()
  src/gate.rs                NoiseGate
  src/tonestack.rs           ToneStack
  src/cab.rs                 Cabinet
  src/engine.rs              AmpEngine (the chain)
  src/wasm.rs                #[cfg(feature="wasm")] WasmAmp + free fns
  tests/common/mod.rs        signal generators + measurement helpers
  tests/engine.rs            engine safety tests (Task 5)
  tests/tone.rs              tone-character tests (Task 6)
  examples/render.rs         offline WAV renderer
web/
  package.json, vite.config.ts, tsconfig*.json, index.html
  src/main.tsx, src/App.tsx, src/App.css
  src/audio/dsp.ts           main-thread WASM loader
  src/audio/AmpAudio.ts      audio graph lifecycle
  src/audio/amp-processor.ts AudioWorklet processor
  src/audio/useMicAccess.ts  permission + device list hook
  src/components/TopBar.tsx, Knob.tsx, AmpFace.tsx, LevelMeter.tsx
README.md
```

---

### Task 1: Toolchain + crate skeleton + parameters

**Files:** Create `dsp/Cargo.toml`, `dsp/src/lib.rs`, `dsp/src/params.rs`.

**Interfaces — Produces:**
```rust
pub const PARAM_COUNT: usize = 8;
pub const GATE_OFF_DB: f32 = -96.0;
#[repr(u32)] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset { Clean = 0, Crunch = 1, Overdrive = 2, Distortion = 3 }
impl Preset {
    pub const ALL: [Preset; 4];
    pub fn from_index(i: u32) -> Option<Preset>;
    pub fn knobs(self) -> [f32; PARAM_COUNT];   // indexed by Param as usize
    pub fn voicing(self) -> Voicing;
}
#[repr(u32)] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param { InputGain = 0, Drive, Bass, Mid, Treble, Presence, Master, Gate }
impl Param {
    pub const ALL: [Param; PARAM_COUNT];
    pub fn from_index(i: u32) -> Option<Param>;
    pub fn range(self) -> (f32, f32);           // Gate: (GATE_OFF_DB, -30.0); all others (0.0, 1.0)
    pub fn clamp(self, v: f32) -> Option<f32>;  // None for NaN/Inf
}
#[derive(Clone, Copy, Debug)]
pub struct Voicing { pub tight_hz: f32, pub drive_min_db: f32, pub drive_max_db: f32,
                     pub bias: f32, pub hardness: f32, pub mid_hz: f32, pub level_db: f32 }
```

Starting tables (Task 9 retunes them; tests must not depend on exact numbers):

| preset | In | Drive | Bass | Mid | Treble | Pres | Master | Gate dB |
|---|---|---|---|---|---|---|---|---|
| clean | 0.5 | 0.2 | 0.5 | 0.55 | 0.6 | 0.5 | 0.6 | −96 |
| crunch | 0.5 | 0.5 | 0.55 | 0.6 | 0.6 | 0.55 | 0.55 | −70 |
| overdrive | 0.5 | 0.6 | 0.55 | 0.65 | 0.55 | 0.5 | 0.5 | −65 |
| distortion | 0.5 | 0.75 | 0.65 | 0.4 | 0.6 | 0.55 | 0.45 | −50 |

| preset | tight_hz | drive_min_db | drive_max_db | bias | hardness | mid_hz | level_db |
|---|---|---|---|---|---|---|---|
| clean | 40 | −6 | 12 | 0.05 | 0.0 | 600 | 0 |
| crunch | 80 | 0 | 24 | 0.15 | 0.2 | 700 | −4 |
| overdrive | 120 | 6 | 36 | 0.10 | 0.3 | 750 | −6 |
| distortion | 170 | 18 | 54 | 0.08 | 0.7 | 650 | −9 |

- [ ] **Step 1: Toolchain.** Run `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"; rustc --print sysroot`. Expected: a path under `~/.rustup`. Then run `brew install wasm-pack && wasm-pack --version`. Expected: a version prints.
- [ ] **Step 2: Cargo.toml.** `[lib] crate-type = ["cdylib","rlib"]`. `[features] wasm = ["dep:wasm-bindgen"]`. `wasm-bindgen = { version = "0.2", optional = true }`. `[dev-dependencies] hound = "3"`. `[profile.release] opt-level = 3, lto = true, codegen-units = 1, panic = "abort"`.
- [ ] **Step 3: Failing tests** in `params.rs` `#[cfg(test)]`:
  - `from_index_roundtrip`: `Preset::from_index(p as u32) == Some(p)` for all presets, `Preset::from_index(4) == None`. Same for `Param` with `8`.
  - `clamp_limits`: `Param::Bass.clamp(1.5) == Some(1.0)`, `Param::Bass.clamp(-1.0) == Some(0.0)`, `Param::Gate.clamp(-200.0) == Some(-96.0)`, `Param::Gate.clamp(0.0) == Some(-30.0)`, `Param::Mid.clamp(f32::NAN) == None`, `clamp(f32::INFINITY) == None`.
  - `preset_tables_in_range`: every `knobs()` value `v` for every param satisfies `p.clamp(v) == Some(v)`.
- [ ] **Step 4:** Run `cd dsp && cargo test params`. Expected: FAIL (not defined).
- [ ] **Step 5:** Implement `params.rs` with the tables above. `lib.rs` declares the modules and re-exports.
- [ ] **Step 6:** Run `cargo test params`. Expected: PASS.

---

### Task 2: Filters, smoother, oversampler

**Files:** Create `dsp/src/filters.rs`, `dsp/src/oversample.rs`. Unit tests go in each file.

**Interfaces — Produces:**
```rust
// filters.rs. All structs are Copy-able plain data with no heap.
pub struct Biquad;  impl Biquad {
    pub fn new() -> Self;   // identity
    pub fn set_highpass(&mut self, sr: f32, hz: f32, q: f32);
    pub fn set_lowpass(&mut self, sr: f32, hz: f32, q: f32);
    pub fn set_peaking(&mut self, sr: f32, hz: f32, q: f32, db: f32);
    pub fn set_low_shelf(&mut self, sr: f32, hz: f32, db: f32);
    pub fn set_high_shelf(&mut self, sr: f32, hz: f32, db: f32);
    pub fn process(&mut self, x: f32) -> f32;
    pub fn reset(&mut self);
}
pub struct OnePole; impl OnePole {
    pub fn lowpass(sr: f32, hz: f32) -> Self; pub fn highpass(sr: f32, hz: f32) -> Self;
    pub fn process(&mut self, x: f32) -> f32; pub fn reset(&mut self);
}
pub struct Smoother; impl Smoother {   // linear ramp, 15 ms
    pub fn new(sr: f32, initial: f32) -> Self;
    pub fn set_target(&mut self, v: f32);
    pub fn next(&mut self) -> f32;       // advance one sample
    pub fn value(&self) -> f32; pub fn is_settled(&self) -> bool;
}
pub fn db_to_lin(db: f32) -> f32;
// oversample.rs
pub const OVERSAMPLE: usize = 4;
pub const LATENCY_SAMPLES: usize;       // base-rate delay of upsample+downsample, must be <= 32
pub struct Oversampler4x; impl Oversampler4x {
    pub fn new() -> Self;
    pub fn upsample(&mut self, x: f32) -> [f32; 4];
    pub fn downsample(&mut self, xs: [f32; 4]) -> f32;
    pub fn reset(&mut self);
}
```
Biquad uses RBJ cookbook coefficients in transposed direct form II. Every `Biquad::process` and `OnePole::process` flushes any state with `|s| < 1e-20` to `0.0`, so idle signals settle to exact zero (Review Focus 4). Oversampler: two cascaded 2x halfband FIR stages (Kaiser-windowed or equiripple, designed in `new()` into fixed arrays, or as const tables).

- [ ] **Step 1: Failing tests.** Add a local helper `gain_db(f: impl FnMut(f32)->f32, sr, hz)`: run a 1 s sine and measure the RMS ratio over the last 0.5 s.
  - `biquad_lowpass`: LP at 1 kHz with Q 0.707, sr 48000. Gain at 100 Hz is within ±0.5 dB. Gain at 10 kHz is ≤ −35 dB.
  - `biquad_peaking`: +6 dB at 1 kHz with Q 1 gives 6 ± 0.3 dB at 1 kHz and within ±0.5 dB at 50 Hz.
  - `biquad_flushes_to_zero`: an impulse, then 48000 zeros, gives a final output that is exactly `0.0`.
  - `smoother_ramp`: at sr 48000 from 0 to 1, after 360 samples (7.5 ms) the value is 0.5 ± 0.01, after 720 it is 1.0 and `is_settled()`, and the value never overshoots.
  - `oversampler_roundtrip`: a 1 kHz sine with amplitude 0.5, upsampled and then downsampled, matches the input delayed by `LATENCY_SAMPLES` with RMS error < 1e-3.
  - `oversampler_image_rejection`: upsample a 10 kHz sine at sr 48000. In the 192 kHz stream, the power at 38 kHz is ≤ −60 dB relative to 10 kHz (Goertzel).
  - `latency_small`: `LATENCY_SAMPLES <= 32`.
- [ ] **Step 2:** Run `cargo test filters oversample`. Expected: FAIL.
- [ ] **Step 3:** Implement both files.
- [ ] **Step 4:** Run `cargo test filters oversample`. Expected: PASS.

---

### Task 3: Clipper and noise gate

**Files:** Create `dsp/src/shaper.rs`, `dsp/src/gate.rs`. Unit tests go in each file.

**Interfaces — Produces:**
```rust
pub fn fast_tanh(x: f32) -> f32;                     // rational approx, |err| < 1e-3, |out| <= 1
pub fn clip(x: f32, bias: f32, hardness: f32) -> f32; // hardness 0 = tanh-soft, 1 = near-hard knee
pub fn safety_limit(x: f32) -> f32;                  // linear for |x| <= 0.8, then 0.8 + 0.2*tanh((|x|-0.8)/0.2), sign kept
pub struct NoiseGate; impl NoiseGate {
    pub fn new(sr: f32) -> Self;
    pub fn set_threshold_db(&mut self, db: f32);     // <= GATE_OFF_DB: bypass
    pub fn process(&mut self, x: f32) -> f32;
}
```
`clip` must be asymmetric through `bias`, using the form `g(x + bias) − g(bias)` so that `clip(0) == 0`. Both `bias` and `hardness` are continuous so the engine can ramp them. The builder picks the curve family, for example a crossfade between `fast_tanh` and a harder rational knee. Gate: peak envelope detector (1 ms attack, 50 ms release). It opens at the threshold and closes at threshold −6 dB (hysteresis). Hold 50 ms. Gain ramps with 1 ms attack and 80 ms release.

- [ ] **Step 1: Failing tests.**
  - `clip_zero_is_zero`: for bias in {0, 0.05, 0.15} and hardness in {0, 0.5, 1}, `clip(0.0, b, h) == 0.0`.
  - `clip_bounded_monotonic`: over x from −1e6 to 1e6 (log sweep plus a linear sweep of ±10), `|clip| < 1.5` and non-decreasing, for all of the above (b, h).
  - `clip_asymmetric`: `clip(2.0, 0.15, 0.2) + clip(-2.0, 0.15, 0.2)` has magnitude > 0.01.
  - `clip_small_signal_linear`: for x = 0.001, `clip(x, b, h) / x` is in [0.8, 1.2].
  - `safety_limit_bounds`: `safety_limit(0.5) == 0.5`, `|safety_limit(±1e9)| < 1.0`, and it is continuous at 0.8 (|f(0.8+1e-4) − f(0.8)| < 1e-3).
  - `gate_off_is_bit_exact`: with threshold −96, the output equals the input exactly for white noise.
  - `gate_silences_noise`: white noise at −70 dBFS RMS with threshold −50 gives output RMS ≤ −100 dBFS over 300–1000 ms.
  - `gate_passes_note_fast`: silence for 200 ms, then a −20 dBFS 220 Hz sine. Within 96 samples (2 ms) of onset the output envelope is ≥ 90% of the input. After 10 ms the gain is ≥ 0.99.
  - `gate_no_chatter`: a sine decaying from −20 to −60 dBFS over 2 s, plus −75 dBFS noise, with threshold −50. The gain goes from open to closed at most once.
- [ ] **Step 2:** Run `cargo test shaper gate`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test shaper gate`. Expected: PASS.

---

### Task 4: Tone stack and cabinet

**Files:** Create `dsp/src/tonestack.rs`, `dsp/src/cab.rs`. Unit tests go in each file, reusing the `gain_db` pattern from Task 2.

**Interfaces — Produces:**
```rust
pub struct ToneStack; impl ToneStack {
    pub fn new(sr: f32) -> Self;
    pub fn set(&mut self, bass: f32, mid: f32, treble: f32, mid_hz: f32); // knobs 0..1
    pub fn process(&mut self, x: f32) -> f32;
}
pub struct Cabinet; impl Cabinet { pub fn new(sr: f32) -> Self; pub fn process(&mut self, x: f32) -> f32; }
```
ToneStack: a low shelf at 120 Hz, a peaking band at `mid_hz` with Q 0.8, and a high shelf at 2.5 kHz. The dB values are `bass_db = (bass−0.5)·24`, `mid_db = (mid−0.5)·20`, `treble_db = (treble−0.5)·24`. Interaction, like a passive stack's mid pot in the ground leg: the shelf gains are scaled by `0.6 + 0.4·(1 − mid)`. Cabinet (fixed, a closed-back 4x12): HP 70 Hz with Q 0.7, peak 110 Hz +4 dB with Q 1.4, peak 2.2 kHz +2 dB with Q 1.0, and a 4th-order Butterworth LP at 5.5 kHz (two biquads, Q 0.541 and 1.307).

- [ ] **Step 1: Failing tests.**
  - `tonestack_flat_at_noon`: with all knobs at 0.5 the gain is within ±0.5 dB at 80, 500, 3000 and 8000 Hz.
  - `tonestack_bass_treble_mid`: bass=1 gives ≥ +6 dB at 80 Hz and ≤ 1 dB change at 3 kHz. treble=1 gives ≥ +6 dB at 6 kHz. mid=0 gives ≤ −6 dB at `mid_hz`.
  - `tonestack_interacts`: the bass=1 boost at 80 Hz is smaller with mid=1 than with mid=0.
  - `cab_shape`, all relative to 1 kHz: 110 Hz is ≥ 2 dB above 400 Hz, 40 Hz is ≤ −6 dB, and 10 kHz is ≤ −20 dB. Run at sr 44100, 48000 and 96000.
- [ ] **Step 2:** Run `cargo test tonestack cab`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test tonestack cab`. Expected: PASS.

---

### Task 5: AmpEngine chain + safety tests

**Files:** Create `dsp/src/engine.rs`, `dsp/tests/common/mod.rs`, `dsp/tests/engine.rs`.

**Interfaces:**
- **Consumes:** everything from Tasks 1–4.
- **Produces:**
```rust
pub const DSP_LATENCY_SAMPLES: usize = oversample::LATENCY_SAMPLES;
pub struct AmpEngine; impl AmpEngine {
    pub fn new(sample_rate: f32) -> Self;           // starts on Preset::Distortion
    pub fn set_preset(&mut self, preset: Preset);   // knobs <- preset.knobs(), voicing targets <- preset.voicing(), all ramped
    pub fn set_param(&mut self, param: Param, value: f32); // Param::clamp; None (NaN/Inf) ignored
    pub fn param(&self, param: Param) -> f32;       // current target
    pub fn preset(&self) -> Preset;
    pub fn process(&mut self, input: &[f32], output: &mut [f32]); // n = min(len); output[n..] = 0.0
    pub fn reset(&mut self);
}
// tests/common/mod.rs
pub fn sine(sr: f32, hz: f32, amp: f32, secs: f32) -> Vec<f32>;
pub fn pluck(sr: f32, hz: f32, peak: f32, secs: f32) -> Vec<f32>; // harmonics 1..10, amp 1/h, decay exp(-t*(1.5+0.6h)), normalized to peak
pub fn noise(seed: u32, rms: f32, n: usize) -> Vec<f32>;          // deterministic LCG, white
pub fn run(e: &mut AmpEngine, x: &[f32]) -> Vec<f32>;             // 128-sample blocks
pub fn rms(x: &[f32]) -> f32; pub fn db(x: f32) -> f32;
pub fn goertzel_power(x: &[f32], hz: f32, sr: f32) -> f32;
pub fn thd(x: &[f32], f0: f32, sr: f32) -> f32;   // sqrt((P_total_dc_removed - P_f0)/P_f0), window = integer cycles
pub fn max_step(x: &[f32]) -> f32;                // max |x[n]-x[n-1]|
```

**Chain (decided):** per base-rate sample:
1. `OnePole` HP at 20 Hz.
2. `NoiseGate` (Gate param).
3. Input gain `db_to_lin((In−0.5)·24)`.
4. Tight HP biquad at `voicing.tight_hz`, Q 0.7.
5. Upsample 4x. For each of the 3 stages: `x = clip(x·g, bias, hardness)`, then a 4x-rate one-pole LP at 7 kHz, then a 4x-rate one-pole HP at 30 Hz. Here `g = db_to_lin(lerp(drive_min_db, drive_max_db, Drive)/3)`. Then downsample.
6. ToneStack(Bass, Mid, Treble, `mid_hz`).
7. Presence high shelf at 4.5 kHz, `(Pres−0.5)·16` dB. Then `clip(x, 0.05, 0.0)` as a mild power-amp stage. Shortcut: this saturation runs at the base rate and is kept mild; move it inside the oversampled block if aliasing shows up in Task 6.
8. Cabinet.
9. Master: `0.0` if Master == 0, else `db_to_lin(−40 + 40·Master + level_db)`.
10. `OnePole` HP at 10 Hz.
11. `safety_limit`.

Every knob and voicing field is a `Smoother`. Biquad coefficients are recomputed every 32 samples while any of their smoothers is unsettled. The 32-sample counter runs per sample, not per block, so output does not depend on block size.

- [ ] **Step 1: Failing tests** in `tests/engine.rs`. Run every test at sr 48000 unless stated.
  - `preset_switching_sets_all_params`: for each preset, after `set_preset`, `param(p) == preset.knobs()[p as usize]` for every param, and `preset()` matches.
  - `set_param_clamps_and_ignores_nan`: setting Drive to 2.0 reads 1.0. Setting NaN then reading gives the previous value.
  - `finite_and_bounded_full_scale`: for each preset, with all knobs at max and then all at min, feed 1 s of each of: a 220 Hz sine at amp 1.0, noise at rms 0.7 clamped to ±1, and a ±1 square. Every output sample is finite with `|y| <= 1.0`.
  - `idle_output_is_exact_zero`: for each preset with Gate forced to −96, feed a pluck at 0.5, then 3 s of zeros. The last 0.5 s is exactly `0.0` (Review Focus 4).
  - `no_dc_offset`: for each preset, plucks at peak 0.3 every 0.5 s for 3 s. `|mean(last 2 s)| < 1e-3`.
  - `chunking_invariance`: a 1 s pluck run as one block, and as blocks of 1, 37 and 128 samples, with a `set_param(Bass, 0.9)` at sample 10000 in each run. The outputs are identical within 1e-6.
  - `other_sample_rates`: at sr 44100 and 96000, `finite_and_bounded_full_scale` (at max knobs) and `idle_output_is_exact_zero` (distortion) still hold.
  - `mismatched_lengths`: input len 100 and output len 128 gives `output[100..]` all `0.0` and no panic.
  - `latency_report`: `DSP_LATENCY_SAMPLES <= 32`.
- [ ] **Step 2:** Run `cargo test --test engine`. Expected: FAIL.
- [ ] **Step 3:** Implement `engine.rs` per the chain. Re-export from `lib.rs`.
- [ ] **Step 4:** Run `cargo test`. Expected: all PASS.

---

### Task 6: Tone-character tests + offline renderer

**Files:** Create `dsp/tests/tone.rs`, `dsp/examples/render.rs`. Modify the `params.rs` voicing values and `engine.rs` constants only as needed to pass.

**Interfaces:**
- **Consumes:** `AmpEngine` and `tests/common`.
- **Produces:** the CLI `cargo run --release --example render -- <in.wav> [preset] [out_dir]`. It reads channel 0 of any 16- or 24-bit integer or float WAV with `hound`, runs `AmpEngine` at the file's sample rate in 128-sample blocks, and writes `<out_dir>/<preset>.wav` as mono 32-bit float. `out_dir` defaults to `renders/`. All four presets are rendered when `preset` is omitted. The tool prints the peak and RMS in dBFS for each output.

Set Gate to −96 in every test below except the gate tests.

- [ ] **Step 1: Failing tests** in `tests/tone.rs`.
  - `clean_has_headroom`: clean, 220 Hz sine at amp 0.126 (−18 dBFS): `thd < 0.01`.
  - `crunch_breaks_up_with_dynamics`: crunch, `thd` at amp 0.01 is < 0.01, and at amp 0.25 it is > 0.03.
  - `crunch_pluck_compresses`: crunch, `rms(out)/rms(in)` for a 110 Hz pluck at peak 0.5 is < 0.5× the same ratio at peak 0.02. Clean keeps the ratio ≥ 0.85× between peak 0.126 and 0.02.
  - `overdrive_louder_is_dirtier`: overdrive, `thd` at amp 0.25 is greater than `thd` at amp 0.02.
  - `distortion_tight_lows`: distortion, small signal (amp 1e-4, linear region). The gain at 60 Hz relative to 500 Hz is ≤ −6 dB, and lower than the same ratio for clean.
  - `no_audible_aliasing`: distortion and overdrive, a 1319 Hz sine at amp 0.25 over a 1 s window (integer Hz bins). The power not at harmonics k·1319 Hz (k ≥ 1, below Nyquist) is ≤ −40 dB relative to total power.
  - `param_change_click_free`: clean, 220 Hz sine at amp 0.1. Master jumps from 0.2 to 1.0 at a block boundary. `max_step` over 50 ms after the jump is ≤ 1.5× `max_step` of the steady state 100 ms later.
  - `preset_switch_click_free`: a distortion to clean switch mid-sine, at amp 0.1. `max_step` over 50 ms after the switch is ≤ 1.5× the larger of the steady-state `max_step` before and after.
  - `distortion_gate_silences_noise`: distortion with its default Gate, noise at −66 dBFS RMS (peaks stay below the −50 dB threshold). Output RMS over 0.5–1.5 s is ≤ −60 dBFS.
  - `distortion_gate_passes_note`: the same, plus a −20 dBFS 110 Hz pluck at 1 s. The output RMS of the first 100 ms of the note is ≥ −30 dBFS.
  - `di_fixture_safe`: `../guitar-clean-riff.wav` through each preset at default knobs gives finite output, `|y| <= 1.0`, and `|mean| < 1e-3`. If the file is missing, print a message and skip.
- [ ] **Step 2:** Run `cargo test --test tone`. Expected: some FAIL.
- [ ] **Step 3:** Adjust the voicing table and stage constants until they pass. Keep the starting table's shape: clean has the lowest gain and distortion the highest. Write `render.rs`.
- [ ] **Step 4:** Run `cargo test`. Expected: all PASS. Then run `cargo run --release --example render -- ../guitar-clean-riff.wav`. Expected: `renders/{clean,crunch,overdrive,distortion}.wav` exist, and the peaks printed are ≤ 0 dBFS.

---

### Task 7: WASM wrapper + web audio engine

**Files:** Create `dsp/src/wasm.rs`, the Vite scaffold in `web/` (`package.json`, `vite.config.ts`, `tsconfig*.json`, `index.html`, `src/main.tsx`), `web/src/audio/dsp.ts`, `web/src/audio/AmpAudio.ts`, `web/src/audio/amp-processor.ts`.

**Interfaces — Produces (Rust, `#[cfg(feature = "wasm")]`):**
```rust
pub const MAX_BLOCK: usize = 1024;
#[wasm_bindgen] pub struct WasmAmp { /* engine, input: [f32; MAX_BLOCK], output: [f32; MAX_BLOCK] */ }
#[wasm_bindgen] impl WasmAmp {
    #[wasm_bindgen(constructor)] pub fn new(sample_rate: f32) -> WasmAmp;
    pub fn set_preset(&mut self, preset: u32);               // invalid ignored
    pub fn set_param(&mut self, param: u32, value: f32);      // invalid ignored
    pub fn process(&mut self, input: &[f32], output: &mut [f32]);
    pub fn input_ptr(&mut self) -> *mut f32;
    pub fn output_ptr(&self) -> *const f32;
    pub fn process_block(&mut self, len: usize);             // input[..len] -> output[..len], len clamped to MAX_BLOCK
}
#[wasm_bindgen] pub fn preset_values(preset: u32) -> Vec<f32>; // empty if invalid
#[wasm_bindgen] pub fn param_min(param: u32) -> f32;           // NaN if invalid
#[wasm_bindgen] pub fn param_max(param: u32) -> f32;
#[wasm_bindgen] pub fn dsp_latency_samples() -> u32;
```

**Produces (TS):**
```ts
// dsp.ts
export interface Dsp { module: WebAssembly.Module; presetValues(p: number): number[];
  paramRange(i: number): [number, number]; latencySamples: number }
export function loadDsp(): Promise<Dsp>;          // cached; fetch amp_dsp_bg.wasm?url -> WebAssembly.compile -> init({ module_or_path: module })
// AmpAudio.ts
export const PRESETS = ['clean', 'crunch', 'overdrive', 'distortion'] as const;
export const PARAMS = ['Input', 'Drive', 'Bass', 'Mid', 'Treble', 'Presence', 'Master', 'Gate'] as const;
export interface LatencyReport { sampleRate: number; baseMs: number; outputMs: number;
  blockSamples: 128; blockMs: number; dspSamples: number; dspMs: number; totalMs: number }
export class AmpAudio {
  constructor(dsp: Dsp, onLevel: (peak: number) => void, onEnded: () => void);
  start(deviceId: string | undefined, preset: number, values: number[]): Promise<void>;
  stop(): Promise<void>;
  setPreset(p: number): void; setParam(i: number, v: number): void;
  latency(): LatencyReport | null; get running(): boolean;
}
```
Worklet protocol. Main → worklet: `{type:'preset', preset}` and `{type:'param', param, value}`. Worklet → main: `{type:'level', peak}`, every 16 render quanta. `processorOptions`: `{ module, preset, values }`. The processor runs `initSync({ module })`, then `new WasmAmp(sampleRate)`, `set_preset`, and `set_param` for each value. It builds `Float32Array` views on `memory.buffer` at the pointers, and rebuilds them if the buffer is detached. Each quantum: copy `inputs[0][0]` into the input view (zeros if there is no input channel), call `process_block(n)`, and copy the output view to **every** output channel. `totalMs = baseMs + outputMs + blockMs + dspMs`.

- [ ] **Step 1: Failing native test** in `wasm.rs`: `wasm_matches_engine`. Feed a pluck through `WasmAmp` (copy into `input_ptr`, `process_block(128)`) and through `AmpEngine`. Results are identical. `preset_values(3)` equals `Preset::Distortion.knobs()`, `preset_values(9).is_empty()`, and `param_min(7) == -96.0`. Run `cargo test --features wasm wasm_`. Expected: FAIL, then implement, then PASS.
- [ ] **Step 2: Scaffold.** `npm create vite@latest web -- --template react-ts`. Delete the demo assets and markup. In `package.json` scripts: `"wasm": "wasm-pack build ../dsp --target web --release --out-dir pkg -- --features wasm"`, `"dev": "npm run wasm && vite"`, `"build": "npm run wasm && tsc -b && vite build"`. Run `npm run wasm`, then `npm i ../dsp/pkg` (the dependency becomes `"amp-dsp": "file:../dsp/pkg"`). In `vite.config.ts`: `worker: { format: 'es' }`, `optimizeDeps: { exclude: ['amp-dsp'] }`, `server: { fs: { allow: ['..'] } }`.
- [ ] **Step 3:** Implement `dsp.ts`, `amp-processor.ts` (load it with `import processorUrl from './amp-processor.ts?worker&url'`, and declare `AudioWorkletProcessor`, `registerProcessor` and `sampleRate` in the file) and `AmpAudio.ts`. Check the generated `dsp/pkg/amp_dsp.js`: if it calls `TextDecoder` or `TextEncoder` at module top level without a `typeof` guard, add a minimal polyfill at the top of the processor before the import.
- [ ] **Step 4: `AmpAudio.start` specifics.** Call `getUserMedia({ audio: { deviceId: deviceId ? { exact: deviceId } : undefined, echoCancellation: false, noiseSuppression: false, autoGainControl: false } })`. Create `new AudioContext({ latencyHint: 'interactive', sampleRate: track.getSettings().sampleRate })`, leaving out `sampleRate` if it's undefined (Review Focus 1). Then `addModule`, build the node with `outputChannelCount: [2]` and default channel mode, connect source → node → `ctx.destination`, call `ctx.resume()`, and listen for track `ended` → `onEnded`. Wrap it all in try/catch: on any failure, stop the tracks, close the context, reset state, and rethrow (Review Focus 2). `stop()` disconnects, stops all tracks, and awaits `ctx.close()`.
- [ ] **Step 5: Verify.** Run `npm run build`. Expected: success, and `dist/` contains the worklet chunk and the `.wasm`. Temporarily make `App.tsx` a single button that calls `loadDsp()` and then `new AmpAudio(...).start(undefined, 3, presetValues(3))`. Run `npm run dev`, open it in Chrome, and confirm there are no console errors and the worklet posts level messages (log them).

---

### Task 8: GUI (amp head) + README

**Files:** Create `web/src/audio/useMicAccess.ts`, `web/src/components/{TopBar,Knob,AmpFace,LevelMeter}.tsx`. Modify `web/src/App.tsx` and `web/src/App.css`. Create `README.md` at the repo root.

**Interfaces:**
- **Consumes:** `loadDsp`, `AmpAudio`, `PRESETS`, `PARAMS`, `LatencyReport`.
- **Produces:**
```ts
export type MicStatus = 'pending' | 'granted' | 'denied' | 'unavailable';
export function useMicAccess(): { status: MicStatus; message?: string; devices: MediaDeviceInfo[]; retry(): void };
export function Knob(p: { label: string; value: number; min: number; max: number; resetValue: number;
  format(v: number): string; disabled?: boolean; onChange(v: number): void }): JSX.Element;
```
The design reference is card A in `.superpowers/brainstorm/*/content/layout.html`: a dark panel with gold/tan trim, red for the active preset and the Start button, and 8 round knobs in a row (wrapping on narrow screens).

- [ ] **Step 1: `useMicAccess`.** On mount, call `getUserMedia({audio:true})`, then stop its tracks immediately. Status goes to granted and devices = `enumerateDevices()` filtered to `audioinput`. `NotAllowedError`/`SecurityError` → `denied`. `NotFoundError`, missing `navigator.mediaDevices`, or an insecure context → `unavailable`, with the message "No audio input found. Connect your interface and press Retry." (or "Open this page on localhost or HTTPS." for the insecure case). Listen for `devicechange` and refresh the list.
- [ ] **Step 2: `Knob`.** `role="slider"` with `aria-label`, `aria-valuemin`, `aria-valuemax`, `aria-valuenow` and `aria-valuetext={format(value)}`. Vertical pointer drag with pointer capture: 200 px = the full range, Shift = 10× finer. Arrow keys ±1% of range, PageUp/PageDown ±10%, Home/End. Double-click → `resetValue`. Rotation −135°..+135°. Format: Gate shows `off` at −96, otherwise `-50 dB`; other knobs show `0.0`–`10.0`.
- [ ] **Step 3: Components.** `TopBar`: the status dot and text. When denied, show: "Microphone access is blocked. Click the icon left of the address bar, set Microphone to Allow, then reload the page." Plus the input `<select>`, and the latency text `Latency 11.3 ms (out 8.6 + block 2.7 ms / 128 smp + DSP 0.3 ms)`, shown only while running. `AmpFace`: preset buttons and knobs. `LevelMeter`: a horizontal bar with peak to dB to width over −60..0 dB, and red above −3 dB.
- [ ] **Step 4: `App` state.** `preset` (initially 3), `values` (from `presetValues(3)`), `deviceId` (by default the first device whose label includes "Scarlett", otherwise `undefined` = the system default; my call), `running`, `error`, `latency` (polled once a second while running), and `level`. Choosing a preset sets `values = presetValues(p)` and calls `amp.setPreset(p)`. A knob change sets that value and calls `amp.setParam(i, v)`. A knob's `resetValue` is the current preset's value. Start/Stop and the knobs are disabled unless status is `granted`. Errors from `start` appear in the top bar.
- [ ] **Step 5: Lifecycle guards** (Review Focus 3 and 5). Hold a `busy` flag: Start/Stop are disabled while a start or stop is in progress. Changing the device while running awaits `stop()` and then `start(newId, preset, values)`. On `onEnded`, await `stop()` and show "Input device disconnected." On unmount, call `amp.stop()`.
- [ ] **Step 6: README.md.** Cover: headphones warning (feedback); prerequisites (rustup toolchain PATH export, `brew install wasm-pack`, Node); `cd dsp && cargo test`; `cd web && npm install && npm run dev`, then open http://localhost:5173; pick the Scarlett in the input picker; latency notes; `cargo run --release --example render -- ../guitar-clean-riff.wav`.
- [ ] **Step 7: Verify.** Run `npm run build`. Expected: success with no TS errors. Open the dev server in a headless browser (Playwright) with no mic permission granted. Expected: the denied or unavailable message renders, Start is disabled, and there are no uncaught errors. Then in real Chrome: the permission prompt appears on load; after granting it the device list shows labels; Start makes the level meter move and shows the latency text; Stop turns off the recording indicator; double-clicking Start quickly creates only one context (check `chrome://media-internals` or the console).

---

### Task 9: Tune with the user's DI + acceptance

**Files:** Modify only the `dsp/src/params.rs` tables and `engine.rs` constants.

- [ ] **Step 1:** Run `cargo run --release --example render -- ../guitar-clean-riff.wav`. Report each preset's peak/RMS, plus what you hear in the numbers: compare spectral balance against the Task 6 targets.
- [ ] **Step 2: User checkpoint.** Give the user the four `dsp/renders/*.wav` paths and ask: is `distortion` tight on the palm mutes, free of fizz, with clean gaps between riffs? Does `clean` stay clean, does `crunch` break up only on hard hits, and does `overdrive` sustain? Wait for the answers.
- [ ] **Step 3:** Change only the tables and constants based on the feedback. After each change, run `cargo test` (must stay green) and re-render. Repeat Step 2 until the user approves.
- [ ] **Step 4:** Run `npm run build` in `web/` to pick up the new WASM. Then run the PROMPT.md acceptance checklist with the user playing live (mic prompt, four presets differ, clicks/dropouts, denied path, Stop releases the mic, latency reported under ~20 ms total, soft playing clean / hard playing breaks up). Record the latency they measure in README.
