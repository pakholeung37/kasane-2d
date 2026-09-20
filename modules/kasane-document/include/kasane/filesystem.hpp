// SPDX-License-Identifier: MIT
#pragma once
#include <filesystem>
#include <memory>
#include <span>
#include <stdexcept>
#include <system_error>
#include <vector>

namespace kasane::io {
namespace fs = std::filesystem;
using Bytes = std::vector<uint8_t>;

struct Error : std::runtime_error {
    std::string operation;
    fs::path path;
    std::error_code system;
    Error(std::string operation, fs::path path, std::error_code system);
};

struct FileInfo {
    bool exists = false, directory = false, symlink = false;
};

class Writer {
  public:
    virtual ~Writer() = default;
    virtual void write(std::span<const uint8_t>) = 0;
    virtual void sync() = 0;
    virtual void close() = 0;
};

class Lock {
  public:
    virtual ~Lock() = default;
};

// All paths are native paths. Mutations throw Error before returning failure.
// replace_file commits at return; subsequent directory-sync errors are durability warnings.
class FileSystem {
  public:
    virtual ~FileSystem() = default;
    virtual Bytes read(const fs::path &) = 0;
    virtual FileInfo info(const fs::path &) = 0;
    virtual fs::path canonical(const fs::path &) = 0;
    virtual void create_directories(const fs::path &) = 0;
    virtual void create_directory_new(const fs::path &) = 0;
    virtual std::unique_ptr<Writer> create_file_new(const fs::path &) = 0;
    virtual void replace_file(const fs::path &temporary, const fs::path &target) = 0;
    virtual void move_new(const fs::path &source, const fs::path &target) = 0;
    virtual void remove(const fs::path &) = 0;
    virtual void sync_directory(const fs::path &) = 0;
    virtual std::unique_ptr<Lock> lock(const fs::path &directory) = 0;
};

class NativeFileSystem final : public FileSystem {
  public:
    Bytes read(const fs::path &) override;
    FileInfo info(const fs::path &) override;
    fs::path canonical(const fs::path &) override;
    void create_directories(const fs::path &) override;
    void create_directory_new(const fs::path &) override;
    std::unique_ptr<Writer> create_file_new(const fs::path &) override;
    void replace_file(const fs::path &, const fs::path &) override;
    void move_new(const fs::path &, const fs::path &) override;
    void remove(const fs::path &) override;
    void sync_directory(const fs::path &) override;
    std::unique_ptr<Lock> lock(const fs::path &) override;
};

std::shared_ptr<FileSystem> native_filesystem();
std::string path_text(const fs::path &);
fs::path path_from_utf8(const std::string &);
fs::path local_path(const fs::path &); // Reject relative paths, URI schemes and embedded NUL.
bool asset_path(const std::string &);
fs::path resolve_asset(FileSystem &, const fs::path &root, const std::string &source);
std::string unique_name();
void write_new(FileSystem &, const fs::path &, std::span<const uint8_t>);

class TemporaryDirectory {
    FileSystem &filesystem_;

  public:
    fs::path path;
    TemporaryDirectory(FileSystem &, const fs::path &parent);
    ~TemporaryDirectory();
    TemporaryDirectory(const TemporaryDirectory &) = delete;
    TemporaryDirectory &operator=(const TemporaryDirectory &) = delete;
};
} // namespace kasane::io
