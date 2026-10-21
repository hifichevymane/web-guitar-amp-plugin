//! Biquad (RBJ cookbook, TDF-II), one-pole filters, and a linear-ramp parameter smoother.
//! Plain data, no heap. State below 1e-20 is flushed to zero so idle signals reach exact 0.0.

use std::f64::consts::PI;

const FLUSH: f32 = 1e-20;

#[inline]
fn flush(s: f32) -> f32 {
    if s.abs() < FLUSH {
        0.0
    } else {
        s
    }
}

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new()
    }
}

impl Biquad {
    pub fn new() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }

    /// Stores coefficients normalised by a0. Filter state is kept so retuning is click-free.
    fn set(&mut self, b: [f64; 3], a: [f64; 3]) {
        self.b0 = (b[0] / a[0]) as f32;
        self.b1 = (b[1] / a[0]) as f32;
        self.b2 = (b[2] / a[0]) as f32;
        self.a1 = (a[1] / a[0]) as f32;
        self.a2 = (a[2] / a[0]) as f32;
    }

    fn w0(sr: f32, hz: f32) -> (f64, f64) {
        let w = 2.0 * PI * hz as f64 / sr as f64;
        (w.cos(), w.sin())
    }

    pub fn set_highpass(&mut self, sr: f32, hz: f32, q: f32) {
        let (c, s) = Self::w0(sr, hz);
        let al = s / (2.0 * q as f64);
        self.set([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], [1.0 + al, -2.0 * c, 1.0 - al]);
    }

    pub fn set_lowpass(&mut self, sr: f32, hz: f32, q: f32) {
        let (c, s) = Self::w0(sr, hz);
        let al = s / (2.0 * q as f64);
        self.set([(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0], [1.0 + al, -2.0 * c, 1.0 - al]);
    }

    pub fn set_peaking(&mut self, sr: f32, hz: f32, q: f32, db: f32) {
        let (c, s) = Self::w0(sr, hz);
        let a = 10f64.powf(db as f64 / 40.0);
        let al = s / (2.0 * q as f64);
        self.set([1.0 + al * a, -2.0 * c, 1.0 - al * a], [1.0 + al / a, -2.0 * c, 1.0 - al / a]);
    }

    // Shelf slope S = 1.
    pub fn set_low_shelf(&mut self, sr: f32, hz: f32, db: f32) {
        let (c, s) = Self::w0(sr, hz);
        let a = 10f64.powf(db as f64 / 40.0);
        let t = 2.0 * a.sqrt() * s / 2.0 * 2f64.sqrt();
        self.set(
            [a * ((a + 1.0) - (a - 1.0) * c + t), 2.0 * a * ((a - 1.0) - (a + 1.0) * c), a * ((a + 1.0) - (a - 1.0) * c - t)],
            [(a + 1.0) + (a - 1.0) * c + t, -2.0 * ((a - 1.0) + (a + 1.0) * c), (a + 1.0) + (a - 1.0) * c - t],
        );
    }

    pub fn set_high_shelf(&mut self, sr: f32, hz: f32, db: f32) {
        let (c, s) = Self::w0(sr, hz);
        let a = 10f64.powf(db as f64 / 40.0);
        let t = 2.0 * a.sqrt() * s / 2.0 * 2f64.sqrt();
        self.set(
            [a * ((a + 1.0) + (a - 1.0) * c + t), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - t)],
            [(a + 1.0) - (a - 1.0) * c + t, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - t],
        );
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        let z1 = self.b1 * x - self.a1 * y + self.z2;
        let z2 = self.b2 * x - self.a2 * y;
        // Flush both states together: zeroing only one injects error that can sustain a limit cycle.
        (self.z1, self.z2) = if z1.abs() < FLUSH && z2.abs() < FLUSH { (0.0, 0.0) } else { (z1, z2) };
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OnePole {
    a: f32,
    z: f32,
    high: bool,
}

impl OnePole {
    fn new(sr: f32, hz: f32, high: bool) -> Self {
        let a = 1.0 - (-2.0 * PI * hz as f64 / sr as f64).exp();
        Self { a: a as f32, z: 0.0, high }
    }

    pub fn lowpass(sr: f32, hz: f32) -> Self {
        Self::new(sr, hz, false)
    }

    pub fn highpass(sr: f32, hz: f32) -> Self {
        Self::new(sr, hz, true)
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.z = flush(self.z + self.a * (x - self.z));
        if self.high {
            x - self.z
        } else {
            self.z
        }
    }

    pub fn reset(&mut self) {
        self.z = 0.0;
    }
}

/// Linear ramp to the target over 15 ms.
#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    cur: f32,
    target: f32,
    step: f32,
    remaining: u32,
    ramp_samples: u32,
}

impl Smoother {
    pub fn new(sr: f32, initial: f32) -> Self {
        Self {
            cur: initial,
            target: initial,
            step: 0.0,
            remaining: 0,
            ramp_samples: ((0.015 * sr).round() as u32).max(1),
        }
    }

    pub fn set_target(&mut self, v: f32) {
        self.target = v;
        self.step = (v - self.cur) / self.ramp_samples as f32;
        self.remaining = self.ramp_samples;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.cur = if self.remaining == 0 { self.target } else { self.cur + self.step };
        }
        self.cur
    }

    pub fn value(&self) -> f32 {
        self.cur
    }

    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }
}

#[cfg(test)]
/// Steady-state gain of `f` at `hz`, measured over the second half of one second of sine.
pub(crate) fn gain_db(mut f: impl FnMut(f32) -> f32, sr: f32, hz: f32) -> f32 {
    let n = sr as usize;
    let (mut si, mut so) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = (2.0 * PI * hz as f64 * i as f64 / sr as f64).sin() as f32;
        let y = f(x);
        if i >= n / 2 {
            si += (x * x) as f64;
            so += (y * y) as f64;
        }
    }
    (10.0 * (so / si).log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biquad_lowpass() {
        let mut b = Biquad::new();
        b.set_lowpass(48000.0, 1000.0, 0.707);
        assert!(gain_db(|x| b.process(x), 48000.0, 100.0).abs() < 0.5);
        b.reset();
        assert!(gain_db(|x| b.process(x), 48000.0, 10000.0) <= -35.0);
    }

    #[test]
    fn biquad_peaking() {
        let mut b = Biquad::new();
        b.set_peaking(48000.0, 1000.0, 1.0, 6.0);
        assert!((gain_db(|x| b.process(x), 48000.0, 1000.0) - 6.0).abs() < 0.3);
        b.reset();
        assert!(gain_db(|x| b.process(x), 48000.0, 50.0).abs() < 0.5);
    }

    #[test]
    fn biquad_flushes_to_zero() {
        let mut b = Biquad::new();
        b.set_lowpass(48000.0, 1000.0, 0.707);
        let mut y = b.process(1.0);
        for _ in 0..48000 {
            y = b.process(0.0);
        }
        assert_eq!(y, 0.0);
    }

    #[test]
    fn smoother_ramp() {
        let mut s = Smoother::new(48000.0, 0.0);
        assert!(s.is_settled());
        s.set_target(1.0);
        let mut prev = 0.0;
        for i in 1..=720 {
            let v = s.next();
            assert!(v >= prev && v <= 1.0);
            prev = v;
            if i == 360 {
                assert!((v - 0.5).abs() < 0.01);
            }
        }
        assert_eq!(s.value(), 1.0);
        assert!(s.is_settled());
    }

    #[test]
    fn one_pole_and_db() {
        let mut lp = OnePole::lowpass(48000.0, 1000.0);
        assert!(gain_db(|x| lp.process(x), 48000.0, 50.0).abs() < 0.5);
        let mut hp = OnePole::highpass(48000.0, 1000.0);
        assert!(gain_db(|x| hp.process(x), 48000.0, 50.0) < -20.0);
        assert!((db_to_lin(-6.0206) - 0.5).abs() < 1e-3);
    }
}
