mod common;

use amp_dsp::{AmpEngine, Param, Preset, DSP_LATENCY_SAMPLES, GATE_OFF_DB};
use common::*;

const SR: f32 = 48000.0;

fn set_all(e: &mut AmpEngine, max: bool) {
    for p in Param::ALL {
        let (lo, hi) = p.range();
        e.set_param(p, if max { hi } else { lo });
    }
}

fn check_bounded(sr: f32, maxes: &[bool]) {
    let n = sr as usize;
    let square: Vec<f32> = sine(sr, 220.0, 1.0, 1.0).iter().map(|v| if *v >= 0.0 { 1.0 } else { -1.0 }).collect();
    let noise: Vec<f32> = noise(7, 0.7, n).iter().map(|v| v.clamp(-1.0, 1.0)).collect();
    let signals = [sine(sr, 220.0, 1.0, 1.0), noise, square];
    for pr in Preset::ALL {
        for &max in maxes {
            for (k, x) in signals.iter().enumerate() {
                let mut e = AmpEngine::new(sr);
                e.set_preset(pr);
                set_all(&mut e, max);
                let y = run(&mut e, x);
                for (i, v) in y.iter().enumerate() {
                    assert!(v.is_finite() && v.abs() <= 1.0, "sr {sr} {pr:?} max={max} sig {k} [{i}] = {v}");
                }
            }
        }
    }
}

fn check_idle(sr: f32, presets: &[Preset]) {
    for &pr in presets {
        let mut e = AmpEngine::new(sr);
        e.set_preset(pr);
        e.set_param(Param::Gate, GATE_OFF_DB);
        let mut x = pluck(sr, 110.0, 0.5, 1.0);
        x.extend(std::iter::repeat(0.0).take((3.0 * sr) as usize));
        let y = run(&mut e, &x);
        let tail = &y[y.len() - (0.5 * sr) as usize..];
        let bad = tail.iter().position(|v| *v != 0.0);
        assert!(bad.is_none(), "sr {sr} {pr:?}: tail[{}] = {}", bad.unwrap_or(0), tail[bad.unwrap_or(0)]);
    }
}

#[test]
fn preset_switching_sets_all_params() {
    let mut e = AmpEngine::new(SR);
    assert_eq!(e.preset(), Preset::Distortion);
    for pr in Preset::ALL {
        e.set_preset(pr);
        assert_eq!(e.preset(), pr);
        for p in Param::ALL {
            assert_eq!(e.param(p), pr.knobs()[p as usize], "{pr:?} {p:?}");
        }
    }
}

#[test]
fn set_param_clamps_and_ignores_nan() {
    let mut e = AmpEngine::new(SR);
    e.set_param(Param::Drive, 2.0);
    assert_eq!(e.param(Param::Drive), 1.0);
    e.set_param(Param::Drive, f32::NAN);
    assert_eq!(e.param(Param::Drive), 1.0);
    e.set_param(Param::Drive, f32::INFINITY);
    assert_eq!(e.param(Param::Drive), 1.0);
}

#[test]
fn finite_and_bounded_full_scale() {
    check_bounded(SR, &[true, false]);
}

#[test]
fn idle_output_is_exact_zero() {
    check_idle(SR, &Preset::ALL);
}

#[test]
fn no_dc_offset() {
    let period = (0.5 * SR) as usize;
    let p = pluck(SR, 110.0, 0.3, 0.5);
    let x: Vec<f32> = (0..6 * period).map(|i| p[i % period]).collect();
    for pr in Preset::ALL {
        let mut e = AmpEngine::new(SR);
        e.set_preset(pr);
        let y = run(&mut e, &x);
        let last = &y[y.len() - 2 * SR as usize..];
        let mean = last.iter().map(|v| *v as f64).sum::<f64>() / last.len() as f64;
        assert!(mean.abs() < 1e-3, "{pr:?} mean {mean}");
        assert!(db(rms(last)) > -50.0, "{pr:?} silent: {} dBFS", db(rms(last)));
    }
}

#[test]
fn non_finite_input_does_not_poison_state() {
    for pr in Preset::ALL {
        let mut e = AmpEngine::new(SR);
        e.set_preset(pr);
        e.set_param(Param::Gate, GATE_OFF_DB);
        let mut x = sine(SR, 220.0, 0.1, 0.5);
        x[100] = f32::NAN;
        x[200] = f32::INFINITY;
        let y = run(&mut e, &x);
        assert!(y.iter().all(|v| v.is_finite()), "{pr:?} non-finite output");
        assert!(db(rms(&y[y.len() / 2..])) > -50.0, "{pr:?} silent after NaN");
    }
}

#[test]
fn master_fades_to_exact_zero() {
    let mut e = AmpEngine::new(SR);
    e.set_preset(Preset::Clean);
    let x = sine(SR, 1000.0, 0.1, 1.0);
    let j = 128 * 100;
    let mut y = run(&mut e, &x[..j]);
    e.set_param(Param::Master, 0.0);
    y.extend(run(&mut e, &x[j..]));
    let steady = rms(&y[j - 2400..j]);
    // Last 20 samples of the 720-sample ramp: without the fade the gain is still about -40 dB here
    // and then jumps to 0 (a click); with it the level is already near silence.
    let end = rms(&y[j + 700..j + 720]);
    assert!(end <= 0.03 * steady, "end of ramp {end} vs steady {steady}");
    // Master 0 is a gain of exactly 0; only the DC blocker's tiny decay remains.
    let tail = y[j + 2400..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(tail < 1e-5, "tail {tail}");
}

#[test]
fn chunking_invariance() {
    let x = pluck(SR, 110.0, 0.5, 1.0);
    let split = 10000;
    let render = |chunk: usize| {
        let mut e = AmpEngine::new(SR);
        let mut y = vec![0.0; x.len()];
        for (range_x, range_y) in [(&x[..split], 0..split), (&x[split..], split..x.len())] {
            if range_y.start == split {
                e.set_param(Param::Bass, 0.9);
            }
            for (i, o) in range_x.chunks(chunk).zip(y[range_y].chunks_mut(chunk)) {
                e.process(i, o);
            }
        }
        y
    };
    let reference = render(x.len());
    for chunk in [1, 37, 128] {
        let y = render(chunk);
        for (i, (a, b)) in reference.iter().zip(&y).enumerate() {
            assert!((a - b).abs() <= 1e-6, "chunk {chunk} [{i}]: {a} vs {b}");
        }
    }
}

#[test]
fn other_sample_rates() {
    for sr in [44100.0, 96000.0] {
        check_bounded(sr, &[true]);
        check_idle(sr, &[Preset::Distortion]);
    }
}

#[test]
fn mismatched_lengths() {
    let mut e = AmpEngine::new(SR);
    let x = sine(SR, 220.0, 0.5, 100.0 / SR);
    assert_eq!(x.len(), 100);
    let mut y = [1.0f32; 128];
    e.process(&x, &mut y);
    assert!(y[100..].iter().all(|v| *v == 0.0));
}

#[test]
fn latency_report() {
    assert!(DSP_LATENCY_SAMPLES <= 32);
}
