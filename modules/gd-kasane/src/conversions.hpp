// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/document.hpp>
#include <godot_cpp/variant/variant.hpp>
#include <godot_cpp/variant/string.hpp>
#include <godot_cpp/variant/dictionary.hpp>
#include <godot_cpp/variant/packed_vector2_array.hpp>
#include <godot_cpp/variant/packed_int64_array.hpp>
#include <godot_cpp/variant/packed_int32_array.hpp>
#include <limits>

namespace kasane_gd {
inline std::string utf8(const godot::String &s) {
    return s.utf8().get_data();
}

inline godot::String string(const std::string &s) {
    return godot::String::utf8(s.c_str());
}

inline godot::Dictionary result(const kasane::Status &status) {
    godot::Dictionary out;
    out["ok"] = status.ok();
    out["code"] = string(status.code);
    out["message"] = string(status.message);
    return out;
}

inline godot::Dictionary error(const char *code, const char *message) {
    return result(kasane::Status::error(code, message));
}

inline std::vector<kasane::Vec2> vectors(const godot::PackedVector2Array &input) {
    std::vector<kasane::Vec2> out;
    out.reserve(input.size());
    for (int64_t i = 0; i < input.size(); ++i)
        out.push_back({static_cast<float>(input[i].x), static_cast<float>(input[i].y)});
    return out;
}

inline godot::PackedVector2Array vectors(std::span<const kasane::Vec2> input) {
    godot::PackedVector2Array out;
    out.resize(input.size());
    for (size_t i = 0; i < input.size(); ++i)
        out.set(i, {input[i].x, input[i].y});
    return out;
}

inline kasane::Status ids(const godot::PackedInt64Array &input, std::vector<uint32_t> &out) {
    out.reserve(input.size());
    for (int64_t i = 0; i < input.size(); ++i) {
        if (input[i] < 0 || uint64_t(input[i]) > UINT32_MAX)
            return kasane::Status::error("INVALID_VERTEX_ID", "Vertex IDs must fit uint32.");
        out.push_back(static_cast<uint32_t>(input[i]));
    }
    return {};
}

inline godot::PackedInt64Array ids(std::span<const uint32_t> input) {
    godot::PackedInt64Array out;
    out.resize(input.size());
    for (size_t i = 0; i < input.size(); ++i)
        out.set(i, input[i]);
    return out;
}
} // namespace kasane_gd
