// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/document.hpp>
#include <string_view>

namespace kasane {
Status encode_project(const Document &, std::string &);
// Decodes through public editing APIs; output remains unchanged on every failure.
Status decode_project(std::string_view, Document &);
} // namespace kasane
