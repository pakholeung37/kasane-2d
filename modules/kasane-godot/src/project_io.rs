use godot::prelude::*;
use std::path::PathBuf;

use kasane_core::types::Status;
use kasane_project::store::ProjectResult;

use crate::conversions::{is_main_thread, project_result_dict, status_to_dict, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct KasaneProjectIO {
    base: Base<RefCounted>,
}

#[godot_api]
impl KasaneProjectIO {
    fn ready(owner: Option<&Gd<KasaneDocumentBridge>>) -> Result<(), Status> {
        if owner.is_none() {
            return Err(Status::error("MISSING_DOCUMENT", "Provide a Document."));
        }
        if !is_main_thread() {
            return Err(Status::error(
                "WRONG_THREAD",
                "Document binding requires the main thread.",
            ));
        }
        Ok(())
    }

    #[func]
    pub fn save_project(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        path: GString,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let mut doc = document.unwrap();
        let path_buf = PathBuf::from(path.to_string());
        let saved = doc.bind_mut().session_mut().save(&path_buf);
        let mut out = project_result_dict(&saved);
        if saved.status.is_ok() {
            let manifest_str = doc
                .bind()
                .session()
                .manifest()
                .to_string_lossy()
                .to_string();
            let path_str = GString::from(manifest_str.as_str());
            out.set("path", &path_str);
            doc.bind_mut()
                .base_mut()
                .emit_signal("changed", &[out.to_variant()]);
        }
        out
    }

    #[func]
    pub fn open_project(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        path: GString,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let mut doc = document.unwrap();
        let path_buf = PathBuf::from(path.to_string());
        let opened = doc.bind_mut().session_mut().open(&path_buf);
        let mut out = project_result_dict(&opened);
        if opened.status.is_ok() {
            doc.bind_mut().increment_generation();
            doc.bind_mut().preview_values_mut().clear();
            let manifest_str = doc
                .bind()
                .session()
                .manifest()
                .to_string_lossy()
                .to_string();
            let rev = doc.bind().session().document().revision() as i64;
            let path_str = GString::from(manifest_str.as_str());
            out.set("path", &path_str);
            out.set("revision", rev);
            doc.bind_mut()
                .base_mut()
                .emit_signal("changed", &[out.to_variant()]);
        }
        out
    }

    #[func]
    pub fn diagnose_resources(&mut self, document: Option<Gd<KasaneDocumentBridge>>) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let doc = document.unwrap();
        let diags = doc.bind().session().diagnose();
        let report = ProjectResult {
            status: Status::ok(),
            diagnostics: diags,
            ..Default::default()
        };
        project_result_dict(&report)
    }

    #[func]
    pub fn relocate_asset(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        asset_id: GString,
        path: GString,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let mut doc = document.unwrap();
        let p = PathBuf::from(path.to_string());
        let edit = doc
            .bind_mut()
            .session_mut()
            .relocate_asset(&asset_id.to_string(), &p);
        let res = doc.bind_mut().apply(edit);
        res
    }

    #[func]
    pub fn replace_asset(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        asset_id: GString,
        path: GString,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let mut doc = document.unwrap();
        let p = PathBuf::from(path.to_string());
        let edit = doc
            .bind_mut()
            .session_mut()
            .replace_asset(&asset_id.to_string(), &p);
        let res = doc.bind_mut().apply(edit);
        res
    }

    #[func]
    pub fn export_package(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        path: GString,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let doc = document.unwrap();
        let p = PathBuf::from(path.to_string());
        let res = doc.bind().session().export_package(&p);
        project_result_dict(&res)
    }

    #[func]
    pub fn import_model3(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        path: GString,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let mut doc = document.unwrap();
        let path_buf = PathBuf::from(path.to_string());
        let (imported, report) = doc.bind_mut().session_mut().import_model3(&path_buf);
        let mut out = project_result_dict(&imported);
        if imported.status.is_ok() {
            doc.bind_mut().increment_generation();
            doc.bind_mut().preview_values_mut().clear();
            let rev = doc.bind().session().document().revision() as i64;
            out.set("revision", rev);
            if let Some(rep) = report {
                out.set("moc_version", rep.moc_version as i64);
                let mut unimported_arr = Array::<Variant>::new();
                for s in rep.unimported_attachments {
                    unimported_arr.push(&GString::from(s.as_str()).to_variant());
                }
                out.set("unimported_attachments", &unimported_arr);
            }
            doc.bind_mut()
                .base_mut()
                .emit_signal("changed", &[out.to_variant()]);
        }
        out
    }

    #[func]
    pub fn import_moc3(
        &mut self,
        document: Option<Gd<KasaneDocumentBridge>>,
        path: GString,
        texture_map: Dictionary,
    ) -> Dictionary {
        if let Err(s) = Self::ready(document.as_ref()) {
            return status_to_dict(&s);
        }
        let mut doc = document.unwrap();
        let path_buf = PathBuf::from(path.to_string());
        let mut tex_map = std::collections::HashMap::new();
        for (k, v) in texture_map.iter_shared() {
            let Ok(slot) = k.try_to::<i64>() else {
                return status_to_dict(&Status::error("INVALID_TEXTURE_MAP", "Texture slots must be nonnegative integers."));
            };
            let Ok(tex_path) = v.try_to::<GString>() else {
                return status_to_dict(&Status::error("INVALID_TEXTURE_MAP", "Texture paths must be strings."));
            };
            if slot < 0 || usize::try_from(slot).is_err() {
                return status_to_dict(&Status::error("INVALID_TEXTURE_MAP", "Texture slot is out of range."));
            }
            tex_map.insert(slot as usize, PathBuf::from(tex_path.to_string()));
        }
        let (imported, report) = doc.bind_mut().session_mut().import_bare_moc3(&path_buf, &tex_map);
        let mut out = project_result_dict(&imported);
        if imported.status.is_ok() {
            doc.bind_mut().increment_generation();
            doc.bind_mut().preview_values_mut().clear();
            let rev = doc.bind().session().document().revision() as i64;
            out.set("revision", rev);
            if let Some(rep) = report {
                out.set("moc_version", rep.moc_version as i64);
            }
            doc.bind_mut()
                .base_mut()
                .emit_signal("changed", &[out.to_variant()]);
        }
        out
    }

    #[func]
    pub fn inspect_model(&mut self, path: GString) -> Dictionary {
        let path_str = path.to_string();
        let path_buf = PathBuf::from(&path_str);
        let moc_path = if path_str.ends_with(".model3.json") {
            match std::fs::read_to_string(&path_buf) {
                Ok(content) => {
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
                    match parsed {
                        Ok(json) => {
                            if let Some(moc_rel) = json
                                .get("FileReferences")
                                .and_then(|f| f.get("Moc"))
                                .and_then(|m| m.as_str())
                            {
                                path_buf.parent().unwrap_or(&path_buf).join(moc_rel)
                            } else {
                                return status_to_dict(&Status::error(
                                    "INVALID_MODEL3_JSON",
                                    "Missing FileReferences.Moc",
                                ));
                            }
                        }
                        Err(e) => {
                            return status_to_dict(&Status::error(
                                "INVALID_MODEL3_JSON",
                                e.to_string(),
                            ))
                        }
                    }
                }
                Err(e) => return status_to_dict(&Status::error("IO_ERROR", e.to_string())),
            }
        } else {
            path_buf
        };

        let bytes = match std::fs::read(&moc_path) {
            Ok(b) => b,
            Err(e) => return status_to_dict(&Status::error("MOC3_IO_ERROR", e.to_string())),
        };

        match kasane_moc3::inspect_moc3_safety(&bytes) {
            Ok(report) => {
                let mut out = Dictionary::new();
                out.set("ok", true);
                out.set("code", "");
                out.set("message", "");
                out.set("version", report.version_number as i64);
                out.set("is_compatible", report.unsupported_features.is_empty());

                let mut canvas_dict = Dictionary::new();
                canvas_dict.set("width", report.canvas.width as f64);
                canvas_dict.set("height", report.canvas.height as f64);
                canvas_dict.set("origin_x", report.canvas.origin_x as f64);
                canvas_dict.set("origin_y", report.canvas.origin_y as f64);
                canvas_dict.set("pixels_per_unit", report.canvas.pixels_per_unit as f64);
                canvas_dict.set("flag", report.canvas.flag as i64);
                out.set("canvas", &canvas_dict.to_variant());

                let mut counts_dict = Dictionary::new();
                counts_dict.set("parts", report.counts.parts as i64);
                counts_dict.set("deformers", report.counts.deformers as i64);
                counts_dict.set("warps", report.counts.warps as i64);
                counts_dict.set("rotations", report.counts.rotations as i64);
                counts_dict.set("art_meshes", report.counts.art_meshes as i64);
                counts_dict.set("parameters", report.counts.parameters as i64);
                counts_dict.set("glues", report.counts.glues as i64);
                counts_dict.set("glue_info", report.counts.glue_info as i64);
                counts_dict.set("blend_bindings", report.counts.blend_bindings as i64);
                counts_dict.set("blend_key_tables", report.counts.blend_key_tables as i64);
                counts_dict.set("bs_warps", report.counts.bs_warps as i64);
                counts_dict.set("bs_art_meshes", report.counts.bs_art_meshes as i64);
                counts_dict.set("bs_parts", report.counts.bs_parts as i64);
                counts_dict.set("bs_rotations", report.counts.bs_rotations as i64);
                counts_dict.set("bs_constraints", report.counts.bs_constraints as i64);
                counts_dict.set("bs_glues", report.counts.bs_glues as i64);
                counts_dict.set("offscreens", report.counts.offscreens as i64);
                out.set("counts", &counts_dict.to_variant());

                let mut issues_arr = Array::<Variant>::new();
                for issue in &report.unsupported_features {
                    let mut issue_dict = Dictionary::new();
                    issue_dict.set(
                        "category",
                        &GString::from(issue.category.as_str()).to_variant(),
                    );
                    issue_dict.set("count", issue.count as i64);
                    issue_dict.set("detail", &GString::from(issue.detail.as_str()).to_variant());
                    issues_arr.push(&issue_dict.to_variant());
                }
                out.set("unsupported_features", &issues_arr);
                out
            }
            Err(s) => status_to_dict(&s),
        }
    }
}
