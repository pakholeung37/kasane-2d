// SPDX-License-Identifier: MIT
#pragma once
#include "document_bridge.hpp"

namespace kasane_gd {
// Source persistence only; opening a project neither loads textures nor creates
// nodes. This prototype format is not the M2 packaged-project implementation.
class KasaneProjectIO : public godot::RefCounted {
    GDCLASS(KasaneProjectIO, godot::RefCounted)
  protected:
    static void _bind_methods();

  public:
    godot::Dictionary save_project(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
    godot::Dictionary open_project(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
};
} // namespace kasane_gd
