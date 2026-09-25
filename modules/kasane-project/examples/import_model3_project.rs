//! Import a real model3/MOC3 model into a self-contained editable project.
//! Usage: cargo run --release -p kasane-project --example import_model3_project -- MODEL3 OUTPUT_DIR

use kasane_project::{encode_project, DocumentSession};
use std::fs;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: import_model3_project MODEL3 OUTPUT_DIR");
        std::process::exit(2);
    }
    let source = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    assert!(source.is_absolute() && output.is_absolute());
    assert!(
        !output.join("project.kasane.json").exists(),
        "OUTPUT_DIR already contains a project"
    );

    let mut session = DocumentSession::new();
    let start = Instant::now();
    let (imported, report) = session.import_model3_authoring(source);
    assert!(imported.status.is_ok(), "{imported:?}");
    assert!(imported.resources_complete(), "{imported:?}");
    let import_ms = start.elapsed().as_secs_f64() * 1000.0;

    let report = report.expect("successful import has a report");
    let meshes = session.document().mesh_order().len();
    let bindings = session.document().binding_order().len();
    let assets = session.document().asset_order().len();
    let start = Instant::now();
    let saved = session.save(output);
    assert!(
        saved.status.is_ok() && saved.resources_complete(),
        "{saved:?}"
    );
    let initial_save_ms = start.elapsed().as_secs_f64() * 1000.0;

    let mut reopened = DocumentSession::new();
    let start = Instant::now();
    let opened = reopened.open_authoring(output);
    assert!(
        opened.status.is_ok() && opened.resources_complete(),
        "{opened:?}"
    );
    let first_open_ms = start.elapsed().as_secs_f64() * 1000.0;
    let roundtrip_equal = session.document().same_content(reopened.document());
    if !roundtrip_equal {
        let before = encode_project(session.document()).unwrap();
        let after = encode_project(reopened.document()).unwrap();
        if before != after {
            let offset = before
                .bytes()
                .zip(after.bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(before.len().min(after.len()));
            let before_start = before.floor_char_boundary(offset.saturating_sub(100));
            let after_start = after.floor_char_boundary(offset.saturating_sub(100));
            let before_end = before.ceil_char_boundary((offset + 100).min(before.len()));
            let after_end = after.ceil_char_boundary((offset + 100).min(after.len()));
            eprintln!(
                "First serialized difference at byte {offset}: before {:?}, after {:?}",
                &before[before_start..before_end],
                &after[after_start..after_end]
            );
        } else {
            eprintln!("Document differs after reopen, but serialized JSON is identical");
        }
    }

    let asset_bytes: u64 = fs::read_dir(output.join("assets"))
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "source_model3": source.display().to_string(),
            "moc_version": report.moc_version,
            "meshes": meshes,
            "bindings": bindings,
            "assets": assets,
            "manifest_bytes": fs::metadata(output.join("project.kasane.json")).unwrap().len(),
            "asset_bytes": asset_bytes,
            "import_ms": import_ms,
            "initial_save_ms": initial_save_ms,
            "first_open_ms": first_open_ms,
            "roundtrip_equal": roundtrip_equal,
            "import_warnings": report.warnings.len(),
            "unimported_attachments": report.unimported_attachments.len()
        }))
        .unwrap()
    );
    if !roundtrip_equal {
        std::process::exit(1);
    }
}
