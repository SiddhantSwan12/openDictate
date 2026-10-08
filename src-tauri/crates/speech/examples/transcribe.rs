//! Developer smoke test: `cargo run -p opendictate-speech --example transcribe -- <model-id> <file.wav> [--install]`
//! Mirrors BetterWispr's CLI target.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let id = args.get(1).expect("model id");
    let wav = PathBuf::from(args.get(2).expect("wav file"));
    let root = PathBuf::from(std::env::var("APPDATA")?).join("OpenDictate").join("Models");
    let model = opendictate_speech::find_model(id).expect("unknown model id");
    if args.iter().any(|a| a == "--install") && !opendictate_speech::is_installed(&root, &model) {
        let started = Instant::now();
        opendictate_speech::install(&root, &model, true, &AtomicBool::new(false), &|p| eprint!("\rdownloading {:5.1}%", p * 100.0))?;
        eprintln!("\ninstalled in {:.0}s", started.elapsed().as_secs_f64());
    }
    let started = Instant::now();
    let mut loaded = opendictate_speech::LoadedModel::load(&root, &model, true)?;
    eprintln!("loaded in {:.2}s", started.elapsed().as_secs_f64());
    let mut reader = hound::WavReader::open(&wav)?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader.samples::<i32>().map(|s| s.unwrap() as f32 / (1i64 << (spec.bits_per_sample - 1)) as f32).collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().map(Result::unwrap).collect(),
    };
    let audio = opendictate_speech::audio::to_speech_format(&samples, spec.channels as usize, spec.sample_rate);
    eprintln!("audio: {:.1}s", audio.len() as f64 / 16000.0);
    for run in 0..2 {
        let started = Instant::now();
        let text = loaded.transcribe(&audio, None, &[])?;
        eprintln!("run {run}: {:.2}s", started.elapsed().as_secs_f64());
        if run == 0 {
            println!("RAW: {text}");
            let cleaned = opendictate_core::cleaner::clean(&text, None);
            println!("CLEAN: {}", opendictate_core::voice_commands::apply(&cleaned));
        }
    }
    Ok(())
}
