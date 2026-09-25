use kasane_live2d::motion3::{decode_motion3, encode_motion3};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let source = args.next().ok_or("source path required")?;
    let output = args.next().ok_or("output path required")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let motion = decode_motion3(&fs::read_to_string(source)?)?;
    fs::write(output, encode_motion3(&motion)?)?;
    Ok(())
}
