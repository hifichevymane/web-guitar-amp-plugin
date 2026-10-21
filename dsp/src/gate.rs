//! Noise gate with hysteresis and hold; gain reaches exactly 0.0 when closed.
use crate::filters::db_to_lin;
use crate::params::GATE_OFF_DB;

pub struct NoiseGate {
    bypass: bool,
    open_thr: f32,
    close_thr: f32,
    env: f32,
    env_rel: f32,
    gain: f32,
    gain_up: f32,
    gain_down: f32,
    hold: u32,
    hold_len: u32,
    open: bool,
}

impl NoiseGate {
    pub fn new(sr: f32) -> Self {
        let tc = |s: f32| (-1.0 / (s * sr)).exp();
        Self {
            bypass: true,
            open_thr: 0.0,
            close_thr: 0.0,
            env: 0.0,
            env_rel: tc(0.050),
            gain: 0.0,
            gain_up: 1.0 / (0.001 * sr), // linear 1 ms attack
            gain_down: tc(0.080),
            hold: 0,
            hold_len: (0.050 * sr) as u32,
            open: false,
        }
    }

    /// `db <= GATE_OFF_DB` bypasses the gate bit-exactly.
    pub fn set_threshold_db(&mut self, db: f32) {
        let was_bypass = self.bypass;
        self.bypass = db <= GATE_OFF_DB;
        if was_bypass && !self.bypass {
            // Enter fully open so the gate closes smoothly via hold + release instead of hard-muting.
            self.gain = 1.0;
            self.open = true;
            self.hold = self.hold_len;
            self.env = 0.0;
        }
        self.open_thr = db_to_lin(db);
        self.close_thr = self.open_thr * 0.5; // 6 dB hysteresis
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn process(&mut self, x: f32) -> f32 {
        if self.bypass {
            return x;
        }
        // Peak detector: instant attack (the 1 ms attack lives in the gain ramp), 50 ms release.
        let a = x.abs();
        self.env = if a > self.env { a } else { self.env * self.env_rel };
        if self.env < 1e-12 {
            self.env = 0.0; // avoid sticking at a denormal after digital silence
        }
        if self.env >= self.open_thr {
            self.open = true;
        }
        if self.env >= self.close_thr {
            self.hold = self.hold_len;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.open = false;
        }
        if self.open {
            self.gain = (self.gain + self.gain_up).min(1.0);
        } else if self.gain > 0.0 {
            self.gain *= self.gain_down;
            if self.gain < 1e-4 {
                self.gain = 0.0; // exact zero so downstream idles at exactly 0.0
            }
        }
        if self.gain == 0.0 {
            0.0
        } else {
            x * self.gain
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48000.0;

    struct Rng(u32);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0 // uniform [-1, 1)
        }
    }
    // uniform [-1,1) has RMS 1/sqrt(3)
    fn noise(rng: &mut Rng, rms_db: f32) -> f32 {
        rng.next() * 3f32.sqrt() * db_to_lin(rms_db)
    }

    #[test]
    fn gate_off_is_bit_exact() {
        let mut g = NoiseGate::new(SR);
        g.set_threshold_db(GATE_OFF_DB);
        let mut r = Rng(1);
        for _ in 0..48000 {
            let x = noise(&mut r, -20.0);
            assert_eq!(g.process(x).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn gate_silences_noise() {
        let mut g = NoiseGate::new(SR);
        g.set_threshold_db(-50.0);
        let mut r = Rng(2);
        let mut sum = 0.0f64;
        let mut n = 0;
        for i in 0..48000 {
            let y = g.process(noise(&mut r, -70.0));
            if i >= 14400 {
                sum += (y as f64).powi(2);
                n += 1;
            }
        }
        let db = 10.0 * (sum / n as f64).max(1e-30).log10();
        assert!(db <= -100.0, "rms {db} dBFS");
    }

    #[test]
    fn gate_passes_note_fast() {
        let mut g = NoiseGate::new(SR);
        g.set_threshold_db(-50.0);
        for _ in 0..9600 {
            assert_eq!(g.process(0.0), 0.0);
        }
        let a = db_to_lin(-20.0) * 2f32.sqrt();
        let mut peak96 = 0.0f32;
        for i in 0..960 {
            let x = a * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin();
            let y = g.process(x);
            if (48..96).contains(&i) {
                peak96 = peak96.max(y.abs());
            }
            if i >= 480 {
                assert!(y.abs() >= 0.99 * x.abs() - 1e-6, "i={i}");
            }
        }
        assert!(peak96 >= 0.9 * a * 0.99, "peak {peak96} vs {a}");
    }

    #[test]
    fn gate_no_chatter() {
        let mut g = NoiseGate::new(SR);
        g.set_threshold_db(-50.0);
        let mut r = Rng(3);
        let n = 96000;
        let mut open = false;
        let mut closes = 0;
        let mut ever_open = false;
        for i in 0..n {
            let t = i as f32 / SR;
            let env_db = -20.0 - 40.0 * (i as f32 / n as f32);
            let s = db_to_lin(env_db) * 2f32.sqrt() * (2.0 * std::f32::consts::PI * 220.0 * t).sin();
            let x = s + noise(&mut r, -75.0);
            g.process(x);
            let gain = g.gain();
            let now = gain > 0.5;
            if open && !now {
                closes += 1;
            }
            open = now;
            ever_open |= now;
        }
        assert!(ever_open, "gate never opened");
        assert!(closes <= 1, "closes={closes}");
    }

    #[test]
    fn gate_env_flushes_to_zero() {
        let mut g = NoiseGate::new(SR);
        g.set_threshold_db(-50.0);
        for _ in 0..4800 {
            g.process(0.5);
        }
        for _ in 0..480_000 {
            g.process(0.0);
        }
        assert_eq!(g.env, 0.0);
    }

    #[test]
    fn gate_off_to_on_is_click_free() {
        let mut g = NoiseGate::new(SR);
        g.set_threshold_db(GATE_OFF_DB);
        let a = db_to_lin(-20.0) * 2f32.sqrt();
        let sine = |i: usize| a * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin();
        let mut prev = 0.0;
        let mut steady = 0.0f32;
        for i in 0..9600 {
            let y = g.process(sine(i));
            steady = steady.max((y - prev).abs());
            prev = y;
        }
        g.set_threshold_db(-50.0);
        let mut worst = 0.0f32;
        for i in 9600..19200 {
            let y = g.process(sine(i));
            worst = worst.max((y - prev).abs());
            prev = y;
        }
        assert!(worst <= 1.5 * steady, "worst {worst} steady {steady}");
    }
}
