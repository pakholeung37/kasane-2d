# kasane-render-metal

Native macOS Metal renderer for Kasane `ScenePlan`. It calls `MTLDevice`,
`MTLRenderCommandEncoder` and `MTLBlitCommandEncoder` through `metal-rs`, and
compiles the included MSL shader directly. No wgpu device or pipeline is used
by this crate. The CPU geometry, mask layout and scene validation live in
`kasane-render` and are shared with `kasane-render-wgpu`.

Create `MetalContext` with the system device or an existing host `MTLDevice`.
Upload source images with `upload_rgba8`, build a `MetalTextureCatalog`, then
call `MetalRenderer::sync_model`, `update_view` and `encode`. The output may be
an offscreen `MTLTexture` or a `CAMetalDrawable` texture from the same device.
The host commits the command buffer after encoding; `render` is a convenience
method that commits one frame.

The renderer supports normal, additive and multiplicative drawing, raw blend
modes with destination snapshots, alpha and inverted masks, nested offscreen
targets, texture repeat and mipmapped sampling, resize, and Replace/Composite
presentation. The native path currently rebuilds its per-frame geometry and
attachments rather than caching them across frames.

On macOS, `kasane-sdk-observe` and the Python `observe` feature use this
native backend. Other platforms keep `kasane-render-wgpu`. Python builds
without `observe` do not include a GPU renderer.

```sh
cargo test -p kasane-render-metal --test metal_smoke --locked
cargo test -p kasane-sdk-observe --locked
```
