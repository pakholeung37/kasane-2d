//! Add a static Offscreen to the synthetic animation model for GPU reference.
use std::{collections::HashMap, env, fs};

use kasane_core::{Offscreen, OffscreenKeyform};
use kasane_moc3::{encode_moc3, import_from_bare_moc3};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let source = args.next().ok_or("source MOC3 path required")?;
    let output = args.next().ok_or("output MOC3 path required")?;
    let nested = match args.next().as_deref() {
        None => false,
        Some("--nested") => true,
        Some(_) => return Err("expected optional --nested flag".into()),
    };
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let bytes = fs::read(source)?;
    let mut document = import_from_bare_moc3(&bytes, &HashMap::new())
        .map_err(|status| format!("{}: {}", status.code, status.message))?
        .document;
    let owner = document
        .part_order()
        .first()
        .ok_or("fixture has no Part")?
        .clone();
    if nested {
        let child_id = document
            .part_order()
            .get(1)
            .ok_or("fixture has no second Part")?;
        let mut child = document
            .get_part(child_id)
            .ok_or("missing second Part")?
            .clone();
        child.parent_id = owner.clone();
        let result = document.replace_part(child);
        if !result.status.is_ok() {
            return Err(format!("{}: {}", result.status.code, result.status.message).into());
        }
    }
    let result = document.create_offscreen(Offscreen {
        id: "00000000-0000-4000-8000-00000000f501".into(),
        runtime_id: "Offscreen0".into(),
        name: "Offscreen 0".into(),
        part_id: owner,
        keyforms: vec![OffscreenKeyform {
            opacity: 0.5,
            multiply: Some([1.0, 1.0, 1.0]),
            screen: Some([0.0, 0.0, 0.0]),
        }],
        ..Default::default()
    });
    if !result.status.is_ok() {
        return Err(format!("{}: {}", result.status.code, result.status.message).into());
    }
    if nested {
        let child_id = document
            .part_order()
            .get(1)
            .ok_or("fixture has no second Part")?
            .clone();
        let result = document.create_offscreen(Offscreen {
            id: "00000000-0000-4000-8000-00000000f502".into(),
            runtime_id: "Offscreen1".into(),
            name: "Offscreen 1".into(),
            part_id: child_id,
            keyforms: vec![OffscreenKeyform {
                opacity: 0.7,
                multiply: Some([1.0, 1.0, 1.0]),
                screen: Some([0.0, 0.0, 0.0]),
            }],
            ..Default::default()
        });
        if !result.status.is_ok() {
            return Err(format!("{}: {}", result.status.code, result.status.message).into());
        }
    }
    fs::write(
        output,
        encode_moc3(&document)
            .map_err(|status| format!("{}: {}", status.code, status.message))?
            .bytes,
    )?;
    Ok(())
}
