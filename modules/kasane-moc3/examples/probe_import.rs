use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let bare = args.first().map(String::as_str) == Some("--bare");
    for path in args.iter().skip(usize::from(bare)) {
        let path = Path::new(&path);
        let result = if bare {
            kasane_moc3::import_from_bare_moc3_file(path, &Default::default())
        } else {
            kasane_moc3::import_from_model3_file(path)
        };
        match result {
            Ok(result) => {
                println!(
                    "OK\t{}\t{:?}\t{:?}\ttextures_complete={}",
                    path.display(),
                    result.diagnostics,
                    result.report.warnings,
                    result.textures_complete
                );
            }
            Err(status) => {
                println!(
                    "ERROR\t{}\t{}\t{}",
                    path.display(),
                    status.code,
                    status.message
                );
            }
        }
    }
}
