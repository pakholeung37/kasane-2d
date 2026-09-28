# kasane-render-metal

Native macOS Metal renderer for Kasane `ScenePlan`. It calls `MTLDevice`,
`MTLRenderCommandEncoder` and `MTLBlitCommandEncoder` through `metal-rs`, and
compiles the included MSL shader directly. No wgpu device or pipeline is used
by this crate. The CPU geometry, mask layout and scene validation live in
`kasane-render` and are shared with `kasane-render-wgpu`.

Create `MetalContext` with the system device or an existing host `MTLDevice`.
Upload source images with `upload_rgba8`, build a `MetalTextureCatalog`, then
call `MetalRenderer::sync_model` (or `sync_model_shared` for an `Arc<DrawableFrame>`),
`update_view` and `encode`. The output may be
an offscreen `MTLTexture` or a `CAMetalDrawable` texture from the same device.
The host commits the command buffer after encoding and uses the **same command
queue for each renderer's lifetime**. Cached mask writes and subsequent reads
depend on queue ordering. `render` is a convenience method that commits one frame.

The renderer supports normal, additive and multiplicative drawing, raw blend
modes with destination snapshots, alpha and inverted masks, nested offscreen
targets, texture repeat and mipmapped sampling, resize, and Replace/Composite
presentation. Consecutive draws to a target share a render pass; destination
snapshots end the pass before the blit. Replace renders directly into the host
output; Composite preserves the host background with an intermediate texture.

Mesh buffers are immutable and retained across view changes. Model submissions
replace only buffers whose geometry changed, so earlier submitted frames keep
their original data. Changed meshes share packed vertex/index slabs (normally
up to 4 MiB each), avoiding a Metal allocation per drawable during deformation.
Raw masks are shared by consumers and cached across frames
using geometry, scale, texture identity/revision, and repeat mode.
Mask density rounds up to powers of two to share masks across nearby zoom levels,
falling back to exact density when the larger masks exceed the attachment budget.
Give textures
monotonically increasing revisions with `set_revision` when their contents
change. Unversioned textures redraw masks every frame. Unused mask entries are
evicted after encoding; source textures are retained while their identities are
cached. Output and offscreen textures remain per-frame allocations.

`MetalRenderStats` reports draw calls, render passes, mask redraws/cache hits and
buffer uploads. A warm pan requires no geometry upload or mask redraw.

On macOS, `kasane-sdk-observe` and the Python `observe` feature use this
native backend. Other platforms keep `kasane-render-wgpu`. Python builds
without `observe` do not include a GPU renderer.

```sh
cargo test -p kasane-render-metal --test metal_smoke --locked
cargo test -p kasane-sdk-observe --locked
```
