use super::*;

#[pymethods]
impl NativeEdit {
    fn add_png_asset_from_base(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        base: &str,
        relative: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "add_png_asset_from_base")?;
        let (id, name, base, relative) = (
            id.to_owned(),
            name.to_owned(),
            base.to_owned(),
            relative.to_owned(),
        );
        match py.detach(move || {
            prepare_png_asset_from_base(&id, &name, Path::new(&base), Path::new(&relative))
        }) {
            Ok(asset) => {
                self.commands
                    .push(Command::AddPng(asset))
                    .map_err(|error| sdk_failure(py, error))?;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn replace_png_asset(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        path: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_png_asset")?;
        let (id, name, path) = (id.to_owned(), name.to_owned(), path.to_owned());
        match py.detach(move || prepare_png_asset(&id, &name, Path::new(&path))) {
            Ok(asset) => {
                self.commands
                    .push(Command::ReplaceAsset(asset))
                    .map_err(|error| sdk_failure(py, error))?;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn relocate_png_asset(&mut self, py: Python<'_>, id: &str, path: &str) -> PyResult<()> {
        self.ensure_open(py, "relocate_png_asset")?;
        let original = self.commands.candidate_document().get_asset(id).cloned();
        let Some(original) = original else {
            self.failed = true;
            return Err(edit_failure(
                py,
                "MISSING_ASSET",
                "relocate_png_asset",
                "Asset does not exist",
            ));
        };
        let path = path.to_owned();
        match py.detach(move || prepare_relocated_asset(&original, Path::new(&path))) {
            Ok(asset) => {
                self.commands
                    .push(Command::ReplaceAsset(asset))
                    .map_err(|error| sdk_failure(py, error))?;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn add_png_asset(&mut self, py: Python<'_>, id: &str, name: &str, path: &str) -> PyResult<()> {
        self.ensure_open(py, "add_png_asset")?;
        let id = id.to_owned();
        let name = name.to_owned();
        let path = path.to_owned();
        match py.detach(move || prepare_png_asset(&id, &name, Path::new(&path))) {
            Ok(asset) => {
                self.commands
                    .push(Command::AddPng(asset))
                    .map_err(|error| sdk_failure(py, error))?;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }
}
