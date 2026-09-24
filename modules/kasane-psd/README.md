# PSD import module

`kasane-psd` converts a layered, 8-bit RGB PSD into a new in-memory Kasane `Document` and a list of PNG assets. It does not write files or replace a live editing session.

```rust
let bundle = kasane_psd::import_psd(&std::fs::read("art.psd")?)?;
// Publish each bundle.assets[i].bytes at bundle.assets[i].source,
// relative to the destination project root, before opening/saving the document.
let document = bundle.document;
```

Raster layers become cropped PNG assets and rectangular meshes. The PSD canvas, layer names, positions, stacking order, visibility, opacity, supported blend modes, and group hierarchy are retained. Object IDs and PNG paths are deterministic for identical input bytes. This module does not infer a rig, parameters, masks, or deformers.

Current supported blend modes are normal, multiply, and linear dodge (additive). Clipping, layer masks, vector masks, adjustments, layer effects, group opacity, and unsupported blend modes return `UNSUPPORTED_LAYER`. Text, vector, and placed layers with available raster pixels import as raster images and produce warnings. Empty or pixel-less layers are rejected. PSD/PSB features outside 8-bit RGB PSD version 1 are rejected. Input and decoded-image limits guard memory use.

The returned asset paths are relative. `kasane-project` publishes the PNG files and manifest together through `DocumentSession::import_psd_authoring`; `kasane-sdk` and Python expose this as `import_psd(source, destination)`. The destination must be a new project directory. A failed import leaves the active project untouched.
