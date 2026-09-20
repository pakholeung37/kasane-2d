# kasane-document

Native editor persistence, independent of Godot.

- `DocumentSession`: Document, project root and optimistic save baseline.
- `DocumentStore`: verified opening, portable save-as and atomic manifest publication.
- `FileSystem`: one injectable interface for reads, exclusive writes, locks, moves and synchronization. NativeFileSystem combines standard-library paths/reads with OS primitives.
- Native JSON codec (nlohmann), PNG decoding (libpng) and SHA-256 (OpenSSL Crypto).
- Runtime package publication shares the filesystem and supports rollback.

All public project paths must be native absolute paths. Project manifests keep relative assets under the project root; explicit replacement may temporarily reference an external absolute image until the next save. Existing directory-project v1 files remain readable. Experimental formats are rejected.

Save serializes cooperating writers and refuses stale manifest overwrites. Post-publication synchronization failures return success with a durability warning. This is not a general database transaction or a crash-recovery journal.

Build and run without Godot from the repository root:

```sh
cmake --preset core-make
cmake --build --preset core-make --parallel
ctest --preset core-make
```

The native tests use actual temporary directories with an injectable failure wrapper for short writes, sync/close failures, manifest replacement, and package rollback. Cross-process movement, editing, locks and double-Core compatibility are checked by `tools/validate_native_project.py`.

See [format and guarantees](../../docs/milestones/M2-FORMAT.md).
