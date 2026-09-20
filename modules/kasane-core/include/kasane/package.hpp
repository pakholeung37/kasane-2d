// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/moc3.hpp>
#include <filesystem>
#include <functional>

namespace kasane {
// Runs after encoding and texture decoding, before any existing output moves.
// The application supplies its runtime compatibility gate (e.g. both Cores).
using ArtifactValidator = std::function<Status(const Moc3Artifact &)>;

struct PackageOptions {
    std::filesystem::path asset_root, destination;
    ArtifactValidator validate;
};

// Filesystem adapter, deliberately separate from Document and encode_moc3.
// Publishes a whole directory; failures retain the previous package.
Status publish_package(const Document &, const PackageOptions &);
} // namespace kasane
