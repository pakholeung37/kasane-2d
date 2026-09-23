# WGPU model viewer

Render a real model in a native window after creating a validation case:

```sh
python3 tools/compare_wgpu_real_model.py
cargo run -p kasane-wgpu-viewer --locked -- target/wgpu-real-model/case.json
```

The viewer owns its window, WGPU device, queue and surface. It shares model
import, evaluation and texture upload with the independent validation host.
Resizing the window updates the output and fitted view. Press Escape or close
the window to exit. It presents GPU output directly and does not read pixels
back to the CPU each frame.

For an automated window and resize check:

```sh
cargo run -p kasane-wgpu-viewer --locked -- \
  target/wgpu-real-model/case.json --frames 2 --smoke-resize
```

This viewer is a rendering shell. It does not yet provide editor controls,
selection overlays, device-loss recovery or long-running resource tests.
