// SPDX-License-Identifier: MIT
// CPU-only reference trace from the unmodified Live2D Framework and Core.
#include <CubismFramework.hpp>
#include <ICubismAllocator.hpp>
#include <Id/CubismIdManager.hpp>
#include <Model/CubismMoc.hpp>
#include <Model/CubismModel.hpp>
#include <Motion/CubismMotion.hpp>
#include <Motion/CubismMotionJson.hpp>
#include <Motion/CubismExpressionMotion.hpp>
#include <Motion/CubismExpressionMotionManager.hpp>
#include <Motion/CubismMotionManager.hpp>
#include <Effect/CubismPose.hpp>
#include <Physics/CubismPhysics.hpp>
#include <Physics/CubismPhysicsJson.hpp>
#include <CubismModelSettingJson.hpp>
#include <Utils/CubismJson.hpp>
#include <algorithm>
#include <cmath>
#include <cstring>
#include <cstdlib>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <memory>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

namespace Csm = Live2D::Cubism::Framework;

struct Allocator : Csm::ICubismAllocator {
    void* Allocate(Csm::csmSizeType n) override { return std::malloc(n); }
    void Deallocate(void* p) override { std::free(p); }
    void* AllocateAligned(Csm::csmSizeType n, Csm::csmUint32 a) override {
        void* p = nullptr;
        return posix_memalign(&p, std::max(size_t(a), sizeof(void*)), n) == 0 ? p : nullptr;
    }
    void DeallocateAligned(void* p) override { std::free(p); }
};

struct FrameworkSession {
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

struct ModelDeleter {
    Csm::CubismMoc* moc;
    void operator()(Csm::CubismModel* model) const { if (model) moc->DeleteModel(model); }
};

static std::vector<unsigned char> read_file(const char* path) {
    std::ifstream stream(path, std::ios::binary);
    if (!stream) throw std::runtime_error(std::string("Cannot read ") + path);
    std::vector<unsigned char> bytes(std::istreambuf_iterator<char>{stream}, {});
    if (bytes.empty()) throw std::runtime_error(std::string("Empty file ") + path);
    return bytes;
}

static void value(float v) {
    if (!std::isfinite(v)) throw std::runtime_error("Framework produced a non-finite value");
    std::cout << v;
}

static void frame(Csm::CubismModel* model, Csm::CubismIdHandle part0,
                  Csm::CubismIdHandle part1, const char* stage, float time) {
    const int firstIndex = model->GetParameterIndex(part0);
    const int secondIndex = model->GetParameterIndex(part1);
    std::cout << "{\"stage\":\"" << stage << "\",\"time\":";
    value(time);
    std::cout << ",\"part0_control\":{" << "\"index\":" << firstIndex << ",\"value\":";
    value(model->GetParameterValue(firstIndex));
    std::cout << "},\"part1_control\":{" << "\"index\":" << secondIndex << ",\"value\":";
    value(model->GetParameterValue(secondIndex));
    std::cout << "},\"part_opacities\":[";
    value(model->GetPartOpacity(model->GetPartIndex(part0)));
    std::cout << ',';
    value(model->GetPartOpacity(model->GetPartIndex(part1)));
    std::cout << ']';
    std::cout << ",\"model_opacity\":";
    value(model->GetModelOpacity());
    std::cout << ",\"parameters\":[";
    for (int i = 0; i < model->GetParameterCount(); ++i) {
        if (i) std::cout << ',';
        value(model->GetParameterValue(i));
    }
    std::cout << "],\"drawable_opacities\":[";
    for (int i = 0; i < model->GetDrawableCount(); ++i) {
        if (i) std::cout << ',';
        value(model->GetDrawableOpacity(i));
    }
    std::cout << "]}";
}

int main(int argc, char** argv) {
    try {
        if (argc == 3 && std::string(argv[1]) == "--motion-json-check") {
            auto bytes = read_file(argv[2]);
            Allocator allocator;
            FrameworkSession session(allocator);
            Csm::CubismMotionJson motion(bytes.data(), bytes.size());
            if (!motion.IsValid()) throw std::runtime_error("Framework rejected motion JSON");
            std::cout << "{\"consistent\":" << (motion.HasConsistency() ? "true" : "false")
                      << ",\"curves\":" << motion.GetMotionCurveCount()
                      << ",\"segments\":" << motion.GetMotionTotalSegmentCount()
                      << ",\"points\":" << motion.GetMotionTotalPointCount()
                      << ",\"events\":" << motion.GetEventCount()
                      << ",\"event_bytes\":" << motion.GetTotalEventValueSize();
            if (motion.GetEventCount() > 0) {
                std::cout << ",\"first_event_actual_bytes\":" << std::strlen(motion.GetEventValue(0))
                          << ",\"first_event_time\":";
                value(motion.GetEventTime(0));
            }
            std::cout << "}\n";
            return 0;
        }
        if (argc == 5 && std::string(argv[1]) == "--expression-mix") {
            auto mocBytes = read_file(argv[2]);
            auto firstBytes = read_file(argv[3]);
            auto secondBytes = read_file(argv[4]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected expression MOC");
            std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
            if (!model || model->GetParameterCount() != 2)
                throw std::runtime_error("Expression MOC does not contain two parameters");
            Csm::CubismExpressionMotionManager manager;
            auto* first = Csm::CubismExpressionMotion::Create(firstBytes.data(), firstBytes.size());
            auto* second = Csm::CubismExpressionMotion::Create(secondBytes.data(), secondBytes.size());
            if (!first || !second) throw std::runtime_error("Framework rejected expression files");
            manager.StartMotion(first, true);
            std::cout << std::setprecision(9) << "{\"frames\":[";
            float time = 0.0f;
            bool firstFrame = true;
            for (int index = 0; index < 7; ++index) {
                if (index == 3) manager.StartMotion(second, true);
                float dt = index == 0 ? 0.0f : 0.125f;
                time += dt;
                model->SetParameterValue(0, 0.0f);
                model->SetParameterValue(1, 0.5f);
                manager.UpdateMotion(model.get(), dt);
                if (!firstFrame) std::cout << ',';
                firstFrame = false;
                std::cout << "{\"time\":";
                value(time);
                std::cout << ",\"param_x\":";
                value(model->GetParameterValue(0));
                std::cout << ",\"param_y\":";
                value(model->GetParameterValue(1));
                std::cout << '}';
            }
            std::cout << "]}\n";
            manager.StopAllMotions();
            return 0;
        }
        if (argc == 3 && std::string(argv[1]) == "--expression-check") {
            auto bytes = read_file(argv[2]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismExpressionMotion, void(*)(Csm::ACubismMotion*)> expression(
                Csm::CubismExpressionMotion::Create(bytes.data(), bytes.size()),
                &Csm::ACubismMotion::Delete);
            if (!expression) throw std::runtime_error("Framework expression allocation failed");
            auto parameters = expression->GetExpressionParameters();
            auto* ids = Csm::CubismFramework::GetIdManager();
            std::cout << "{\"fade_in\":";
            value(expression->GetFadeInTime());
            std::cout << ",\"fade_out\":";
            value(expression->GetFadeOutTime());
            std::cout << ",\"parameters\":[";
            for (int i = 0; i < parameters.GetSize(); ++i) {
                if (i) std::cout << ',';
                std::cout << "{\"is_param_x\":" << (parameters[i].ParameterId == ids->GetId("ParamX") ? "true" : "false")
                          << ",\"blend\":" << int(parameters[i].BlendType) << ",\"value\":";
                value(parameters[i].Value);
                std::cout << '}';
            }
            std::cout << "]}\n";
            return 0;
        }
        if (argc == 3 && std::string(argv[1]) == "--json-check") {
            auto jsonBytes = read_file(argv[2]);
            Allocator allocator;
            FrameworkSession session(allocator);
            auto* json = Csm::Utils::CubismJson::Create(jsonBytes.data(), jsonBytes.size());
            if (!json) {
                std::cout << "{\"accepted\":false}\n";
            } else {
                auto& node = json->GetRoot()["Value"];
                std::cout << "{\"accepted\":true,\"numeric\":"
                          << (node.IsFloat() ? "true" : "false");
                if (node.IsFloat()) {
                    float parsed = node.ToFloat();
                    std::cout << ",\"finite\":" << (std::isfinite(parsed) ? "true" : "false");
                    if (std::isfinite(parsed)) {
                        std::cout << ",\"value\":";
                        std::cout << std::setprecision(std::numeric_limits<float>::max_digits10);
                        value(parsed);
                    }
                }
                std::cout << "}\n";
                Csm::Utils::CubismJson::Delete(json);
            }
            return 0;
        }
        if (argc == 3 && std::string(argv[1]) == "--repeat-check") {
            auto mocBytes = read_file(argv[2]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected repeat MOC");
            std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
            if (!model || model->GetParameterCount() != 2)
                throw std::runtime_error("Repeat MOC does not contain two parameters");
            const int index = model->GetParameterIndex(Csm::CubismFramework::GetIdManager()->GetId("ParamY"));
            if (index != 1 || !model->GetParameterRepeats(index))
                throw std::runtime_error("ParamY is not a real MOC repeat parameter");
            std::cout << std::setprecision(9)
                      << "{\"parameter_id\":\"ParamY\",\"parameter_index\":" << index
                      << ",\"moc_repeat\":true,\"minimum\":";
            value(model->GetParameterMinimumValue(index));
            std::cout << ",\"maximum\":";
            value(model->GetParameterMaximumValue(index));
            std::cout << ",\"configs\":[";
            bool firstConfig = true;
            for (bool override : {true, false}) {
                model->SetOverrideFlagForModelParameterRepeat(override);
                if (!firstConfig) std::cout << ',';
                firstConfig = false;
                std::cout << "{\"model_override\":" << (override ? "true" : "false")
                          << ",\"effective_repeat\":" << (model->IsRepeat(index) ? "true" : "false")
                          << ",\"samples\":[";
                bool firstSample = true;
                for (float input : {-3.0f, -1.25f, -1.0f, -0.75f, 0.0f, 0.75f, 1.0f, 1.25f, 3.0f}) {
                    model->SetParameterValue(index, input);
                    model->Update();
                    if (!firstSample) std::cout << ',';
                    firstSample = false;
                    std::cout << "{\"input\":";
                    value(input);
                    std::cout << ",\"parameter\":";
                    value(model->GetParameterValue(index));
                    std::cout << ",\"drawable0_vertex0_y\":";
                    value(model->GetDrawableVertexPositions(0)[0].Y);
                    std::cout << '}';
                }
                std::cout << "]}";
            }
            std::cout << "]}\n";
            return 0;
        }
        if (argc == 4 && std::string(argv[1]) == "--motion-curve") {
            auto mocBytes = read_file(argv[2]);
            auto motionBytes = read_file(argv[3]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected curve MOC");
            std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
            std::unique_ptr<Csm::CubismMotion, void(*)(Csm::ACubismMotion*)> motion(
                Csm::CubismMotion::Create(motionBytes.data(), motionBytes.size(), nullptr, nullptr, true),
                &Csm::ACubismMotion::Delete);
            if (!model || !motion) throw std::runtime_error("Framework rejected curve motion");
            Csm::CubismMotionManager manager;
            manager.StartMotionPriority(motion.get(), false, 1);
            std::cout << std::setprecision(9) << "{\"samples\":[";
            float time = 0.0f;
            for (int index = 0; index <= 8; ++index) {
                float dt = index == 0 ? 0.0f : 0.125f;
                time += dt;
                model->SetParameterValue(0, 0.0f);
                manager.UpdateMotion(model.get(), dt);
                if (index > 0) std::cout << ',';
                std::cout << "{\"time\":";
                value(time);
                std::cout << ",\"param_x\":";
                value(model->GetParameterValue(0));
                std::cout << '}';
            }
            std::cout << "]}\n";
            manager.StopAllMotions();
            return 0;
        }
        if (argc == 4 && std::string(argv[1]) == "--motion-loop") {
            auto mocBytes = read_file(argv[2]);
            auto motionBytes = read_file(argv[3]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected loop MOC");
            std::cout << std::setprecision(9) << "{\"meta_loop\":true,\"set_loop_called\":true,\"configs\":[";
            bool firstConfig = true;
            for (auto behavior : {Csm::CubismMotion::MotionBehavior_V2, Csm::CubismMotion::MotionBehavior_V1}) {
                std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
                if (!model) throw std::runtime_error("Could not create loop model");
                std::unique_ptr<Csm::CubismMotion, void(*)(Csm::ACubismMotion*)> motion(
                    Csm::CubismMotion::Create(motionBytes.data(), motionBytes.size(), nullptr, nullptr, true),
                    &Csm::ACubismMotion::Delete);
                if (!motion) throw std::runtime_error("Framework rejected loop motion");
                int defaultBehavior = int(motion->GetMotionBehavior());
                motion->SetMotionBehavior(behavior);
                motion->SetLoop(true);
                Csm::CubismMotionManager manager;
                manager.StartMotionPriority(motion.get(), false, 1);
                if (!firstConfig) std::cout << ',';
                firstConfig = false;
                std::cout << "{\"default_behavior\":" << defaultBehavior
                          << ",\"behavior\":" << int(behavior) << ",\"frames\":[";
                bool firstFrame = true;
                float time = 0.0f;
                for (float dt : {0.25f, 0.25f, 0.5f, 0.25f, 0.25f, 0.25f, 0.5f}) {
                    time += dt;
                    manager.UpdateMotion(model.get(), dt);
                    model->Update();
                    if (!firstFrame) std::cout << ',';
                    firstFrame = false;
                    std::cout << "{\"time\":";
                    value(time);
                    std::cout << ",\"param_x\":";
                    value(model->GetParameterValue(0));
                    std::cout << '}';
                }
                std::cout << "]}";
                manager.StopAllMotions();
            }
            std::cout << "]}\n";
            return 0;
        }
        if (argc == 4 && std::string(argv[1]) == "--pose-check") {
            auto mocBytes = read_file(argv[2]);
            auto poseBytes = read_file(argv[3]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected pose MOC");
            std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
            std::unique_ptr<Csm::CubismPose, void(*)(Csm::CubismPose*)> pose(
                Csm::CubismPose::Create(poseBytes.data(), poseBytes.size()), &Csm::CubismPose::Delete);
            if (!model || !pose) throw std::runtime_error("Framework rejected pose asset");
            pose->UpdateParameters(model.get(), 1.0f / 60.0f);
            std::cout << "{\"parts\":" << model->GetPartCount() << ",\"part0_opacity\":";
            value(model->GetPartOpacity(0));
            std::cout << "}\n";
            return 0;
        }
        if (argc == 3 && std::string(argv[1]) == "--model3-check") {
            auto bytes = read_file(argv[2]);
            Allocator allocator;
            FrameworkSession session(allocator);
            Csm::CubismModelSettingJson settings(bytes.data(), bytes.size());
            if (!settings.IsValid()) throw std::runtime_error("Framework rejected model3");
            std::cout << "{\"moc\":\"" << settings.GetModelFileName()
                      << "\",\"textures\":" << settings.GetTextureCount()
                      << ",\"expressions\":" << settings.GetExpressionCount()
                      << ",\"motion_groups\":" << settings.GetMotionGroupCount()
                      << ",\"hit_areas\":" << settings.GetHitAreasCount()
                      << "}\n";
            return 0;
        }
        if (argc == 4 && std::string(argv[1]) == "--physics-stabilize") {
            auto mocBytes = read_file(argv[2]);
            auto physicsBytes = read_file(argv[3]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected stabilization MOC");
            std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
            std::unique_ptr<Csm::CubismPhysics, void(*)(Csm::CubismPhysics*)> physics(
                Csm::CubismPhysics::Create(physicsBytes.data(), physicsBytes.size()), &Csm::CubismPhysics::Delete);
            if (!model || !physics) throw std::runtime_error("Framework rejected stabilization asset");
            model->SetParameterValue(0, 0.7f);
            physics->Stabilization(model.get());
            model->Update();
            std::cout << std::setprecision(9) << "{\"stabilized\":";
            value(model->GetParameterValue(1));
            std::cout << ",\"frames\":[";
            for (int i = 0; i < 20; ++i) {
                float dt = i % 2 == 0 ? 1.0f / 60.0f : 1.0f / 30.0f;
                model->SetParameterValue(0, i % 3 == 0 ? -0.4f : 0.7f);
                physics->Evaluate(model.get(), dt);
                model->Update();
                if (i) std::cout << ',';
                value(model->GetParameterValue(1));
            }
            std::cout << "]}\n";
            return 0;
        }
        if (argc == 4 && std::string(argv[1]) == "--physics-sequence") {
            auto mocBytes = read_file(argv[2]);
            auto physicsBytes = read_file(argv[3]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected physics-sequence MOC");
            std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
            std::unique_ptr<Csm::CubismPhysics, void(*)(Csm::CubismPhysics*)> physics(
                Csm::CubismPhysics::Create(physicsBytes.data(), physicsBytes.size()), &Csm::CubismPhysics::Delete);
            if (!model || !physics) throw std::runtime_error("Framework rejected physics-sequence asset");
            std::cout << std::setprecision(9) << "{\"frames\":[";
            for (int i = 0; i < 120; ++i) {
                float dt = i % 11 == 0 ? 0.1f : (i % 3 == 0 ? 1.0f / 30.0f : 1.0f / 60.0f);
                float input = sinf(static_cast<float>(i) * 0.17f);
                model->SetParameterValue(0, input);
                physics->Evaluate(model.get(), dt);
                model->Update();
                if (i) std::cout << ',';
                std::cout << "{\"dt\":"; value(dt);
                std::cout << ",\"input\":"; value(model->GetParameterValue(0));
                std::cout << ",\"output\":"; value(model->GetParameterValue(1));
                std::cout << '}';
            }
            std::cout << "]}\n";
            return 0;
        }
        if (argc == 6 && std::string(argv[1]) == "--physics-fps") {
            auto mocBytes = read_file(argv[2]);
            Allocator allocator;
            FrameworkSession session(allocator);
            std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
                Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
            if (!moc) throw std::runtime_error("Official Core rejected physics MOC");
            std::cout << std::setprecision(9) << "{\"configs\":[";
            bool firstConfig = true;
            for (int config = 0; config < 3; ++config) {
                auto physicsBytes = read_file(argv[3 + config]);
                Csm::CubismPhysicsJson parsed(physicsBytes.data(), physicsBytes.size());
                if (!parsed.IsValid()) throw std::runtime_error("Framework rejected physics JSON");
                std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
                if (!model || model->GetParameterCount() != 2)
                    throw std::runtime_error("Physics MOC does not have two parameters");
                std::unique_ptr<Csm::CubismPhysics, void(*)(Csm::CubismPhysics*)> physics(
                    Csm::CubismPhysics::Create(physicsBytes.data(), physicsBytes.size()),
                    &Csm::CubismPhysics::Delete);
                if (!physics) throw std::runtime_error("Framework rejected physics fixture");
                if (!firstConfig) std::cout << ',';
                firstConfig = false;
                std::cout << "{\"config\":\"" << (config == 0 ? "missing" : config == 1 ? "zero" : "thirty")
                          << "\",\"parsed_fps\":";
                value(parsed.GetFps());
                std::cout << ",\"frames\":[";
                float time = 0.0f;
                bool firstFrame = true;
                const float dt[] = {1.0f / 60, 1.0f / 60, 1.0f / 60, 1.0f / 60, 1.0f / 30, 0.1f};
                const float input[] = {0, 1, 1, -1, -1, 0};
                for (int i = 0; i < 6; ++i) {
                    time += dt[i];
                    model->SetParameterValue(0, input[i]);
                    physics->Evaluate(model.get(), dt[i]);
                    model->Update();
                    if (!firstFrame) std::cout << ',';
                    firstFrame = false;
                    std::cout << "{\"time\":";
                    value(time);
                    std::cout << ",\"dt\":";
                    value(dt[i]);
                    std::cout << ",\"input\":";
                    value(model->GetParameterValue(0));
                    std::cout << ",\"output\":";
                    value(model->GetParameterValue(1));
                    std::cout << '}';
                }
                std::cout << "]}";
            }
            std::cout << "]}\n";
            return 0;
        }
        const bool realControl = argc == 5 && std::string(argv[1]) == "--real-control";
        if ((!realControl && argc != 4) || (realControl && argc != 5))
            throw std::runtime_error("Usage: probe [--real-control] model.moc3 motion3.json pose3.json");
        const int offset = realControl ? 2 : 1;
        auto mocBytes = read_file(argv[offset]);
        auto motionBytes = read_file(argv[offset + 1]);
        auto poseBytes = read_file(argv[offset + 2]);
        Allocator allocator;
        FrameworkSession session(allocator);
        std::unique_ptr<Csm::CubismMoc, void(*)(Csm::CubismMoc*)> moc(
            Csm::CubismMoc::Create(mocBytes.data(), mocBytes.size(), true), &Csm::CubismMoc::Delete);
        if (!moc) throw std::runtime_error("Official Core rejected MOC");
        std::unique_ptr<Csm::CubismModel, ModelDeleter> model(moc->CreateModel(), ModelDeleter{moc.get()});
        if (!model) throw std::runtime_error("Official Core could not create model");
        std::unique_ptr<Csm::CubismMotion, void(*)(Csm::ACubismMotion*)> motion(
            Csm::CubismMotion::Create(motionBytes.data(), motionBytes.size(), nullptr, nullptr, true),
            &Csm::ACubismMotion::Delete);
        if (!motion) throw std::runtime_error("Framework rejected motion fixture");
        std::unique_ptr<Csm::CubismPose, void(*)(Csm::CubismPose*)> pose(
            Csm::CubismPose::Create(poseBytes.data(), poseBytes.size()), &Csm::CubismPose::Delete);
        if (!pose) throw std::runtime_error("Framework rejected pose fixture");
        auto* ids = Csm::CubismFramework::GetIdManager();
        auto* part0 = ids->GetId("Part0");
        auto* part1 = ids->GetId("Part1");
        if (model->GetParameterCount() != 2 || model->GetPartCount() != 2 || model->GetDrawableCount() != 2 ||
            model->GetPartIndex(part0) != 0 || model->GetPartIndex(part1) != 1 ||
            model->GetDrawableParentPartIndex(0) != 0 || model->GetDrawableParentPartIndex(1) != 1)
            throw std::runtime_error("MOC fixture does not have two real Parts with separate drawables");
        const int part0Control = model->GetParameterIndex(part0);
        const int part1Control = model->GetParameterIndex(part1);
        if (realControl) {
            if (part0Control != 1 || part1Control < model->GetParameterCount())
                throw std::runtime_error("Pose fixture needs real Part0 control and virtual Part1 control");
        } else if (part0Control < model->GetParameterCount() ||
                   part1Control < model->GetParameterCount() || part0Control == part1Control) {
            throw std::runtime_error("Pose control IDs are not distinct virtual parameters");
        }
        Csm::CubismMotionManager manager;
        pose->UpdateParameters(model.get(), 0.0f);
        model->Update();
        std::cout << std::setprecision(9)
                  << "{\"core_version\":" << Live2D::Cubism::Core::csmGetVersion()
                  << ",\"motion_behavior\":" << int(motion->GetMotionBehavior())
                  << ",\"real_parameter_count\":" << model->GetParameterCount()
                  << ",\"real_part_count\":" << model->GetPartCount()
                  << ",\"real_drawable_count\":" << model->GetDrawableCount()
                  << ",\"real_parameter_ids\":[";
        for (int i = 0; i < model->GetParameterCount(); ++i) {
            if (i) std::cout << ',';
            std::cout << std::quoted(model->GetParameterId(i)->GetString().GetRawString());
        }
        std::cout << "],\"real_part_ids\":[";
        for (int i = 0; i < model->GetPartCount(); ++i) {
            if (i) std::cout << ',';
            std::cout << std::quoted(model->GetPartId(i)->GetString().GetRawString());
        }
        std::cout << "],\"drawable_parent_parts\":[";
        for (int i = 0; i < model->GetDrawableCount(); ++i) {
            if (i) std::cout << ',';
            std::cout << model->GetDrawableParentPartIndex(i);
        }
        std::cout << ']'
                  << ",\"frames\":[";
        frame(model.get(), part0, part1, "pose_reset", 0.0f);
        manager.StartMotionPriority(motion.get(), false, 1);
        float time = 0.0f;
        for (float dt : {0.25f, 0.25f, 0.25f}) {
            time += dt;
            manager.UpdateMotion(model.get(), dt);
            pose->UpdateParameters(model.get(), dt);
            model->Update();
            std::cout << ',';
            frame(model.get(), part0, part1, "motion_pose", time);
        }
        std::cout << "]}\n";
        manager.StopAllMotions();
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
