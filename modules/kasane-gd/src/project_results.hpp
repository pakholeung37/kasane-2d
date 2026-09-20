// SPDX-License-Identifier: MIT
#pragma once
#include "conversions.hpp"
#include <kasane/project.hpp>

namespace kasane_gd {
inline godot::Array diagnostic_array(const std::vector<kasane::ResourceDiagnostic> &source) {
    godot::Array result;
    for (const auto &d : source) {
        godot::Dictionary item;
        item["asset_id"] = string(d.asset_id);
        item["code"] = string(d.code);
        item["message"] = string(d.message);
        result.push_back(item);
    }
    return result;
}

inline godot::Dictionary project_result(const kasane::ProjectResult &source) {
    auto out = result(source.status);
    out["diagnostics"] = diagnostic_array(source.diagnostics);
    out["resources_complete"] = source.resources_complete();
    out["published"] = source.published;
    out["durable"] = source.durable;
    godot::Array warnings;
    for (const auto &warning : source.warnings)
        warnings.push_back(string(warning));
    out["warnings"] = warnings;
    return out;
}
} // namespace kasane_gd
