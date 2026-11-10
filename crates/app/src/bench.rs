//! Banc de test: ruleaza motorul cu si fara dictionar pe toate .wav-urile care au .txt alaturi.

use std::path::Path;
use std::time::Duration;

use vf_application::wer::{char_errors, word_errors};
use vf_application::Dictation;
use vf_domain::{Dictionary, Settings};

struct Score {
    w: (usize, usize),
    c: (usize, usize),
    asr: Duration,
    audio: Duration,
}

pub fn run(dir: &Path, s: &Settings, dict: Dictionary) -> anyhow::Result<()> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "wav") && p.with_extension("txt").exists())
        .collect();
    files.sort();
    anyhow::ensure!(
        !files.is_empty(),
        "No .wav + .txt pairs found in {}",
        dir.display()
    );

    let audio: Vec<(String, Vec<f32>, String)> = files
        .iter()
        .map(|p| {
            Ok((
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                crate::wav::read_16k_mono(p)?,
                std::fs::read_to_string(p.with_extension("txt"))?
                    .trim()
                    .to_string(),
            ))
        })
        .collect::<anyhow::Result<_>>()?;

    let mut configs = vec![("no dictionary", Dictionary::default())];
    if !dict.is_empty() {
        configs.push(("with dictionary", dict));
    }

    println!("\n{} recordings from {}\n", audio.len(), dir.display());
    let mut details: Vec<(String, Vec<String>)> = audio
        .iter()
        .map(|(n, _, r)| (format!("[{n}] REF: {r}"), vec![]))
        .collect();
    let mut rows = Vec::new();

    for (label, d) in configs {
        let (dictation, _): (Dictation, _) = crate::build_dictation(s, d)?;
        let mut total = Score {
            w: (0, 0),
            c: (0, 0),
            asr: Duration::ZERO,
            audio: Duration::ZERO,
        };
        let mut voice = Score {
            w: (0, 0),
            c: (0, 0),
            asr: Duration::ZERO,
            audio: Duration::ZERO,
        };
        for (i, (name, samples, reference)) in audio.iter().enumerate() {
            let r = dictation.process(samples)?;
            let text = r.as_ref().map(|r| r.text.clone()).unwrap_or_default();
            let (we, wn) = word_errors(reference, &text);
            let (ce, cn) = char_errors(reference, &text);
            for sc in
                std::iter::once(&mut total).chain(name.starts_with("voce").then_some(&mut voice))
            {
                sc.w.0 += we;
                sc.w.1 += wn;
                sc.c.0 += ce;
                sc.c.1 += cn;
                if let Some(r) = &r {
                    sc.asr += r.asr_time;
                    sc.audio += r.audio;
                }
            }
            details[i].1.push(format!("  {label:16} {text}"));
        }
        rows.push((label, total, voice));
    }

    let pct = |(e, n): (usize, usize)| {
        if n == 0 {
            0.0
        } else {
            100.0 * e as f64 / n as f64
        }
    };
    println!(
        "{:18} {:>18} {:>18} {:>10}",
        "", "total WER / CER", "your voice WER/CER", "speed"
    );
    for (label, t, v) in &rows {
        let speed = if t.asr.is_zero() {
            0.0
        } else {
            t.audio.as_secs_f64() / t.asr.as_secs_f64()
        };
        let voice = if v.w.1 > 0 {
            format!("{:5.1}% / {:4.1}%", pct(v.w), pct(v.c))
        } else {
            "-".into()
        };
        println!(
            "{label:18} {:>9.1}% / {:4.1}% {voice:>18} {speed:>8.0}x",
            pct(t.w),
            pct(t.c)
        );
    }
    println!("\nDetails:");
    for (head, lines) in details {
        println!("\n{head}");
        for l in lines {
            println!("{l}");
        }
    }
    Ok(())
}
