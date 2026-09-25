//! Compare JSON and experimental CBOR through the same DocumentStore path.
//! Usage:
//!   project_binary_prototype prepare JSON_PROJECT CBOR_PROJECT
//!   project_binary_prototype bench json|cbor INPUT_PROJECT OUTPUT_PROJECT [REPEATS]

use kasane_core::Document;
use kasane_project::{
    decode_project, decode_project_cbor, encode_project, encode_project_cbor, DocumentSnapshot,
    DocumentStore,
};
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Copy)]
enum Format {
    Json,
    Cbor,
}

impl Format {
    fn parse(value: &str) -> Self {
        match value {
            "json" => Self::Json,
            "cbor" => Self::Cbor,
            _ => panic!("expected json or cbor"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Cbor => "cbor",
        }
    }

    fn manifest(self, root: &Path) -> std::path::PathBuf {
        root.join(match self {
            Self::Json => "project.kasane.json",
            Self::Cbor => "project.kasane.cbor",
        })
    }

    fn encode(self, document: &Document) -> Vec<u8> {
        match self {
            Self::Json => encode_project(document).unwrap().into_bytes(),
            Self::Cbor => encode_project_cbor(document).unwrap(),
        }
    }

    fn decode(self, bytes: &[u8]) -> Document {
        match self {
            Self::Json => decode_project(std::str::from_utf8(bytes).unwrap()).unwrap(),
            Self::Cbor => decode_project_cbor(bytes).unwrap(),
        }
    }

    fn open(self, store: &DocumentStore, path: &Path) -> DocumentSnapshot {
        let (result, snapshot) = match self {
            Self::Json => store.open(path),
            Self::Cbor => store.open_cbor(path),
        };
        assert!(
            result.status.is_ok() && result.resources_complete(),
            "{result:?}"
        );
        let snapshot = snapshot.unwrap();
        assert!(snapshot.document.validate_structure().is_empty());
        snapshot
    }

    fn save(
        self,
        store: &DocumentStore,
        document: &Document,
        source_root: &Path,
        path: &Path,
        expected: Option<&str>,
    ) -> DocumentSnapshot {
        let (result, snapshot) = match self {
            Self::Json => store.save(document, source_root, path, expected),
            Self::Cbor => store.save_cbor(document, source_root, path, expected),
        };
        assert!(
            result.status.is_ok() && result.resources_complete(),
            "{result:?}"
        );
        snapshot.unwrap()
    }
}

fn measure<T>(run: impl FnOnce() -> T) -> (f64, T) {
    let start = Instant::now();
    let result = run();
    (start.elapsed().as_secs_f64() * 1000.0, result)
}

fn prepare(input: &Path, output: &Path) {
    assert!(input.is_absolute() && output.is_absolute());
    assert!(!Format::Cbor.manifest(output).exists());
    let store = DocumentStore::new();
    let original = Format::Json.open(&store, input);
    let converted = Format::Cbor.save(&store, &original.document, input, output, None);
    let reopened = Format::Cbor.open(&store, output);
    assert!(converted.document.same_content(&reopened.document));
    assert!(original.document.same_content(&reopened.document));
    println!(
        "{}",
        serde_json::json!({
            "json_manifest_bytes": fs::metadata(Format::Json.manifest(input)).unwrap().len(),
            "cbor_manifest_bytes": fs::metadata(Format::Cbor.manifest(output)).unwrap().len(),
            "roundtrip_equal": true,
            "meshes": reopened.document.mesh_order().len(),
            "bindings": reopened.document.binding_order().len(),
            "assets": reopened.document.asset_order().len(),
        })
    );
}

fn benchmark(format: Format, input: &Path, output: &Path, repeats: usize) {
    assert!(input.is_absolute() && output.is_absolute() && repeats >= 3);
    assert!(!format.manifest(output).exists());
    fs::create_dir_all(output).unwrap();
    let bytes = fs::read(format.manifest(input)).unwrap();

    let mut decode_ms = Vec::new();
    for _ in 0..repeats {
        let (elapsed, document) = measure(|| format.decode(black_box(&bytes)));
        black_box(document);
        decode_ms.push(elapsed);
    }

    let store = DocumentStore::new();
    let mut open_ms = Vec::new();
    let mut snapshot = None;
    for _ in 0..repeats {
        let (elapsed, opened) = measure(|| format.open(&store, input));
        snapshot = Some(opened);
        open_ms.push(elapsed);
    }
    let mut snapshot = snapshot.unwrap();

    let mut encode_ms = Vec::new();
    for _ in 0..repeats {
        let (elapsed, encoded) = measure(|| format.encode(black_box(&snapshot.document)));
        black_box(encoded);
        encode_ms.push(elapsed);
    }

    let mut save_ms = Vec::new();
    for index in 0..repeats {
        let source_root = if index == 0 { input } else { output };
        let expected = if index == 0 {
            None
        } else {
            Some(snapshot.manifest_sha256.as_str())
        };
        let (elapsed, saved) =
            measure(|| format.save(&store, &snapshot.document, source_root, output, expected));
        snapshot = saved;
        save_ms.push(elapsed);
    }

    let reopened = format.open(&store, output);
    assert!(snapshot.document.same_content(&reopened.document));
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "format": format.name(),
            "input_manifest_bytes": bytes.len(),
            "saved_manifest_bytes": fs::metadata(format.manifest(output)).unwrap().len(),
            "repeats": repeats,
            "meshes": reopened.document.mesh_order().len(),
            "bindings": reopened.document.binding_order().len(),
            "assets": reopened.document.asset_order().len(),
            "roundtrip_equal": true,
            "times": {
                "decode_ms": decode_ms,
                "open_ms": open_ms,
                "encode_ms": encode_ms,
                "save_ms": save_ms,
            }
        }))
        .unwrap()
    );
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    match args.get(1).map(|arg| arg.to_string_lossy()) {
        Some(mode) if mode == "prepare" && args.len() == 4 => {
            prepare(Path::new(&args[2]), Path::new(&args[3]));
        }
        Some(mode) if mode == "bench" && (args.len() == 5 || args.len() == 6) => {
            let format = Format::parse(&args[2].to_string_lossy());
            let repeats = args
                .get(5)
                .map(|value| value.to_string_lossy().parse().unwrap())
                .unwrap_or(5);
            benchmark(format, Path::new(&args[3]), Path::new(&args[4]), repeats);
        }
        _ => {
            eprintln!("Usage: project_binary_prototype prepare JSON_PROJECT CBOR_PROJECT");
            eprintln!("   or: project_binary_prototype bench json|cbor INPUT OUTPUT [REPEATS]");
            std::process::exit(2);
        }
    }
}
