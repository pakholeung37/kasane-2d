//! Re-encode an expression fixture for the local Framework compatibility probe.
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or("expected input path")?;
    let output = args.next().ok_or("expected output path")?;
    if args.next().is_some() {
        return Err("expected only input and output paths".into());
    }
    let decoded = kasane_live2d::exp3::decode_exp3(&std::fs::read_to_string(Path::new(&input))?)?;
    std::fs::write(
        Path::new(&output),
        kasane_live2d::exp3::encode_exp3(&decoded)?,
    )?;
    Ok(())
}
