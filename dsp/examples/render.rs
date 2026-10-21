//! Offline renderer: `cargo run --release --example render -- <in.wav> [preset] [out_dir]`.
//! Reads channel 0 (16/24-bit int or float WAV), writes `<out_dir>/<preset>.wav` as mono f32.
use amp_dsp::{AmpEngine, Preset};
use std::path::PathBuf;

const NAMES: [&str; 4] = ["clean", "crunch", "overdrive", "distortion"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(input) = args.first() else {
        return Err("usage: render <in.wav> [clean|crunch|overdrive|distortion] [out_dir]".into());
    };
    let presets: Vec<Preset> = match args.get(1) {
        None => Preset::ALL.to_vec(),
        Some(name) => {
            let i = NAMES.iter().position(|n| n == name).ok_or(format!("unknown preset {name}"))?;
            vec![Preset::ALL[i]]
        }
    };
    let out_dir = PathBuf::from(args.get(2).map_or("renders", String::as_str));

    let mut reader = hound::WavReader::open(input)?;
    let spec = reader.spec();
    let ch = spec.channels as usize;
    let x: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().step_by(ch).collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().step_by(ch).map(|s| s.map(|v| v as f32 / scale)).collect::<Result<_, _>>()?
        }
    };

    std::fs::create_dir_all(&out_dir)?;
    let out_spec = hound::WavSpec {
        channels: 1,
        sample_rate: spec.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    for pr in presets {
        let mut e = AmpEngine::new(spec.sample_rate as f32);
        e.set_preset(pr);
        e.reset(); // start on the preset's values, no ramp from distortion
        let mut y = vec![0.0f32; x.len()];
        for (i, o) in x.chunks(128).zip(y.chunks_mut(128)) {
            e.process(i, o);
        }
        let name = NAMES[pr as usize];
        let path = out_dir.join(format!("{name}.wav"));
        let mut w = hound::WavWriter::create(&path, out_spec)?;
        for v in &y {
            w.write_sample(*v)?;
        }
        w.finalize()?;
        let peak = y.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let rms = (y.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / y.len().max(1) as f64).sqrt();
        let db = |v: f64| 20.0 * v.max(1e-30).log10();
        println!("{name:<10} peak {:7.2} dBFS  rms {:7.2} dBFS  -> {}", db(peak as f64), db(rms), path.display());
    }
    Ok(())
}
