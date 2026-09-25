use super::*;
use kasane_core::document::{
    MotionClip, MotionEvent, MotionGroup, MotionPoint, MotionSegment, MotionTrack,
};
use serde::de::DeserializeOwned;

fn parse<T: DeserializeOwned>(text: &str, failed: &mut bool) -> PyResult<T> {
    serde_json::from_str(text).map_err(|error| {
        *failed = true;
        PyValueError::new_err(error.to_string())
    })
}

#[pymethods]
impl NativeEdit {
    #[pyo3(signature = (id, name, duration, fps, looping=false, restricted_beziers=true, fade_in=None, fade_out=None))]
    #[allow(clippy::too_many_arguments)]
    fn create_motion(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        duration: f32,
        fps: f32,
        looping: bool,
        restricted_beziers: bool,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_motion")?;
        self.commands
            .push(Command::CreateMotion(MotionClip {
                id,
                name,
                duration,
                fps,
                looping,
                restricted_beziers,
                fade_in,
                fade_out,
                tracks: Vec::new(),
                events: Vec::new(),
                extensions: Default::default(),
                meta_extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            }))
            .map_err(|error| sdk_failure(py, error))
    }

    fn replace_motion_json(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        self.ensure_open(py, "replace_motion_json")?;
        let clip: MotionClip = parse(value, &mut self.failed)?;
        if clip.has_extensions() {
            self.failed = true;
            return Err(edit_failure(
                py,
                "OPAQUE_EDIT_REQUIRES_IMPORT",
                "replace_motion_json",
                "Reimport motion3 to edit a clip with unknown fields",
            ));
        }
        self.commands
            .push(Command::ReplaceMotion(clip))
            .map_err(|error| sdk_failure(py, error))
    }

    fn import_motion3(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        text: &str,
    ) -> PyResult<Vec<(String, String, String)>> {
        self.ensure_open(py, "import_motion3")?;
        let result = self
            .commands
            .workspace
            .as_mut()
            .expect("edit is open")
            .import_motion3(id, name, text);
        match result {
            Ok(diagnostics) => Ok(diagnostics
                .into_iter()
                .map(|item| (item.code, item.path, item.message))
                .collect()),
            Err(error) => {
                self.commands.error = Some(error.clone());
                Err(sdk_failure(py, error))
            }
        }
    }

    fn set_motion_groups_json(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        self.ensure_open(py, "set_motion_groups_json")?;
        let groups: Vec<MotionGroup> = parse(value, &mut self.failed)?;
        self.commands
            .push(Command::SetMotionGroups(groups))
            .map_err(|error| sdk_failure(py, error))
    }

    fn create_motion_track_json(
        &mut self,
        py: Python<'_>,
        id: String,
        value: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_motion_track_json")?;
        let track: MotionTrack = parse(value, &mut self.failed)?;
        self.commands
            .push(Command::CreateMotionTrack(id, track))
            .map_err(|error| sdk_failure(py, error))
    }

    fn replace_motion_track_json(
        &mut self,
        py: Python<'_>,
        id: String,
        value: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_motion_track_json")?;
        let track: MotionTrack = parse(value, &mut self.failed)?;
        self.commands
            .push(Command::ReplaceMotionTrack(id, track))
            .map_err(|error| sdk_failure(py, error))
    }

    fn set_motion_segment_json(
        &mut self,
        py: Python<'_>,
        id: String,
        track_id: String,
        index: usize,
        value: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_motion_segment_json")?;
        let segment: MotionSegment = parse(value, &mut self.failed)?;
        self.commands
            .push(Command::SetMotionSegment(id, track_id, index, segment))
            .map_err(|error| sdk_failure(py, error))
    }

    fn insert_motion_segment_json(
        &mut self,
        py: Python<'_>,
        id: String,
        track_id: String,
        index: usize,
        value: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "insert_motion_segment_json")?;
        let segment: MotionSegment = parse(value, &mut self.failed)?;
        self.commands
            .push(Command::InsertMotionSegment(id, track_id, index, segment))
            .map_err(|error| sdk_failure(py, error))
    }

    fn move_motion_key(
        &mut self,
        py: Python<'_>,
        id: String,
        track_id: String,
        index: usize,
        time: f32,
        value: f32,
    ) -> PyResult<()> {
        self.ensure_open(py, "move_motion_key")?;
        self.commands
            .push(Command::MoveMotionKey(
                id,
                track_id,
                index,
                MotionPoint { time, value },
            ))
            .map_err(|error| sdk_failure(py, error))
    }

    fn set_motion_event_json(&mut self, py: Python<'_>, id: String, value: &str) -> PyResult<()> {
        self.ensure_open(py, "set_motion_event_json")?;
        let event: MotionEvent = parse(value, &mut self.failed)?;
        self.commands
            .push(Command::SetMotionEvent(id, event))
            .map_err(|error| sdk_failure(py, error))
    }

    fn remove_motion_track(
        &mut self,
        py: Python<'_>,
        id: String,
        track_id: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "remove_motion_track")?;
        self.commands
            .push(Command::RemoveMotionTrack(id, track_id))
            .map_err(|error| sdk_failure(py, error))
    }

    fn remove_motion_event(
        &mut self,
        py: Python<'_>,
        id: String,
        event_id: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "remove_motion_event")?;
        self.commands
            .push(Command::RemoveMotionEvent(id, event_id))
            .map_err(|error| sdk_failure(py, error))
    }

    #[pyo3(signature = (id, duration, fps, looping, fade_in=None, fade_out=None))]
    #[allow(clippy::too_many_arguments)]
    fn set_motion_timing(
        &mut self,
        py: Python<'_>,
        id: String,
        duration: f32,
        fps: f32,
        looping: bool,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_motion_timing")?;
        self.commands
            .push(Command::SetMotionTiming(
                id, duration, fps, looping, fade_in, fade_out,
            ))
            .map_err(|error| sdk_failure(py, error))
    }
}
