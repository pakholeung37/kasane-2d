//! Small process boundary for migration acceptance, deliberately independent of Godot.
use kasane_project::DocumentSession;
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: project_roundtrip INPUT_PROJECT OUTPUT_PROJECT");
        std::process::exit(2);
    }
    let mut session = DocumentSession::new();
    let opened = session.open(Path::new(&args[1]));
    if !opened.status.is_ok() || !opened.resources_complete() {
        eprintln!("Open failed: {opened:?}");
        std::process::exit(1);
    }
    let artifact = kasane_moc3::encode_moc3(session.document()).expect("encode input");
    let saved = session.save(Path::new(&args[2]));
    if !saved.status.is_ok() {
        eprintln!("Save failed: {saved:?}");
        std::process::exit(1);
    }
    let mut reopened = DocumentSession::new();
    let result = reopened.open(Path::new(&args[2]));
    assert!(
        result.status.is_ok() && result.resources_complete(),
        "{result:?}"
    );
    assert!(session.document().same_content(reopened.document()));
    assert_eq!(
        artifact.bytes,
        kasane_moc3::encode_moc3(reopened.document()).unwrap().bytes
    );
    println!("Project reopened with identical source data and MOC3 bytes");
}
