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

/// Texture catalog used by [`WgpuRenderer`]. The catalog owns the map but
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
