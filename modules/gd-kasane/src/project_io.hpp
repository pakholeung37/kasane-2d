// SPDX-License-Identifier: MIT
#pragma once
#include "document_bridge.hpp"

namespace kasane_gd {
class KasaneProjectIO : public godot::RefCounted {
    GDCLASS(KasaneProjectIO, godot::RefCounted)
  protected:
    static void _bind_methods();

  public:
    godot::Dictionary save_project(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
    godot::Dictionary open_project(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
    godot::Dictionary diagnose_resources(const godot::Ref<KasaneDocumentBridge> &);
    godot::Dictionary relocate_asset(const godot::Ref<KasaneDocumentBridge> &, const godot::String &,
                                     const godot::String &);
    godot::Dictionary replace_asset(const godot::Ref<KasaneDocumentBridge> &, const godot::String &,
                                    const godot::String &);
    godot::Dictionary export_package(const godot::Ref<KasaneDocumentBridge> &, const godot::String &);
};
} // namespace kasane_gd
