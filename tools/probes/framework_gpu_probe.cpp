// SPDX-License-Identifier: MIT
// Deterministic image oracle using the unmodified official Core + Framework.
#include <GL/glew.h>
#include <GLFW/glfw3.h>
#include <CubismFramework.hpp>
#include <ICubismAllocator.hpp>
#include <Model/CubismMoc.hpp>
#include <Model/CubismModel.hpp>
#include <Math/CubismMatrix44.hpp>
#include <Rendering/OpenGL/CubismRenderer_OpenGLES2.hpp>
#include <Rendering/OpenGL/CubismOffscreenManager_OpenGLES2.hpp>
#define STB_IMAGE_IMPLEMENTATION
#include <stb_image.h>
#include <zlib.h>
#include <algorithm>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <vector>
#include <string>
#include <stdexcept>
namespace Csm = Live2D::Cubism::Framework;
namespace Core = Live2D::Cubism::Core;
using namespace Csm::Rendering;
struct Allocator : Csm::ICubismAllocator {
    void* Allocate(Csm::csmSizeType n) override { return std::malloc(n); }
    void Deallocate(void* p) override { std::free(p); }
    void* AllocateAligned(Csm::csmSizeType n, Csm::csmUint32 a) override {
        void* p = nullptr; return posix_memalign(&p, std::max(size_t(a), sizeof(void*)), n) == 0 ? p : nullptr;
    }
    void DeallocateAligned(void* p) override { std::free(p); }
};
std::filesystem::path shaders;
std::vector<unsigned char> read_file(const std::filesystem::path& p) {
    std::ifstream f(p, std::ios::binary);
    if (!f) throw std::runtime_error("Cannot read " + p.string());
    return {std::istreambuf_iterator<char>(f), {}};
}
Csm::csmByte* load_shader(const std::string path, Csm::csmSizeInt* size) {
    auto bytes = read_file(shaders / std::filesystem::path(path).filename());
    *size = bytes.size(); auto* out = static_cast<Csm::csmByte*>(std::malloc(bytes.size()));
    std::copy(bytes.begin(), bytes.end(), out); return out;
}
void release_bytes(Csm::csmByte* p) { std::free(p); }
void log_message(const char* s) { std::cerr << s << '\n'; }
void save_image(const char* path, int width, int height) {
    std::vector<unsigned char> pixels(size_t(width) * height * 4);
    glFinish(); glReadPixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, pixels.data());
    // Minimal lossless RGBA PNG, with rows flipped out of OpenGL orientation.
    std::vector<unsigned char> rows(size_t(height) * (width * 4 + 1));
    for (int y = 0; y < height; ++y)
        std::copy_n(pixels.data() + size_t(height - 1 - y) * width * 4, width * 4, rows.data() + size_t(y) * (width * 4 + 1) + 1);
    uLongf length = compressBound(rows.size()); std::vector<unsigned char> compressed(length);
    if (compress2(compressed.data(), &length, rows.data(), rows.size(), 6) != Z_OK) throw std::runtime_error("PNG compression failed");
    compressed.resize(length); std::ofstream out(path, std::ios::binary);
    const unsigned char magic[] = {137,80,78,71,13,10,26,10}; out.write((const char*)magic, 8);
    auto be = [](unsigned char* b, unsigned n) { for (int i=3;i>=0;--i) { b[i] = n & 255; n >>= 8; } };
    auto chunk = [&](const char* tag, const std::vector<unsigned char>& data) {
        unsigned char n[4]; be(n, data.size()); out.write((char*)n,4); out.write(tag,4);
        out.write((const char*)data.data(),data.size()); auto crc = crc32(0, (const Bytef*)tag,4);
        crc = crc32(crc,data.data(),data.size()); be(n,crc); out.write((char*)n,4);
    };
    std::vector<unsigned char> header(13); be(header.data(),width); be(header.data()+4,height); header[8]=8; header[9]=6;
    chunk("IHDR",header); chunk("IDAT",compressed); chunk("IEND",{});
    if (!out) throw std::runtime_error("PNG write failed");
}
#include "blend_matrix.hpp"
int main(int argc, char** argv) {
    try {
        if (argc < 5 || (std::string(argv[2]) != "--matrix" && argc < 7)) throw std::runtime_error("Usage: probe shaders moc texture output.png size fit_pixels [parameter_index value]...");
        shaders = argv[1];
        const bool matrix_mode = std::string(argv[2]) == "--matrix";
        const std::string dimensions = matrix_mode ? "64x64" : argv[5];
        const auto split = dimensions.find('x');
        const int width = std::stoi(dimensions), height = split == std::string::npos ? width : std::stoi(dimensions.substr(split+1));
        const float fit = matrix_mode ? 64 : std::stof(argv[6]);
        if (!glfwInit()) throw std::runtime_error("glfwInit failed");
        glfwWindowHint(GLFW_VISIBLE, GLFW_FALSE);
        auto* window = glfwCreateWindow(64, 64, "Kasane official image oracle", nullptr, nullptr);
        if (!window) throw std::runtime_error("GL context failed");
        glfwMakeContextCurrent(window);
        if (glewInit() != GLEW_OK) throw std::runtime_error("glewInit failed");
        if (matrix_mode) { blend_matrix(argv[3],argv[4]); glfwDestroyWindow(window);glfwTerminate();return 0; }
        Allocator allocator; Csm::CubismFramework::Option options{};
        options.LogFunction = log_message; options.LoggingLevel = Csm::CubismFramework::Option::LogLevel_Warning;
        options.LoadFileFunction = load_shader; options.ReleaseBytesFunction = release_bytes;
        Csm::CubismFramework::StartUp(&allocator, &options); Csm::CubismFramework::Initialize();
        auto bytes = read_file(argv[2]); auto* moc = Csm::CubismMoc::Create(bytes.data(), bytes.size(), true);
        if (!moc) throw std::runtime_error("Official Core rejected MOC");
        auto* model = moc->CreateModel();
        float camera_x=0, camera_y=0, camera_scale=0;
        for (int i = 7; i + 1 < argc;) {
            if (std::string(argv[i]) == "--camera" && i+3<argc) {
                camera_x=std::stof(argv[i+1]);camera_y=std::stof(argv[i+2]);camera_scale=std::stof(argv[i+3]);i+=4;
            } else { model->SetParameterValue(std::stoi(argv[i]),std::stof(argv[i+1]));i+=2; }
        }
        model->Update();
        int tw, th, channels; auto* texels = stbi_load(argv[3], &tw, &th, &channels, 4);
        if (!texels) throw std::runtime_error("Texture decode failed");
        GLuint atlas; glGenTextures(1, &atlas); glBindTexture(GL_TEXTURE_2D, atlas);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, tw, th, 0, GL_RGBA, GL_UNSIGNED_BYTE, texels); stbi_image_free(texels);
        glGenerateMipmap(GL_TEXTURE_2D);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR_MIPMAP_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE); glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        GLuint target, fbo; glGenTextures(1, &target); glBindTexture(GL_TEXTURE_2D, target);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, width, height, 0, GL_RGBA, GL_UNSIGNED_BYTE, nullptr);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR); glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glGenFramebuffers(1, &fbo); glBindFramebuffer(GL_FRAMEBUFFER, fbo);
        glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, target, 0);
        if (glCheckFramebufferStatus(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE) throw std::runtime_error("Incomplete FBO");
        auto* renderer = static_cast<CubismRenderer_OpenGLES2*>(Csm::Rendering::CubismRenderer::Create(width, height));
        renderer->Initialize(model); renderer->SetRenderTargetSize(width, height); renderer->BindTexture(0, atlas);
        renderer->IsPremultipliedAlpha(false);
        Core::csmVector2 canvas, origin; float ppu; Core::csmReadCanvasInfo(model->GetModel(), &canvas, &origin, &ppu);
        float scale = fit / std::max(canvas.X, canvas.Y);
        Csm::CubismMatrix44 matrix;
        float px=(width-canvas.X*scale)*0.5f, py=(height-canvas.Y*scale)*0.5f;
        if(camera_scale>0){scale=camera_scale;px=camera_x;py=camera_y;}
        float m[16] = {0}; m[0]=2*ppu*scale/width; m[5]=2*ppu*scale/height; m[10]=m[15]=1;
        m[12]=2*(px+origin.X*scale)/width-1; m[13]=1-2*(py+origin.Y*scale)/height;
        matrix.SetMatrix(m); renderer->SetMvpMatrix(&matrix);
        glViewport(0, 0, width, height); glClearColor(0, 0, 0, 0); glClear(GL_COLOR_BUFFER_BIT);
        auto* manager = CubismOffscreenManager_OpenGLES2::GetInstance();
        manager->BeginFrameProcess(); renderer->DrawModel(); manager->EndFrameProcess();
        glBindFramebuffer(GL_FRAMEBUFFER, fbo); save_image(argv[4], width, height);
        std::cout << "{\"core_version\":" << Core::csmGetVersion() << ",\"offscreen_count\":" << model->GetOffscreenCount() << ",\"gl\":\"" << glGetString(GL_VERSION) << "\"}\n";
        Csm::Rendering::CubismRenderer::Delete(renderer); moc->DeleteModel(model); Csm::CubismMoc::Delete(moc);
        Csm::Rendering::CubismRenderer::StaticRelease(); CubismOffscreenManager_OpenGLES2::ReleaseInstance();
        Csm::CubismFramework::Dispose(); Csm::CubismFramework::CleanUp();
        glfwDestroyWindow(window); glfwTerminate(); return 0;
    } catch (const std::exception& e) { std::cerr << e.what() << '\n'; return 1; }
}
