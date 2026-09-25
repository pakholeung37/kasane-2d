use super::*;

/// The external color target supplied by the eventual wgpu host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuTargetConfig {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
}

impl Default for WgpuTargetConfig {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        }
    }
}

/// One normal draw that a basic wgpu pipeline can submit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuDraw<'a> {
    pub drawable_id: &'a str,
    pub texture_id: &'a str,
    pub index_count: u32,
}

/// Device-free result of preparing a basic wgpu frame.
#[derive(Clone, Debug, PartialEq)]
pub struct WgpuBasicFrame<'a> {
    pub prepared: PreparedFrame<'a>,
    pub target: WgpuTargetConfig,
    pub draws: Vec<WgpuDraw<'a>>,
}

/// Result of executing a normal-draw scene with optional offscreen surfaces.
#[derive(Clone, Debug, PartialEq)]
pub struct WgpuSceneFrame<'a> {
    pub prepared: PreparedFrame<'a>,
    pub target: WgpuTargetConfig,
    pub active_surface_count: usize,
}

/// Host-provided objects used for one scene submission.
pub struct WgpuSceneTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    /// The texture behind `view`, when the host can expose it.
    ///
    /// Destination-reading passes need this handle to copy the current main
    /// target into a sampled snapshot. It is optional so normal scenes can
    /// still render into an embedding API that only exposes a view.
    pub texture: Option<&'a wgpu::Texture>,
    pub surface_pool: &'a mut WgpuSurfacePool,
    pub mask_pool: &'a mut WgpuMaskPool,
    pub destination_pool: &'a mut WgpuDestinationPool,
}

/// First-stage wgpu backend entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuFramePlanner {
    pub(super) target: WgpuTargetConfig,
}

/// A host-owned texture view made available to the wgpu backend.
///
/// The host remains responsible for image decoding and texture lifetime. The
/// backend only needs the view for binding and the dimensions for the shared
/// preflight contract.
pub struct WgpuTexture<'a> {
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Texture catalog used by [`WgpuBasicRenderer`]. The catalog owns the map but
/// borrows the host-created texture views.
pub struct WgpuTextureCatalog<'a> {
    textures: HashMap<String, WgpuTexture<'a>>,
    revisions: HashMap<String, u64>,
    repeating: HashSet<String>,
}

impl<'a> WgpuTextureCatalog<'a> {
    pub fn new(textures: HashMap<String, WgpuTexture<'a>>) -> Self {
        Self {
            textures,
            revisions: HashMap::new(),
            repeating: HashSet::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&WgpuTexture<'a>> {
        self.textures.get(id)
    }

    /// Use Framework-style repeating UVs for one source texture. Other
    /// textures continue to clamp at their edges.
    pub fn set_repeat(&mut self, id: impl Into<String>, repeat: bool) {
        let id = id.into();
        if repeat {
            self.repeating.insert(id);
        } else {
            self.repeating.remove(&id);
        }
    }

    pub fn repeats(&self, id: &str) -> bool {
        self.repeating.contains(id)
    }

    /// Declare the content revision of a host texture. Reusing a revision
    /// promises that its pixels have not changed, even when the view is reused.
    /// Without a revision, dependent masks are conservatively redrawn.
    pub fn set_revision(&mut self, id: impl Into<String>, revision: u64) {
        self.revisions.insert(id.into(), revision);
    }

    pub fn revision(&self, id: &str) -> Option<u64> {
        self.revisions.get(id).copied()
    }
}

impl TextureCatalog for WgpuTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<kasane_render::TextureInfo> {
        self.get(id).map(|texture| kasane_render::TextureInfo {
            width: texture.width,
            height: texture.height,
        })
    }
}
