use kasane_core::types::Status;
use kasane_core::Document;

use crate::types::Moc3Artifact;

mod blendshapes;
mod context;
mod drawing_groups;
mod glues;
mod helpers;
mod meshes;
mod offscreens;
mod parameters;
mod parts;
mod transforms;
mod validation;

use context::Moc3EncoderContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Moc3ExportVersion {
    #[default]
    Auto,
    V50,
    V53,
}

pub fn encode_moc3(doc: &Document) -> Result<Moc3Artifact, Status> {
    encode_moc3_with_version(doc, Moc3ExportVersion::Auto)
}

pub fn encode_moc3_with_version(
    doc: &Document,
    target_version: Moc3ExportVersion,
) -> Result<Moc3Artifact, Status> {
    let (export_version, frame) = validation::validate_preflight(doc, target_version)?;
    let mut ctx = Moc3EncoderContext::new(doc, export_version, &frame.drawables)?;

    ctx.encode_initial_offscreens()?;
    ctx.encode_canvas()?;
    ctx.encode_parameters()?;
    ctx.encode_parts()?;
    ctx.encode_transforms()?;
    ctx.encode_drawing_groups()?;
    ctx.encode_art_meshes()?;
    ctx.encode_glues()?;
    ctx.encode_constraints()?;
    ctx.encode_offscreen_sources()?;
    ctx.encode_blendshapes()?;

    ctx.finish()
}

pub fn encode_moc3_into(doc: &Document, out: &mut Moc3Artifact) -> Status {
    match encode_moc3(doc) {
        Ok(artifact) => {
            *out = artifact;
            Status::ok()
        }
        Err(status) => status,
    }
}
