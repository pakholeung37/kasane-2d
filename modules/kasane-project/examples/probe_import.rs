use kasane_project::DocumentSession;
use std::path::Path;

fn main() {
    for path in std::env::args().skip(1) {
        let mut session = DocumentSession::new();
        let (result, report) = session.import_model3(Path::new(&path));
        if result.status.is_ok() {
            println!(
                "OK\t{}\t{:?}\t{:?}\t{:?}",
                path,
                result.diagnostics,
                result.warnings,
                report.map(|report| report.unimported_attachments)
            );
        } else {
            println!(
                "ERROR\t{}\t{}\t{}",
                path, result.status.code, result.status.message
            );
        }
    }
}
