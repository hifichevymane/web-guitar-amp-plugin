mod common;

use amp_dsp::{AmpEngine, Param, Preset, GATE_OFF_DB};
use common::*;

const SR: f32 = 48000.0;

/// Engine on `pr` with the gate off, settled past the preset ramp.
fn engine(pr: Preset) -> AmpEngine {
    let mut e = AmpEngine::new(SR);
    e.set_preset(pr);
    e.set_param(Param::Gate, GATE_OFF_DB);
    e.reset();
    e
}

/// THD of a steady sine through `pr`, measured after 0.5 s of settling.
fn sine_thd(pr: Preset, hz: f32, amp: f32) -> f32 {
    let y = run(&mut engine(pr), &sine(SR, hz, amp, 1.5));
    thd(&y[(0.5 * SR) as usize..], hz, SR)
}

fn pluck_ratio(pr: Preset, peak: f32) -> f32 {
    let x = pluck(SR, 110.0, peak, 1.0);
    rms(&run(&mut engine(pr), &x)) / rms(&x)
}

/// Small-signal gain (dB) at `hz`, from the last 0.5 s of a 1.5 s sine.
fn gain_db(pr: Preset, hz: f32) -> f32 {
    let amp = 1e-4;
    let y = run(&mut engine(pr), &sine(SR, hz, amp, 1.5));
    let p = goertzel_power(&y[(SR as usize)..], hz, SR);
    10.0 * (p / (amp * amp / 2.0)).log10()
}

const MS: usize = (SR / 1000.0) as usize;

#[test]
fn clean_has_headroom() {
    let t = sine_thd(Preset::Clean, 220.0, 0.126);
    assert!(t < 0.01, "thd {t}");
}

#[test]
fn crunch_breaks_up_with_dynamics() {
    let soft = sine_thd(Preset::Crunch, 220.0, 0.01);
    let hard = sine_thd(Preset::Crunch, 220.0, 0.25);
    assert!(soft < 0.01, "soft thd {soft}");
    assert!(hard > 0.03, "hard thd {hard}");
}

#[test]
fn crunch_pluck_compresses() {
    let (lo, hi) = (pluck_ratio(Preset::Crunch, 0.02), pluck_ratio(Preset::Crunch, 0.5));
    assert!(hi < 0.5 * lo, "crunch ratio hi {hi} lo {lo}");
    let (lo, hi) = (pluck_ratio(Preset::Clean, 0.02), pluck_ratio(Preset::Clean, 0.126));
    assert!(hi >= 0.85 * lo, "clean ratio hi {hi} lo {lo}");
}

#[test]
fn overdrive_louder_is_dirtier() {
    let soft = sine_thd(Preset::Overdrive, 220.0, 0.02);
    let hard = sine_thd(Preset::Overdrive, 220.0, 0.25);
    assert!(hard > soft, "hard {hard} soft {soft}");
}

#[test]
fn distortion_tight_lows() {
    let dist = gain_db(Preset::Distortion, 60.0) - gain_db(Preset::Distortion, 500.0);
    let clean = gain_db(Preset::Clean, 60.0) - gain_db(Preset::Clean, 500.0);
    assert!(dist <= -6.0, "distortion 60/500 {dist} dB");
    assert!(dist < clean, "distortion {dist} dB vs clean {clean} dB");
}

#[test]
fn no_audible_aliasing() {
    let f0 = 1319.0;
    for pr in [Preset::Distortion, Preset::Overdrive] {
        let y = run(&mut engine(pr), &sine(SR, f0, 0.25, 1.5));
        let w = &y[(0.5 * SR) as usize..]; // exactly 1 s: every integer Hz is a whole bin
        let mean = w.iter().map(|v| *v as f64).sum::<f64>() / w.len() as f64;
        let total = w.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / w.len() as f64;
        let harm: f64 = (1..)
            .map(|k| k as f32 * f0)
            .take_while(|f| *f < SR / 2.0)
            .map(|f| goertzel_power(w, f, SR) as f64)
            .sum();
        let rel = 10.0 * ((total - harm).max(1e-30) / total).log10();
        assert!(rel <= -40.0, "{pr:?} non-harmonic power {rel} dB");
    }
}

#[test]
fn param_change_click_free() {
    let mut e = engine(Preset::Clean);
    e.set_param(Param::Master, 0.2);
    let x = sine(SR, 220.0, 0.1, 1.0);
    let j = 128 * 200; // block boundary, after the 0.2 ramp settled
    let mut y = run(&mut e, &x[..j]);
    e.set_param(Param::Master, 1.0);
    y.extend(run(&mut e, &x[j..]));
    let jump = max_step(&y[j - 1..j + 50 * MS]);
    let steady = max_step(&y[j + 100 * MS..j + 150 * MS]);
    assert!(jump <= 1.5 * steady, "jump {jump} steady {steady}");
}

#[test]
fn preset_switch_click_free() {
    let mut e = engine(Preset::Distortion);
    let x = sine(SR, 220.0, 0.1, 1.0);
    let s = 128 * 200;
    let mut y = run(&mut e, &x[..s]);
    e.set_preset(Preset::Clean); // clean's Gate is already off
    y.extend(run(&mut e, &x[s..]));
    let switch = max_step(&y[s - 1..s + 50 * MS]);
    let before = max_step(&y[s - 50 * MS..s]);
    let after = max_step(&y[s + 100 * MS..s + 150 * MS]);
    assert!(switch <= 1.5 * before.max(after), "switch {switch} before {before} after {after}");
}

fn gate_noise() -> Vec<f32> {
    let x = noise(11, 10f32.powf(-66.0 / 20.0), 2 * SR as usize);
    assert!(x.iter().all(|v| db(v.abs()) < -50.0));
    x
}

#[test]
fn distortion_gate_silences_noise() {
    let mut e = AmpEngine::new(SR); // distortion, default Gate
    let y = run(&mut e, &gate_noise());
    let r = db(rms(&y[(0.5 * SR) as usize..(1.5 * SR) as usize]));
    assert!(r <= -60.0, "rms {r} dBFS");
}

#[test]
fn distortion_gate_passes_note() {
    let mut x = gate_noise();
    let at = SR as usize;
    for (v, p) in x[at..].iter_mut().zip(pluck(SR, 110.0, 0.1, 1.0)) {
        *v += p;
    }
    let y = run(&mut AmpEngine::new(SR), &x);
    let r = db(rms(&y[at..at + 100 * MS]));
    assert!(r >= -30.0, "note rms {r} dBFS");
}

#[test]
fn di_fixture_safe() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../guitar-clean-riff.wav");
    let Ok(mut r) = hound::WavReader::open(path) else {
        eprintln!("skipping di_fixture_safe: {path} not found");
        return;
    };
    let spec = r.spec();
    let ch = spec.channels as usize;
    assert_eq!((spec.sample_format, spec.bits_per_sample), (hound::SampleFormat::Int, 16));
    let x: Vec<f32> = r.samples::<i16>().step_by(ch).map(|s| s.unwrap() as f32 / 32768.0).collect();
    for pr in Preset::ALL {
        let mut e = AmpEngine::new(spec.sample_rate as f32);
        e.set_preset(pr);
        e.reset();
        let y = run(&mut e, &x);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 1.0), "{pr:?} out of bounds");
        let mean = y.iter().map(|v| *v as f64).sum::<f64>() / y.len() as f64;
        assert!(mean.abs() < 1e-3, "{pr:?} mean {mean}");
    }
}
