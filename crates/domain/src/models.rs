//! Catalogul de modele de transcriere: ce exista, cat ocupa, de unde se descarca.

use std::path::{Path, PathBuf};

use crate::settings::EngineKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFamily {
    /// Transducer NeMo (Parakeet): encoder / decoder / joiner.
    NemoTransducer,
    /// Whisper: encoder / decoder.
    Whisper,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub name: &'static str,
    /// Marimea in octeti; 0 = necunoscuta (model pus de mana).
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub kind: EngineKind,
    /// Acelasi text ca in config.toml (`engine = "parakeet-ro"`).
    pub id: &'static str,
    /// Subfolderul din `models_dir`.
    pub dir: &'static str,
    pub title: &'static str,
    pub languages: &'static str,
    pub note: &'static str,
    pub family: ModelFamily,
    /// Depozitul Hugging Face din care se descarca; `None` = nu se poate descarca.
    pub repo: Option<&'static str>,
    pub files: &'static [ModelFile],
}

const fn f(name: &'static str, bytes: u64) -> ModelFile {
    ModelFile { name, bytes }
}

const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        kind: EngineKind::ParakeetRo,
        id: "parakeet-ro",
        dir: "parakeet-ro-tdt",
        title: "Parakeet Ro 110M",
        languages: "Romanian",
        note: "Best for Romanian, very fast. No punctuation or capitalization.",
        family: ModelFamily::NemoTransducer,
        // Export ONNX (ramura TDT, int8) al modelului SpeD; vezi scripts/export-parakeet-ro.py.
        repo: Some("cristimihailax/parakeet-ro-110m-tdt-onnx"),
        files: &[
            f("encoder.int8.onnx", 131_113_716),
            f("decoder.int8.onnx", 3_955_864),
            f("joiner.int8.onnx", 1_411_402),
            f("tokens.txt", 10_111),
            // Vocabularul BPE: fara el nu merg indiciile din dictionar (hotwords).
            f("bpe.vocab", 15_173),
        ],
    },
    ModelSpec {
        kind: EngineKind::ParakeetV3,
        id: "parakeet-v3",
        dir: "parakeet-v3",
        title: "Parakeet v3",
        languages: "25 European languages",
        note: "Multilingual, with punctuation. Weaker on Romanian than the dedicated model.",
        family: ModelFamily::NemoTransducer,
        repo: Some("csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8"),
        files: &[
            f("encoder.int8.onnx", 652_282_294),
            f("decoder.int8.onnx", 11_845_274),
            f("joiner.int8.onnx", 6_355_277),
            f("tokens.txt", 93_939),
        ],
    },
    ModelSpec {
        kind: EngineKind::ParakeetV2,
        id: "parakeet-v2",
        dir: "parakeet-v2",
        title: "Parakeet v2",
        languages: "English",
        note: "English only, very accurate and fast.",
        family: ModelFamily::NemoTransducer,
        repo: Some("csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8"),
        files: &[
            f("encoder.int8.onnx", 652_282_309),
            f("decoder.int8.onnx", 7_257_753),
            f("joiner.int8.onnx", 1_739_080),
            f("tokens.txt", 9_384),
        ],
    },
    ModelSpec {
        kind: EngineKind::WhisperTurbo,
        id: "whisper-turbo",
        dir: "whisper-turbo",
        title: "Whisper Turbo",
        languages: "99 languages, including Romanian",
        note: "With punctuation and capitalization. Slower; up to 30 seconds per dictation.",
        family: ModelFamily::Whisper,
        repo: Some("csukuangfj/sherpa-onnx-whisper-turbo"),
        files: &[
            f("turbo-encoder.int8.onnx", 674_716_297),
            f("turbo-decoder.int8.onnx", 361_080_764),
            f("turbo-tokens.txt", 816_730),
        ],
    },
    ModelSpec {
        kind: EngineKind::WhisperSmall,
        id: "whisper-small",
        dir: "whisper-small",
        title: "Whisper Small",
        languages: "99 languages, including Romanian",
        note: "Small and fast, less accurate. Up to 30 seconds per dictation.",
        family: ModelFamily::Whisper,
        repo: Some("csukuangfj/sherpa-onnx-whisper-small"),
        files: &[
            f("small-encoder.int8.onnx", 112_442_483),
            f("small-decoder.int8.onnx", 262_226_114),
            f("small-tokens.txt", 816_730),
        ],
    },
];

pub fn catalog() -> &'static [ModelSpec] {
    CATALOG
}

impl ModelSpec {
    pub fn of(kind: EngineKind) -> &'static ModelSpec {
        CATALOG
            .iter()
            .find(|m| m.kind == kind)
            .expect("every engine has a catalog entry")
    }

    pub fn by_id(id: &str) -> Option<&'static ModelSpec> {
        CATALOG.iter().find(|m| m.id == id)
    }

    pub fn path(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.dir)
    }

    /// Fisierul al carui nume se termina cu `suffix` (ex. "encoder.int8.onnx", "tokens.txt").
    pub fn file(&self, suffix: &str) -> Option<&'static str> {
        self.files.iter().map(|f| f.name).find(|n| n.ends_with(suffix))
    }

    /// Toate fisierele exista si au marimea asteptata (unde o stim).
    pub fn installed(&self, models_dir: &Path) -> bool {
        let dir = self.path(models_dir);
        self.files.iter().all(|f| match std::fs::metadata(dir.join(f.name)) {
            Ok(m) => m.is_file() && m.len() > 0 && (f.bytes == 0 || m.len() == f.bytes),
            Err(_) => false,
        })
    }

    /// Cat ocupa: din catalog, sau de pe disc pentru modelele puse de mana.
    pub fn size_bytes(&self, models_dir: &Path) -> u64 {
        let known: u64 = self.files.iter().map(|f| f.bytes).sum();
        if known > 0 {
            return known;
        }
        let dir = self.path(models_dir);
        self.files
            .iter()
            .filter_map(|f| std::fs::metadata(dir.join(f.name)).ok())
            .map(|m| m.len())
            .sum()
    }

    pub fn url(&self, file: &str) -> Option<String> {
        self.repo
            .map(|r| format!("https://huggingface.co/{r}/resolve/main/{file}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_is_in_the_catalog_once() {
        for m in catalog() {
            assert_eq!(ModelSpec::of(m.kind).id, m.id);
            assert_eq!(ModelSpec::by_id(m.id).unwrap().kind, m.kind);
            assert_eq!(catalog().iter().filter(|x| x.dir == m.dir).count(), 1);
        }
    }

    #[test]
    fn ids_match_the_config_spelling() {
        for m in catalog() {
            let toml = format!("engine = \"{}\"\n", m.id);
            let s = crate::Settings::from_toml(&toml).unwrap();
            assert_eq!(s.engine, m.kind);
        }
    }

    #[test]
    fn installed_needs_every_file_at_its_size() {
        let tmp = std::env::temp_dir().join(format!("vf-models-{}", std::process::id()));
        for m in catalog() {
            let dir = m.path(&tmp);
            std::fs::create_dir_all(&dir).unwrap();
            assert!(!m.installed(&tmp), "{}: folder gol", m.id);
            // Fisiere prezente, dar trunchiate (descarcare intrerupta): nu e "instalat".
            for f in m.files {
                std::fs::write(dir.join(f.name), b"x").unwrap();
            }
            assert!(!m.installed(&tmp), "{}: fisiere trunchiate", m.id);
        }
        // Un fisier mic, la marimea exacta din catalog, e acceptat.
        let ro = ModelSpec::by_id("parakeet-ro").unwrap();
        let small = ro.files.iter().find(|f| f.name == "tokens.txt").unwrap();
        std::fs::write(ro.path(&tmp).join(small.name), vec![b'x'; small.bytes as usize]).unwrap();
        let meta = std::fs::metadata(ro.path(&tmp).join(small.name)).unwrap();
        assert_eq!(meta.len(), small.bytes);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
