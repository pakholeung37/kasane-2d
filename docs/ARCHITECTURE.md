# 当前架构与渲染边界


```text
Document (kasane-core) ──> DrawableFrame ──> kasane-godot Rust renderer ──> Editor 预览
MOC3 (M6) ──> PurismCore C99 FFI ──> DrawableFrame ──> 同一 Rust renderer ──> Viewer

官方 Core + gd-cubism (C++) ──> 独立 GPU 对照基准
```

- [`kasane-core`](../modules/kasane-core/src/evaluation.rs) 管理可编辑模型并求出最终绘制帧，不依赖 Godot。
- [`kasane-godot`](../modules/kasane-godot/src/document_preview.rs) 提供 GDExtension 数据绑定、纹理存储和 M4 绘制核心。绘制核心可接受外部求值帧与纹理表；M6 再实现 PurismCore FFI 和运行帧转换。
- [`gd-cubism`](../modules/gd-cubism/src/private/internal_cubism_renderer_2d.cpp) 仍是独立的 C++ 播放器，供 [GPU 回归](../tools/validate_gpu.py) 比对；不强制接入 Rust renderer。
- [`apps/editor/`](../apps/editor/) 和 [`apps/viewer/`](../apps/viewer/) 目前是应用壳。M5/M6 的正式功能和打包验收仍按各自里程碑执行。

历史 C++ 边界与迁移记录见 [`archive/`](archive/)；不能将其描述为当前 Rust 实现的接口。
