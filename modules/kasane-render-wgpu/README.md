# kasane-render-wgpu

WGPU backend for Kasane render plans, including GPU resources, pipelines, and offscreen rendering.

Source textures default to straight RGBA. Hosts may upload premultiplied pixels
and mip levels, then mark each source with
`WgpuTextureCatalog::set_premultiplied(id, true)`. This prevents transparent RGB
from bleeding through linear/trilinear filtering and matches the native Metal
upload path. Alpha masks use the source alpha in either storage mode; backend
offscreen and output textures are always premultiplied.
