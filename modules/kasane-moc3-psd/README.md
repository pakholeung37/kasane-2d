# MOC3 to PSD

Exports the model's default parameter pose as an 8-bit RGB PSD with one raster layer per ArtMesh. ArtMeshes invisible in the default pose are exported as hidden PSD layers with usable pixels. Textures are required because `.moc3` files contain UV geometry, not image pixels. The PSD writer uses `ag-psd`: layer channels are ZIP compressed and the flattened preview is RLE compressed. A layered PSD still contains more pixels than the packed source texture, so its size need not match the texture PNG.

```sh
cargo run -p kasane-moc3-psd -- model.model3.json model.psd
cargo run -p kasane-moc3-psd -- model.moc3 model.psd --texture 0=texture_00.png --texture 1=texture_01.png
```

The crate also exposes `from_model3_file`, `from_moc3_file`, `write_model3_psd`, and `write_moc3_psd` for programmatic use. Both conversion methods reject missing or corrupt textures.

The PSD contains a flattened preview and editable ArtMesh layers, in evaluated draw order. The model's rig, parameter keyforms, masks, and offscreen effects cannot be represented as PSD layers. Masks and offscreen effects produce warnings. The output is a raster snapshot of the default pose, not an editable Live2D model.
