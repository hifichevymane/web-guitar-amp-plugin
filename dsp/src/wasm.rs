//! wasm-bindgen wrapper. The worklet writes into `input_ptr`, calls `process_block`,
//! and reads `output_ptr`, so the audio path never copies through JS-allocated arrays.
use wasm_bindgen::prelude::*;

use crate::{AmpEngine, Param, Preset, DSP_LATENCY_SAMPLES};

pub const MAX_BLOCK: usize = 1024;

#[wasm_bindgen]
pub struct WasmAmp {
    engine: AmpEngine,
    input: [f32; MAX_BLOCK],
    output: [f32; MAX_BLOCK],
}

#[wasm_bindgen]
impl WasmAmp {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> WasmAmp {
        WasmAmp { engine: AmpEngine::new(sample_rate), input: [0.0; MAX_BLOCK], output: [0.0; MAX_BLOCK] }
    }

    /// Invalid index ignored.
    pub fn set_preset(&mut self, preset: u32) {
        if let Some(p) = Preset::from_index(preset) {
            self.engine.set_preset(p);
        }
    }

    /// Invalid index ignored.
    pub fn set_param(&mut self, param: u32, value: f32) {
        if let Some(p) = Param::from_index(param) {
            self.engine.set_param(p, value);
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        self.engine.process(input, output);
    }

    pub fn input_ptr(&mut self) -> *mut f32 {
        self.input.as_mut_ptr()
    }

    pub fn output_ptr(&self) -> *const f32 {
        self.output.as_ptr()
    }

    /// input[..len] -> output[..len]; `len` clamped to MAX_BLOCK.
    pub fn process_block(&mut self, len: usize) {
        let n = len.min(MAX_BLOCK);
        self.engine.process(&self.input[..n], &mut self.output[..n]);
    }
}

/// Knob values for a preset, indexed like `Param`; empty if invalid.
#[wasm_bindgen]
pub fn preset_values(preset: u32) -> Vec<f32> {
    Preset::from_index(preset).map_or(Vec::new(), |p| p.knobs().to_vec())
}

/// NaN if invalid.
#[wasm_bindgen]
pub fn param_min(param: u32) -> f32 {
    Param::from_index(param).map_or(f32::NAN, |p| p.range().0)
}

/// NaN if invalid.
#[wasm_bindgen]
pub fn param_max(param: u32) -> f32 {
    Param::from_index(param).map_or(f32::NAN, |p| p.range().1)
}

#[wasm_bindgen]
pub fn dsp_latency_samples() -> u32 {
    DSP_LATENCY_SAMPLES as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pluck(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                0.13 * (-3.0 * t).exp() * (2.0 * std::f32::consts::PI * 110.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn wasm_matches_engine() {
        let x = pluck(128 * 40);
        let mut w = WasmAmp::new(48_000.0);
        let mut e = AmpEngine::new(48_000.0);
        let mut want = vec![0.0; 128];
        for block in x.chunks(128) {
            unsafe { std::slice::from_raw_parts_mut(w.input_ptr(), 128) }.copy_from_slice(block);
            w.process_block(128);
            let got = unsafe { std::slice::from_raw_parts(w.output_ptr(), 128) };
            e.process(block, &mut want);
            assert_eq!(got, &want[..]);
        }
        assert_eq!(preset_values(3), Preset::Distortion.knobs().to_vec());
        assert!(preset_values(9).is_empty());
        assert_eq!(param_min(7), -96.0);
    }
}
