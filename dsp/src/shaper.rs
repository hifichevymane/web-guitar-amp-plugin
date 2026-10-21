//! Waveshaping: soft/hard clipper, tanh approximation, output safety limiter.

/// Pade 7/6 tanh; clamped at +-4 (tanh = 0.9993) so it stays monotonic and bounded; |err| < 1e-3.
pub fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let n = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let d = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + 28.0 * x2));
    (n / d).clamp(-1.0, 1.0)
}

/// x / (1+x^4)^(1/4): unit slope at 0, sharp but C-infinity knee, saturates at 1.
fn hard_knee(x: f32) -> f32 {
    let x = x.clamp(-100.0, 100.0);
    let x2 = x * x;
    x / (1.0 + x2 * x2).sqrt().sqrt()
}

/// The clipper's transfer curve; `clip(x, b, h) == shape(x + b, h) - shape(b, h)`.
pub(crate) fn shape(x: f32, hardness: f32) -> f32 {
    let t = fast_tanh(x);
    t + hardness * (hard_knee(x) - t)
}

/// Asymmetric clipper `g(x+bias) - g(bias)`; hardness 0 = tanh, 1 = hard knee (still smooth).
pub fn clip(x: f32, bias: f32, hardness: f32) -> f32 {
    shape(x + bias, hardness) - shape(bias, hardness)
}

/// Linear to 0.8, then a tanh shoulder asymptoting at 1.0.
pub fn safety_limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.8 {
        x
    } else {
        (0.8 + 0.2 * fast_tanh((a - 0.8) * 5.0)).copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BH: [(f32, f32); 9] = [
        (0.0, 0.0), (0.0, 0.5), (0.0, 1.0),
        (0.05, 0.0), (0.05, 0.5), (0.05, 1.0),
        (0.15, 0.0), (0.15, 0.5), (0.15, 1.0),
    ];

    #[test]
    fn fast_tanh_accurate_and_bounded() {
        let mut x = -10.0f32;
        while x <= 10.0 {
            assert!((fast_tanh(x) - x.tanh()).abs() < 1e-3, "x={x}");
            assert!(fast_tanh(x).abs() <= 1.0);
            x += 0.001;
        }
        assert!(fast_tanh(1e9).abs() <= 1.0);
    }

    #[test]
    fn clip_zero_is_zero() {
        for (b, h) in BH {
            assert_eq!(clip(0.0, b, h), 0.0, "b={b} h={h}");
        }
    }

    #[test]
    fn clip_bounded_monotonic() {
        for (b, h) in BH {
            let mut xs: Vec<f32> = Vec::new();
            for i in 0..=2000 {
                xs.push(-10.0 + i as f32 * 0.01);
            }
            for e in 0..=60 {
                let m = 10f32.powf(-3.0 + e as f32 * 0.15); // up to ~1e6
                xs.push(m);
                xs.push(-m);
            }
            xs.push(1e6);
            xs.push(-1e6);
            xs.sort_by(|a, c| a.partial_cmp(c).unwrap());
            let mut prev = f32::NEG_INFINITY;
            for x in xs {
                let y = clip(x, b, h);
                assert!(y.abs() < 1.5, "x={x} b={b} h={h} y={y}");
                // 1e-6 slack: f32 rounding between near-duplicate sweep points is ~1 ulp.
                assert!(y >= prev - 1e-6, "not monotonic at x={x} b={b} h={h}: {y} < {prev}");
                prev = y;
            }
        }
    }

    #[test]
    fn clip_asymmetric() {
        assert!((clip(2.0, 0.15, 0.2) + clip(-2.0, 0.15, 0.2)).abs() > 0.01);
    }

    #[test]
    fn clip_small_signal_linear() {
        for (b, h) in BH {
            let r = clip(0.001, b, h) / 0.001;
            assert!((0.8..=1.2).contains(&r), "b={b} h={h} r={r}");
        }
    }

    #[test]
    fn safety_limit_bounds() {
        assert_eq!(safety_limit(0.5), 0.5);
        assert_eq!(safety_limit(-0.5), -0.5);
        assert!(safety_limit(1e9).abs() < 1.0 && safety_limit(-1e9).abs() < 1.0);
        assert!(safety_limit(-1e9) < 0.0);
        assert!((safety_limit(0.8 + 1e-4) - safety_limit(0.8)).abs() < 1e-3);
    }
}
