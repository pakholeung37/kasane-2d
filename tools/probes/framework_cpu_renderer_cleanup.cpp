// SPDX-License-Identifier: MIT
// CubismFramework::Dispose calls this backend hook. This CPU-only probe never
// creates a renderer or GPU resource, so there is nothing to release. The shim
// is linked only into the animation and CDI CPU probe executables, never the
// GPU probe or a production library.
#include <Rendering/CubismRenderer.hpp>

namespace Live2D { namespace Cubism { namespace Framework { namespace Rendering {
void CubismRenderer::StaticRelease() {}
}}}}
