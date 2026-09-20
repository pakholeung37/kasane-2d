// SPDX-License-Identifier: MIT
#include "document_bridge.hpp"
#include "project_io.hpp"
#include "document_preview.hpp"
#include <godot_cpp/godot.hpp>

using namespace godot;
using namespace kasane_gd;

static void initialize(ModuleInitializationLevel level) {
    if (level != MODULE_INITIALIZATION_LEVEL_SCENE)
        return;
    GDREGISTER_CLASS(KasaneMeshView);
    GDREGISTER_CLASS(KasaneDocumentState);
    GDREGISTER_CLASS(KasaneMeshData);
    GDREGISTER_CLASS(KasaneDeformerData);
    GDREGISTER_CLASS(KasaneDocumentBridge);
    GDREGISTER_CLASS(KasaneProjectIO);
    GDREGISTER_CLASS(KasaneTextureStore);
    GDREGISTER_CLASS(KasaneDocumentPreview);
}

static void terminate(ModuleInitializationLevel) {
}

extern "C" {
GDExtensionBool GDE_EXPORT kasane_gd_library_init(GDExtensionInterfaceGetProcAddress get_proc_address,
                                                  GDExtensionClassLibraryPtr library,
                                                  GDExtensionInitialization *initialization) {
    GDExtensionBinding::InitObject init(get_proc_address, library, initialization);
    init.register_initializer(initialize);
    init.register_terminator(terminate);
    init.set_minimum_library_initialization_level(MODULE_INITIALIZATION_LEVEL_SCENE);
    return init.init();
}
}
