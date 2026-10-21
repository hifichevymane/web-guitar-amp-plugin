//! 4x oversampler: two cascaded 2x Kaiser-windowed halfband FIR stages (48 -> 96 -> 192 kHz).
//! Halfband: every tap is zero except the centre and the even-indexed ones, so only those are
//! evaluated. Round-trip delay is `LATENCY_SAMPLES` base-rate samples.

use std::f64::consts::PI;

pub const OVERSAMPLE: usize = 4;

const N1: usize = 47; // stage 1 (48k -> 96k), sharp transition
const N2: usize = 19; // stage 2 (96k -> 192k), relaxed transition
const KAISER_BETA: f64 = 8.0;

// Round trip in 192 kHz samples is 4*c1 + 2*c2 with centres c = (N-1)/2; down phases are chosen
// so that this is a whole number of base samples: c1 + (c2-1)/2.
pub const LATENCY_SAMPLES: usize = (N1 - 1) / 2 + (N2 - 3) / 4;

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term) = (1.0, 1.0);
    for k in 1..50 {
        term *= (x / (2.0 * k as f64)).powi(2);
        sum += term;
    }
    sum
}

/// Halfband lowpass (cutoff fs/4), DC gain 1, centre tap 0.5.
fn design<const N: usize>() -> [f32; N] {
    let c = (N - 1) / 2;
    let mut h = [0.0f64; N];
    let mut sum = 0.0;
    for (k, hk) in h.iter_mut().enumerate() {
        let n = k as f64 - c as f64;
        let ideal = if k == c {
            0.5
        } else if (k as isize - c as isize) % 2 == 0 {
            0.0
        } else {
            (PI * n / 2.0).sin() / (PI * n)
        };
        let r = n / c as f64;
        let w = bessel_i0(KAISER_BETA * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(KAISER_BETA);
        *hk = ideal * w;
        sum += *hk;
    }
    let mut out = [0.0f32; N];
    for k in 0..N {
        out[k] = (h[k] / sum) as f32;
    }
    out
}

#[derive(Clone, Copy, Debug)]
struct HalfbandUp<const N: usize> {
    h: [f32; N], // gain 2
    d: [f32; N], // d[0] newest input
}

impl<const N: usize> HalfbandUp<N> {
    fn new() -> Self {
        let mut h = design::<N>();
        h.iter_mut().for_each(|v| *v *= 2.0);
        Self { h, d: [0.0; N] }
    }

    /// Returns the two output samples for one input, in time order.
    #[inline]
    fn process(&mut self, x: f32) -> [f32; 2] {
        self.d.copy_within(0..N - 1, 1);
        self.d[0] = x;
        let mut even = 0.0;
        for i in 0..=(N - 1) / 2 {
            even += self.h[2 * i] * self.d[i];
        }
        // The odd phase has only the (unity) centre tap: a pure delay.
        [even, self.d[(N - 3) / 4]]
    }
}

#[derive(Clone, Copy, Debug)]
struct HalfbandDown<const N: usize> {
    h: [f32; N],
    v: [f32; N], // v[0] newest input
}

impl<const N: usize> HalfbandDown<N> {
    fn new() -> Self {
        Self { h: design::<N>(), v: [0.0; N] }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.v.copy_within(0..N - 1, 1);
        self.v[0] = x;
    }

    #[inline]
    fn conv(&self) -> f32 {
        let c = (N - 1) / 2; // odd, so not among the even taps
        let mut y = self.h[c] * self.v[c];
        for k in (0..N).step_by(2) {
            y += self.h[k] * self.v[k];
        }
        y
    }

    /// `phase` 0 keeps the filter output aligned with `a`, 1 with `b`.
    #[inline]
    fn process(&mut self, a: f32, b: f32, phase: usize) -> f32 {
        self.push(a);
        if phase == 0 {
            let y = self.conv();
            self.push(b);
            y
        } else {
            self.push(b);
            self.conv()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Oversampler4x {
    up1: HalfbandUp<N1>,
    up2: HalfbandUp<N2>,
    down2: HalfbandDown<N2>,
    down1: HalfbandDown<N1>,
}

impl Default for Oversampler4x {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler4x {
    pub fn new() -> Self {
        Self { up1: HalfbandUp::new(), up2: HalfbandUp::new(), down2: HalfbandDown::new(), down1: HalfbandDown::new() }
    }

    pub fn upsample(&mut self, x: f32) -> [f32; 4] {
        let [e, o] = self.up1.process(x);
        let [a, b] = self.up2.process(e);
        let [c, d] = self.up2.process(o);
        [a, b, c, d]
    }

    pub fn downsample(&mut self, xs: [f32; 4]) -> f32 {
        let j0 = self.down2.process(xs[0], xs[1], 0);
        let j1 = self.down2.process(xs[2], xs[3], 0);
        self.down1.process(j0, j1, 1)
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_small() {
        assert!(LATENCY_SAMPLES <= 32);
    }

    #[test]
    fn oversampler_roundtrip() {
        let mut o = Oversampler4x::new();
        let x: Vec<f32> = (0..4800).map(|i| (0.5 * (2.0 * PI * 1000.0 * i as f64 / 48000.0).sin()) as f32).collect();
        let y: Vec<f32> = x.iter().map(|&s| {
            let u = o.upsample(s);
            o.downsample(u)
        }).collect();
        let (mut err, mut n) = (0.0f64, 0);
        for i in 1000..x.len() {
            let e = (y[i] - x[i - LATENCY_SAMPLES]) as f64;
            err += e * e;
            n += 1;
        }
        assert!((err / n as f64).sqrt() < 1e-3);
    }

    #[test]
    fn oversampler_image_rejection() {
        let mut o = Oversampler4x::new();
        let mut s = Vec::new();
        for i in 0..48000 * 2 {
            let x = (2.0 * PI * 10000.0 * i as f64 / 48000.0).sin() as f32;
            s.extend_from_slice(&o.upsample(x));
        }
        let s = &s[s.len() - 96000..];
        let goertzel = |hz: f64| {
            let w = 2.0 * PI * hz / 192000.0;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &v) in s.iter().enumerate() {
                re += v as f64 * (w * i as f64).cos();
                im += v as f64 * (w * i as f64).sin();
            }
            re * re + im * im
        };
        let rel = 10.0 * (goertzel(38000.0) / goertzel(10000.0)).log10();
        assert!(rel <= -60.0, "image at {rel} dB");
    }
}
