// SPDX-License-Identifier: MIT
#include <kasane/filesystem.hpp>
#include <cerrno>
#include <fstream>
#include <iomanip>
#include <random>
#include <sstream>
#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#else
#include <fcntl.h>
#include <sys/file.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/syscall.h>
#include <linux/fs.h>
#endif
#ifdef __APPLE__
#include <stdio.h>
#endif
#endif

namespace kasane::io {
std::string path_text(const fs::path &p) {
    auto text = p.generic_u8string();
    return {reinterpret_cast<const char *>(text.data()), text.size()};
}

fs::path path_from_utf8(const std::string &s) {
    return fs::path(std::u8string(reinterpret_cast<const char8_t *>(s.data()), s.size()));
}

Error::Error(std::string op, fs::path p, std::error_code code)
    : std::runtime_error(op + ": " + path_text(p) + ": " + code.message()), operation(std::move(op)),
      path(std::move(p)), system(code) {
}

namespace {
std::error_code last_error() {
#ifdef _WIN32
    return {int(GetLastError()), std::system_category()};
#else
    return {errno, std::generic_category()};
#endif
}

[[noreturn]] void fail(const char *operation, const fs::path &p) {
    throw Error(operation, p, last_error());
}
#ifdef _WIN32
using Handle = HANDLE;
const Handle invalid = INVALID_HANDLE_VALUE;

void release(Handle h) {
    CloseHandle(h);
}
#else
using Handle = int;
constexpr Handle invalid = -1;

void release(Handle h) {
    ::close(h);
}
#endif
class NativeWriter final : public Writer {
    Handle handle_;
    fs::path path_;

  public:
    NativeWriter(Handle h, fs::path p) : handle_(h), path_(std::move(p)) {}

    ~NativeWriter() override {
        if (handle_ != invalid)
            release(handle_);
    }

    void write(std::span<const uint8_t> bytes) override {
        while (!bytes.empty()) {
#ifdef _WIN32
            DWORD count = 0;
            if (!WriteFile(handle_, bytes.data(), DWORD(std::min<size_t>(bytes.size(), 1 << 20)), &count,
                           nullptr))
                fail("write", path_);
#else
            auto count = ::write(handle_, bytes.data(), bytes.size());
            if (count < 0 && errno == EINTR)
                continue;
            if (count < 0)
                fail("write", path_);
#endif
            if (count == 0)
                throw Error("write", path_, std::make_error_code(std::errc::io_error));
            bytes = bytes.subspan(size_t(count));
        }
    }

    void sync() override {
#ifdef _WIN32
        if (!FlushFileBuffers(handle_))
            fail("sync_file", path_);
#else
        int result;
        do {
            result = ::fsync(handle_);
        } while (result != 0 && errno == EINTR);
        if (result != 0)
            fail("sync_file", path_);
#endif
    }

    void close() override {
        auto h = handle_;
        handle_ = invalid;
#ifdef _WIN32
        if (!CloseHandle(h))
            fail("close", path_);
#else
        if (::close(h) != 0)
            fail("close", path_);
#endif
    }
};

class NativeLock final : public Lock {
    Handle handle_;

  public:
    explicit NativeLock(Handle h) : handle_(h) {}

    ~NativeLock() override { release(handle_); }
};
} // namespace

Bytes NativeFileSystem::read(const fs::path &p) {
    std::ifstream file(p, std::ios::binary | std::ios::ate);
    if (!file)
        throw Error("read", p, std::error_code(errno ? errno : EIO, std::generic_category()));
    auto length = file.tellg();
    if (length < 0 || uint64_t(length) > (uint64_t(1) << 30))
        throw Error("read", p, std::make_error_code(std::errc::file_too_large));
    Bytes bytes(static_cast<size_t>(length));
    file.seekg(0);
    if (!bytes.empty())
        file.read(reinterpret_cast<char *>(bytes.data()), std::streamsize(bytes.size()));
    if (!file)
        throw Error("read", p, std::make_error_code(std::errc::io_error));
    return bytes;
}

FileInfo NativeFileSystem::info(const fs::path &p) {
    std::error_code ec;
    auto status = fs::symlink_status(p, ec);
    if (ec == std::errc::no_such_file_or_directory)
        return {};
    if (ec)
        throw Error("stat", p, ec);
    return {fs::exists(status), fs::is_directory(status), fs::is_symlink(status)};
}

fs::path NativeFileSystem::canonical(const fs::path &p) {
    std::error_code ec;
    auto result = fs::weakly_canonical(p, ec);
    if (ec)
        throw Error("canonical", p, ec);
    return result;
}

void NativeFileSystem::create_directories(const fs::path &p) {
    std::error_code ec;
    fs::create_directories(p, ec);
    if (ec)
        throw Error("mkdir", p, ec);
}

void NativeFileSystem::create_directory_new(const fs::path &p) {
    std::error_code ec;
    if (!fs::create_directory(p, ec))
        throw Error("mkdir_new", p, ec ? ec : std::make_error_code(std::errc::file_exists));
}

std::unique_ptr<Writer> NativeFileSystem::create_file_new(const fs::path &p) {
#ifdef _WIN32
    auto h = CreateFileW(p.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
#else
    auto h = ::open(p.c_str(), O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW, 0600);
#endif
    if (h == invalid)
        fail("create_new", p);
    return std::make_unique<NativeWriter>(h, p);
}

void NativeFileSystem::replace_file(const fs::path &source, const fs::path &target) {
#ifdef _WIN32
    if (!MoveFileExW(source.c_str(), target.c_str(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH))
        fail("replace_file", target);
#else
    if (::rename(source.c_str(), target.c_str()) != 0)
        fail("replace_file", target);
#endif
}

void NativeFileSystem::move_new(const fs::path &source, const fs::path &target) {
#ifdef _WIN32
    if (!MoveFileExW(source.c_str(), target.c_str(), MOVEFILE_WRITE_THROUGH))
        fail("move_new", target);
#elif defined(__APPLE__)
    if (renamex_np(source.c_str(), target.c_str(), RENAME_EXCL) != 0)
        fail("move_new", target);
#elif defined(__linux__)
    if (syscall(SYS_renameat2, AT_FDCWD, source.c_str(), AT_FDCWD, target.c_str(), RENAME_NOREPLACE) != 0)
        fail("move_new", target);
#else
    throw Error("move_new", target, std::make_error_code(std::errc::operation_not_supported));
#endif
}

void NativeFileSystem::remove(const fs::path &p) {
    std::error_code ec;
    fs::remove_all(p, ec);
    if (ec)
        throw Error("remove", p, ec);
}

void NativeFileSystem::sync_directory(const fs::path &p) {
#ifdef _WIN32
    // Windows publication uses MOVEFILE_WRITE_THROUGH; directory fsync has no equivalent here.
    (void)p;
#else
    auto fd = ::open(p.c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0)
        fail("sync_directory", p);
    int result;
    do {
        result = ::fsync(fd);
    } while (result != 0 && errno == EINTR);
    auto error = last_error();
    ::close(fd);
    if (result != 0)
        throw Error("sync_directory", p, error);
#endif
}

std::unique_ptr<Lock> NativeFileSystem::lock(const fs::path &directory) {
    auto p = directory / ".kasane.lock";
#ifdef _WIN32
    auto h = CreateFileW(p.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_ALWAYS,
                         FILE_ATTRIBUTE_NORMAL, nullptr);
    if (h == invalid)
        fail("lock", p);
#else
    auto h = ::open(p.c_str(), O_RDWR | O_CREAT | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (h < 0)
        fail("lock", p);
    if (::flock(h, LOCK_EX | LOCK_NB) != 0) {
        auto error = last_error();
        release(h);
        throw Error("lock", p, error);
    }
#endif
    // Keep the lock inode on disk. Unlinking it would allow two concurrent locks on different inodes.
    return std::make_unique<NativeLock>(h);
}

std::shared_ptr<FileSystem> native_filesystem() {
    return std::make_shared<NativeFileSystem>();
}

fs::path local_path(const fs::path &p) {
    auto s = path_text(p);
    if (s.empty() || !p.is_absolute() || s.find("://") != std::string::npos ||
        s.find('\0') != std::string::npos)
        throw Error("local_path", p, std::make_error_code(std::errc::invalid_argument));
    return p.lexically_normal();
}

bool asset_path(const std::string &s) {
    auto p = path_from_utf8(s);
    if (!s.starts_with("assets/") || s.find('\\') != std::string::npos || s.find(':') != std::string::npos ||
        s.find('\0') != std::string::npos || p.is_absolute() || p.lexically_normal() != p ||
        p.filename().empty())
        return false;
    for (const auto &part : p)
        if (part == ".." || part == ".")
            return false;
    return true;
}

fs::path resolve_asset(FileSystem &filesystem, const fs::path &root, const std::string &source) {
    if (root.empty() || !asset_path(source))
        return local_path(path_from_utf8(source));
    auto base = filesystem.canonical(local_path(root));
    auto path = filesystem.canonical(base / path_from_utf8(source));
    auto relative = path.lexically_relative(base);
    if (relative.empty() || *relative.begin() == "..")
        throw Error("resolve_asset", path, std::make_error_code(std::errc::permission_denied));
    return path;
}

std::string unique_name() {
    std::random_device random;
    std::ostringstream out;
    out << std::hex << std::setfill('0');
    for (int i = 0; i < 4; ++i)
        out << std::setw(8) << random();
    return out.str();
}

void write_new(FileSystem &filesystem, const fs::path &p, std::span<const uint8_t> bytes) {
    // Only remove a file after this invocation successfully acquired exclusive ownership.
    auto writer = filesystem.create_file_new(p);
    try {
        writer->write(bytes);
        writer->sync();
        writer->close();
    } catch (...) {
        writer.reset();
        try {
            filesystem.remove(p);
        } catch (...) {
        }
        throw;
    }
}

TemporaryDirectory::TemporaryDirectory(FileSystem &filesystem, const fs::path &parent)
    : filesystem_(filesystem) {
    path = parent / (".kasane-stage-" + unique_name());
    filesystem_.create_directory_new(path);
}

TemporaryDirectory::~TemporaryDirectory() {
    try {
        filesystem_.remove(path);
    } catch (...) {
    }
}
} // namespace kasane::io
