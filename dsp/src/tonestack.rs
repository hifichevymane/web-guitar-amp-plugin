//! Bass / mid / treble stack: low shelf, peaking mid, high shelf. `set` only retunes coefficients.

use crate::filters::Biquad;

pub struct ToneStack {
    sr: f32,
    low: Biquad,
    mid: Biquad,
    high: Biquad,
}

impl ToneStack {
    pub fn new(sr: f32) -> Self {
        let mut t = Self { sr, low: Biquad::new(), mid: Biquad::new(), high: Biquad::new() };
        t.set(0.5, 0.5, 0.5, 700.0);
        t
    }

    /// Knobs 0..1. Mid pot in the ground leg: shelf gains scale by 0.6 + 0.4 * (1 - mid).
    pub fn set(&mut self, bass: f32, mid: f32, treble: f32, mid_hz: f32) {
        let k = 0.6 + 0.4 * (1.0 - mid);
        self.low.set_low_shelf(self.sr, 120.0, (bass - 0.5) * 24.0 * k);
        self.mid.set_peaking(self.sr, mid_hz, 0.8, (mid - 0.5) * 20.0);
        self.high.set_high_shelf(self.sr, 2500.0, (treble - 0.5) * 24.0 * k);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.high.process(self.mid.process(self.low.process(x)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::gain_db;

    const SR: f32 = 48000.0;

    fn gain(bass: f32, mid: f32, treble: f32, mid_hz: f32, hz: f32) -> f32 {
        let mut t = ToneStack::new(SR);
        t.set(bass, mid, treble, mid_hz);
        gain_db(|x| t.process(x), SR, hz)
    }

    #[test]
    fn tonestack_flat_at_noon() {
        for hz in [80.0, 500.0, 3000.0, 8000.0] {
            assert!(gain(0.5, 0.5, 0.5, 700.0, hz).abs() <= 0.5, "{hz}");
        }
    }

    #[test]
    fn tonestack_bass_treble_mid() {
        assert!(gain(1.0, 0.5, 0.5, 700.0, 80.0) >= 6.0);
        assert!(gain(1.0, 0.5, 0.5, 700.0, 3000.0).abs() <= 1.0);
        assert!(gain(0.5, 0.5, 1.0, 700.0, 6000.0) >= 6.0);
        assert!(gain(0.5, 0.0, 0.5, 700.0, 700.0) <= -6.0);
    }

    #[test]
    fn tonestack_interacts() {
        assert!(gain(1.0, 1.0, 0.5, 700.0, 80.0) < gain(1.0, 0.0, 0.5, 700.0, 80.0));
    }
}
