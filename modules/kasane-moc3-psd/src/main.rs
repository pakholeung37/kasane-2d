use kasane_moc3_psd::{write_moc3_psd, write_model3_psd};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("usage: kasane-moc3-psd <model.model3.json|model.moc3> <output.psd> [--texture SLOT=PATH ...]")?;
    let output = args.next().ok_or("missing output PSD path")?;
    let mut textures = HashMap::new();
    while let Some(option) = args.next() {
        if option != "--texture" {
            return Err(format!("unknown option {option}").into());
        }
        let value = args.next().ok_or("--texture requires SLOT=PATH")?;
        let (slot, path) = value
            .split_once('=')
            .ok_or("--texture requires SLOT=PATH")?;
        let slot: usize = slot.parse()?;
        if path.is_empty() {
            return Err("texture path must not be empty".into());
        }
        textures.insert(slot, PathBuf::from(path));
    }
    let input = Path::new(&input);
    let output = Path::new(&output);
    let report = if input
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("moc3"))
    {
        if textures.is_empty() {
            return Err("bare MOC3 requires --texture SLOT=PATH".into());
        }
        write_moc3_psd(input, &textures, output)?
    } else {
        if !textures.is_empty() {
            return Err("--texture is only valid for bare MOC3 input".into());
        }
        write_model3_psd(input, output)?
    };
    println!(
        "{}x{} PSD with {} layers: {}",
        report.width,
        report.height,
        report.layers,
        output.display()
    );
    for warning in report.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(())
}
