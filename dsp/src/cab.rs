//! Fixed closed-back 4x12 voicing: HP, two peaks, 4th-order Butterworth LP.

use crate::filters::Biquad;

pub struct Cabinet {
    stages: [Biquad; 5],
}

impl Cabinet {
    pub fn new(sr: f32) -> Self {
        let mut s = [Biquad::new(); 5];
        s[0].set_highpass(sr, 70.0, 0.7);
        s[1].set_peaking(sr, 110.0, 1.4, 4.0);
        s[2].set_peaking(sr, 2200.0, 1.0, 2.0);
        s[3].set_lowpass(sr, 5500.0, 0.541);
        s[4].set_lowpass(sr, 5500.0, 1.307);
        Self { stages: s }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.stages.iter_mut().fold(x, |y, b| b.process(y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::gain_db;

    #[test]
    fn cab_shape() {
        for sr in [44100.0, 48000.0, 96000.0] {
            let mut c = Cabinet::new(sr);
            let mut g = |hz| {
                c.stages.iter_mut().for_each(|b| b.reset());
                gain_db(|x| c.process(x), sr, hz)
            };
            let r = g(1000.0);
            assert!(g(110.0) - r >= g(400.0) - r + 2.0, "sr {sr}");
            assert!(g(40.0) - r <= -6.0, "sr {sr}");
            assert!(g(10000.0) - r <= -20.0, "sr {sr}");
        }
    }
}
