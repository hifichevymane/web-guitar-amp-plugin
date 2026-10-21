//! AmpEngine: the full mono amp chain. Fixed-size state only; `process` never allocates.
use crate::cab::Cabinet;
use crate::filters::{db_to_lin, Biquad, OnePole, Smoother};
use crate::gate::NoiseGate;
use crate::oversample::{self, Oversampler4x, OVERSAMPLE};
use crate::params::{Param, Preset, Voicing, PARAM_COUNT};
use crate::shaper::{clip, safety_limit, shape};
use crate::tonestack::ToneStack;

pub const DSP_LATENCY_SAMPLES: usize = oversample::LATENCY_SAMPLES;

const STAGES: usize = 3;
const COEF_INTERVAL: u32 = 32;

// Voicing smoother indices.
const TIGHT_HZ: usize = 0;
const DRIVE_MIN: usize = 1;
const DRIVE_MAX: usize = 2;
const BIAS: usize = 3;
const HARDNESS: usize = 4;
const MID_HZ: usize = 5;
const LEVEL: usize = 6;

fn voicing_array(v: Voicing) -> [f32; 7] {
    [v.tight_hz, v.drive_min_db, v.drive_max_db, v.bias, v.hardness, v.mid_hz, v.level_db]
}

pub struct AmpEngine {
    sr: f32,
    preset: Preset,
    targets: [f32; PARAM_COUNT],
    knobs: [Smoother; PARAM_COUNT],
    voicing: [Smoother; 7],
    // Set on any target change; cleared once every smoother has settled and been applied.
    coefs_dirty: bool,
    scalars_dirty: bool,
    coef_counter: u32,
    // Cached per-sample scalars.
    in_gain: f32,
    drive_gain: f32,
    master_gain: f32,

    input_hp: OnePole,
    gate: NoiseGate,
    tight: Biquad,
    os: Oversampler4x,
    stage_lp: [OnePole; STAGES],
    stage_hp: [OnePole; STAGES],
    tone: ToneStack,
    presence: Biquad,
    cab: Cabinet,
    dc_block: OnePole,
}

impl AmpEngine {
    /// Starts on `Preset::Distortion`.
    pub fn new(sample_rate: f32) -> Self {
        Self::build(sample_rate, Preset::Distortion, Preset::Distortion.knobs())
    }

    /// Fresh state with all smoothers sitting at `knobs` / `preset.voicing()` (no ramp).
    fn build(sr: f32, preset: Preset, knobs: [f32; PARAM_COUNT]) -> Self {
        let osr = sr * OVERSAMPLE as f32;
        let mut e = Self {
            sr,
            preset,
            targets: knobs,
            knobs: knobs.map(|v| Smoother::new(sr, v)),
            voicing: voicing_array(preset.voicing()).map(|v| Smoother::new(sr, v)),
            coefs_dirty: false,
            scalars_dirty: false,
            coef_counter: 0,
            in_gain: 0.0,
            drive_gain: 0.0,
            master_gain: 0.0,
            input_hp: OnePole::highpass(sr, 20.0),
            gate: NoiseGate::new(sr),
            tight: Biquad::new(),
            os: Oversampler4x::new(),
            stage_lp: [OnePole::lowpass(osr, 7000.0); STAGES],
            stage_hp: [OnePole::highpass(osr, 30.0); STAGES],
            tone: ToneStack::new(sr),
            presence: Biquad::new(),
            cab: Cabinet::new(sr),
            dc_block: OnePole::highpass(sr, 10.0),
        };
        e.update_coefs();
        e.update_scalars();
        e
    }

    /// Knobs <- preset.knobs(), voicing <- preset.voicing(); everything ramps.
    pub fn set_preset(&mut self, preset: Preset) {
        self.preset = preset;
        self.targets = preset.knobs();
        for (s, v) in self.knobs.iter_mut().zip(self.targets) {
            s.set_target(v);
        }
        for (s, v) in self.voicing.iter_mut().zip(voicing_array(preset.voicing())) {
            s.set_target(v);
        }
        self.coefs_dirty = true;
        self.scalars_dirty = true;
    }

    /// Clamped to the param's range; NaN/Inf are ignored.
    pub fn set_param(&mut self, param: Param, value: f32) {
        if let Some(v) = param.clamp(value) {
            self.targets[param as usize] = v;
            self.knobs[param as usize].set_target(v);
            self.coefs_dirty = true;
            self.scalars_dirty = true;
        }
    }

    /// Current target (not the ramping value).
    pub fn param(&self, param: Param) -> f32 {
        self.targets[param as usize]
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// Clears all filter/gate/oversampler state; params jump to their targets.
    pub fn reset(&mut self) {
        *self = Self::build(self.sr, self.preset, self.targets);
    }

    fn k(&self, p: Param) -> f32 {
        self.knobs[p as usize].value()
    }

    fn v(&self, i: usize) -> f32 {
        self.voicing[i].value()
    }

    fn settled(&self) -> bool {
        self.knobs.iter().chain(&self.voicing).all(Smoother::is_settled)
    }

    fn update_coefs(&mut self) {
        self.tight.set_highpass(self.sr, self.v(TIGHT_HZ), 0.7);
        self.tone.set(self.k(Param::Bass), self.k(Param::Mid), self.k(Param::Treble), self.v(MID_HZ));
        self.presence.set_high_shelf(self.sr, 4500.0, (self.k(Param::Presence) - 0.5) * 16.0);
    }

    fn update_scalars(&mut self) {
        self.in_gain = db_to_lin((self.k(Param::InputGain) - 0.5) * 24.0);
        let drive = self.k(Param::Drive);
        let (lo, hi) = (self.v(DRIVE_MIN), self.v(DRIVE_MAX));
        self.drive_gain = db_to_lin((lo + (hi - lo) * drive) / STAGES as f32);
        let master = self.k(Param::Master);
        // The linear fade below 0.05 takes the -40 dB floor continuously down to exactly 0.
        self.master_gain = if master == 0.0 {
            0.0
        } else {
            db_to_lin(-40.0 + 40.0 * master + self.v(LEVEL)) * (master / 0.05).min(1.0)
        };
        // A ramp from gate-off enters the gate open, then it releases smoothly (see NoiseGate).
        self.gate.set_threshold_db(self.k(Param::Gate));
    }

    /// Processes `n = min(input.len(), output.len())` samples; `output[n..]` is zeroed.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let n = input.len().min(output.len());
        for (x, y) in input.iter().zip(output.iter_mut()) {
            *y = self.tick(*x);
        }
        output[n..].fill(0.0);
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        // One NaN/Inf would otherwise poison every filter state for good.
        let x = if x.is_finite() { x } else { 0.0 };
        for s in self.knobs.iter_mut().chain(self.voicing.iter_mut()) {
            s.next();
        }
        if self.scalars_dirty {
            self.update_scalars();
            self.scalars_dirty = !self.settled();
        }
        // Counter runs per sample so coefficient timing is independent of block size.
        self.coef_counter += 1;
        if self.coef_counter >= COEF_INTERVAL {
            self.coef_counter = 0;
            if self.coefs_dirty {
                self.update_coefs();
                self.coefs_dirty = !self.settled();
            }
        }

        let x = self.input_hp.process(x);
        let x = self.gate.process(x);
        let x = self.tight.process(x * self.in_gain);

        let (g, bias, hard) = (self.drive_gain, self.v(BIAS), self.v(HARDNESS));
        let offset = shape(bias, hard); // clip()'s DC term, hoisted out of the 12 inner calls
        let mut up = self.os.upsample(x);
        for s in up.iter_mut() {
            for i in 0..STAGES {
                let c = shape(*s * g + bias, hard) - offset;
                *s = self.stage_hp[i].process(self.stage_lp[i].process(c));
            }
        }
        let x = self.os.downsample(up);

        let x = self.tone.process(x);
        let x = clip(self.presence.process(x), 0.05, 0.0);
        let x = self.cab.process(x) * self.master_gain;
        safety_limit(self.dc_block.process(x))
    }
}
