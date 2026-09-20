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
            if let Ok(slot) = k.try_to::<i64>() {
                if let Ok(tex_path) = v.try_to::<GString>() {
                    tex_map.insert(slot as usize, PathBuf::from(tex_path.to_string()));
                }
            }
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
}
