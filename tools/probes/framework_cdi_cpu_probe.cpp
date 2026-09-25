// SPDX-License-Identifier: MIT
// CPU-only CDI getter oracle from the unmodified Live2D Framework.
#include <CubismCdiJson.hpp>
#include <CubismFramework.hpp>
#include <ICubismAllocator.hpp>
#include <Utils/CubismJson.hpp>
#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace Csm = Live2D::Cubism::Framework;

struct Allocator final : Csm::ICubismAllocator {
    void* Allocate(Csm::csmSizeType size) override { return std::malloc(size); }
    void Deallocate(void* pointer) override { std::free(pointer); }
    void* AllocateAligned(Csm::csmSizeType size, Csm::csmUint32 alignment) override {
        void* pointer = nullptr;
        return posix_memalign(&pointer, std::max(size_t(alignment), sizeof(void*)), size) == 0
            ? pointer : nullptr;
    }
    void DeallocateAligned(void* pointer) override { std::free(pointer); }
};

struct FrameworkSession final {
    Csm::CubismFramework::Option options{};
    explicit FrameworkSession(Allocator& allocator) {
        options.LogFunction = [](const char* message) { std::cerr << message << '\n'; };
        options.LoggingLevel = Csm::CubismFramework::Option::LogLevel_Info;
        if (!Csm::CubismFramework::StartUp(&allocator, &options))
            throw std::runtime_error("Framework StartUp failed");
        Csm::CubismFramework::Initialize();
    }
    ~FrameworkSession() {
        Csm::CubismFramework::Dispose();
        Csm::CubismFramework::CleanUp();
    }
};

static std::vector<unsigned char> read_file(const char* path) {
    std::ifstream stream(path, std::ios::binary);
    if (!stream) throw std::runtime_error(std::string("Cannot read ") + path);
    std::vector<unsigned char> bytes(std::istreambuf_iterator<char>{stream}, {});
    if (bytes.empty()) throw std::runtime_error("Empty CDI file");
    return bytes;
}

static void json_string(const char* value) {
    if (!value) throw std::runtime_error("Framework returned a null CDI string");
    static constexpr char hex[] = "0123456789abcdef";
    std::cout << '"';
    for (const unsigned char* p = reinterpret_cast<const unsigned char*>(value); *p; ++p) {
        switch (*p) {
        case '"': std::cout << "\\\""; break;
        case '\\': std::cout << "\\\\"; break;
        case '\b': std::cout << "\\b"; break;
        case '\f': std::cout << "\\f"; break;
        case '\n': std::cout << "\\n"; break;
        case '\r': std::cout << "\\r"; break;
        case '\t': std::cout << "\\t"; break;
        default:
            if (*p < 0x20)
                std::cout << "\\u00" << hex[*p >> 4] << hex[*p & 0xf];
            else
                std::cout << char(*p);
        }
    }
    std::cout << '"';
}

int main(int argc, char** argv) {
    try {
        const bool loadOnly = argc == 3 && std::string(argv[1]) == "--load";
        if (argc != 2 && !loadOnly) throw std::runtime_error("usage: kasane_framework_cdi_cpu_probe [--load] CDI3.json");
        auto bytes = read_file(argv[loadOnly ? 2 : 1]);
        Allocator allocator;
        FrameworkSession session(allocator);
        Csm::CubismCdiJson cdi(bytes.data(), bytes.size());
        if (!cdi.IsValid()) {
            std::cout << "{\"accepted\":false}\n";
            return 0;
        }
        if (loadOnly) {
            std::cout << "{\"accepted\":true,\"parameters\":" << cdi.GetParametersCount()
                      << ",\"parts\":" << cdi.GetPartsCount() << "}\n";
            return 0;
        }
        // CubismCdiJson intentionally has no Version/extension getters. Read
        // those only through the same official CubismJson parser as evidence.
        std::unique_ptr<Csm::Utils::CubismJson, void(*)(Csm::Utils::CubismJson*)> raw(
            Csm::Utils::CubismJson::Create(bytes.data(), bytes.size()),
            &Csm::Utils::CubismJson::Delete);
        if (!raw) throw std::runtime_error("CDI valid but direct Framework JSON parse failed");
        std::cout << "{\"accepted\":true,\"version\":" << raw->GetRoot()["Version"].ToInt()
                  << ",\"parameters\":[";
        for (int i = 0; i < cdi.GetParametersCount(); ++i) {
            if (i) std::cout << ',';
            std::cout << "{\"id\":"; json_string(cdi.GetParametersId(i));
            std::cout << ",\"group_id\":"; json_string(cdi.GetParametersGroupId(i));
            std::cout << ",\"name\":"; json_string(cdi.GetParametersName(i));
            std::cout << '}';
        }
        std::cout << "],\"parameter_groups\":[";
        for (int i = 0; i < cdi.GetParameterGroupsCount(); ++i) {
            if (i) std::cout << ',';
            std::cout << "{\"id\":"; json_string(cdi.GetParameterGroupsId(i));
            std::cout << ",\"group_id\":"; json_string(cdi.GetParameterGroupsGroupId(i));
            std::cout << ",\"name\":"; json_string(cdi.GetParameterGroupsName(i));
            std::cout << '}';
        }
        std::cout << "],\"parts\":[";
        for (int i = 0; i < cdi.GetPartsCount(); ++i) {
            if (i) std::cout << ',';
            std::cout << "{\"id\":"; json_string(cdi.GetPartsId(i));
            std::cout << ",\"name\":"; json_string(cdi.GetPartsName(i));
            std::cout << '}';
        }
        std::cout << "],\"combined_parameters\":[";
        for (int i = 0; i < cdi.GetCombinedParametersCount(); ++i) {
            if (i) std::cout << ',';
            const auto* combination = cdi.GetCombinedParameters(i);
            if (!combination) throw std::runtime_error("Framework returned a null combination");
            std::cout << '[';
            for (int j = 0; j < combination->GetSize(); ++j) {
                if (j) std::cout << ',';
                json_string((*combination)[j]->GetRawString());
            }
            std::cout << ']';
        }
        std::cout << "],\"parameter_hint\":";
        json_string(raw->GetRoot()["Parameters"][0]["Hint"].GetRawString());
        std::cout << ",\"future_label\":";
        json_string(raw->GetRoot()["Future"]["Label"].GetRawString());
        auto& futureEnabled = raw->GetRoot()["Future"]["Enabled"];
        if (!futureEnabled.IsBool()) throw std::runtime_error("Framework did not read extension bool");
        std::cout << ",\"future_enabled\":" << (futureEnabled.ToBoolean() ? "true" : "false");
        std::cout << "}\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << "CDI probe failed: " << error.what() << '\n';
        return 2;
    }
}
