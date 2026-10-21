//! Signal generators and measurement helpers shared by the integration tests.
#![allow(dead_code)] // each test crate uses a different subset

use amp_dsp::AmpEngine;
use std::f64::consts::PI;

pub fn sine(sr: f32, hz: f32, amp: f32, secs: f32) -> Vec<f32> {
    let n = (sr * secs) as usize;
    (0..n).map(|i| amp * (2.0 * PI * hz as f64 * i as f64 / sr as f64).sin() as f32).collect()
}

/// Harmonics 1..10 at amplitude 1/h, each decaying as exp(-t*(1.5+0.6h)), normalized to `peak`.
pub fn pluck(sr: f32, hz: f32, peak: f32, secs: f32) -> Vec<f32> {
    let n = (sr * secs) as usize;
    let mut x: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / sr as f64;
            (1..=10)
                .map(|h| {
                    let h = h as f64;
                    (-t * (1.5 + 0.6 * h)).exp() / h * (2.0 * PI * h * hz as f64 * t).sin()
                })
                .sum()
        })
        .collect();
    let m = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    x.iter_mut().for_each(|v| *v *= peak as f64 / m);
    x.into_iter().map(|v| v as f32).collect()
}

/// Deterministic white noise (LCG, uniform) scaled to `rms`.
pub fn noise(seed: u32, rms: f32, n: usize) -> Vec<f32> {
    let mut s = seed;
    let k = rms * 3f32.sqrt(); // uniform [-1,1) has RMS 1/sqrt(3)
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 23) as f32 - 1.0) * k
        })
        .collect()
}

/// Processes `x` in 128-sample blocks.
pub fn run(e: &mut AmpEngine, x: &[f32]) -> Vec<f32> {
    let mut y = vec![0.0; x.len()];
    for (i, o) in x.chunks(128).zip(y.chunks_mut(128)) {
        e.process(i, o);
    }
    y
}

pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32
}

pub fn db(x: f32) -> f32 {
    20.0 * x.max(1e-30).log10()
}

/// Mean-square power of the `hz` component (a sine of amplitude A gives A^2/2).
pub fn goertzel_power(x: &[f32], hz: f32, sr: f32) -> f32 {
    let w = 2.0 * PI * hz as f64 / sr as f64;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s = v as f64 + c * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let mag2 = s1 * s1 + s2 * s2 - c * s1 * s2;
    let n = x.len() as f64;
    (2.0 * mag2 / (n * n)) as f32
}

/// sqrt((P_total_dc_removed - P_f0) / P_f0) over the longest integer-cycle prefix of `x`.
pub fn thd(x: &[f32], f0: f32, sr: f32) -> f32 {
    let cycles = (x.len() as f64 * f0 as f64 / sr as f64).floor();
    let n = (cycles * sr as f64 / f0 as f64).round() as usize;
    let x = &x[..n.min(x.len())];
    let mean = x.iter().map(|v| *v as f64).sum::<f64>() / x.len() as f64;
    let total = x.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / x.len() as f64;
    let p0 = goertzel_power(x, f0, sr) as f64;
    ((total - p0).max(0.0) / p0).sqrt() as f32
}

/// max |x[n] - x[n-1]|
pub fn max_step(x: &[f32]) -> f32 {
    x.windows(2).fold(0.0, |m, w| m.max((w[1] - w[0]).abs()))
}
