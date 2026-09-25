//! Measure the real authoring open/save path, including resource validation and durable writes.
//! Usage: cargo run --release -p kasane-project --example project_io_stress -- INPUT OUTPUT_DIR [REPEATS]

use kasane_project::{decode_project, encode_project, DocumentSession};
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

fn measure(mut run: impl FnMut()) -> f64 {
    let start = Instant::now();
    run();
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if !(3..=4).contains(&args.len()) {
        eprintln!("Usage: project_io_stress INPUT_PROJECT OUTPUT_DIR [REPEATS]");
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let repeats: usize = args
        .get(3)
        .map(|arg| arg.to_string_lossy().parse().expect("invalid repeat count"))
        .unwrap_or(5);
    assert!(repeats > 0);
    fs::create_dir_all(output).unwrap();
    assert!(
        !output.join("project.kasane.json").exists(),
        "OUTPUT_DIR already contains a project; choose a fresh directory"
    );

    let bytes = fs::read(input.join("project.kasane.json")).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let mut times = serde_json::Map::new();

    let mut decode = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        decode.push(measure(|| {
            black_box(decode_project(black_box(text)).expect("decode project"));
        }));
    }
    times.insert("decode_ms".into(), serde_json::json!(decode));

    let mut session = DocumentSession::new();
    let mut open = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        open.push(measure(|| {
            let result = session.open_authoring(input);
            assert!(
                result.status.is_ok() && result.resources_complete(),
                "{result:?}"
            );
        }));
    }
    times.insert("open_ms".into(), serde_json::json!(open));

    let mut encode = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        encode.push(measure(|| {
            black_box(encode_project(black_box(session.document())).expect("encode project"));
        }));
    }
    times.insert("encode_ms".into(), serde_json::json!(encode));

    let mut save = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        save.push(measure(|| {
            let result = session.save(output);
            assert!(
                result.status.is_ok() && result.resources_complete(),
                "{result:?}"
            );
        }));
    }
    times.insert("save_ms".into(), serde_json::json!(save));

    let mut reopened = DocumentSession::new();
    let result = reopened.open_authoring(output);
    assert!(
        result.status.is_ok() && result.resources_complete(),
        "{result:?}"
    );
    assert!(session.document().same_content(reopened.document()));

    let saved_bytes = fs::metadata(output.join("project.kasane.json"))
        .unwrap()
        .len();
    let doc = session.document();
    let report = serde_json::json!({
        "input_manifest_bytes": bytes.len(),
        "saved_manifest_bytes": saved_bytes,
        "meshes": doc.mesh_order().len(),
        "bindings": doc.binding_order().len(),
        "assets": doc.asset_order().len(),
        "repeats": repeats,
        "times": times,
        "roundtrip_equal": true
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
