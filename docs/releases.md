# macOS releases

The Apple silicon bundle installs on macOS 26.0 or later. It contains
`hgl`, the pinned hgraph C++ SDK without Arrow, fmt, spdlog, simdjson and
their licenses. Run `bin/hgl` from the unpacked bundle or link to it from
a directory on `PATH`. `hgl --version` prints the workspace version and
hgraph tag.

Building and running HGL programs with the C++ target still needs
[Apple clang 21 or later](cpp-target.md), CMake and Ninja. The bundle
provides the SDK and its libraries. Set `HGL_HGRAPH_SDK` or
`HGL_HGRAPH_DEPS` to override the corresponding bundled prefix.

## Cut a release

Set the workspace version in `Cargo.toml` and update `Cargo.lock`. Run the
**macOS release bundle** workflow by hand to rehearse building and using
the archive. When `HGL_SELF_HOSTED_MACOS` is true, the release runner
builds the bundle and tests its C++ target from a different unpack path
with an empty build cache. A GitHub-hosted macOS runner independently
runs `hgl --version` and `hgl check`. The rehearsal uploads an artifact.
Push a matching version tag such as `v0.1.0` to publish the archive and
its SHA-256 file to GitHub Releases after those checks pass.

## Build an archive locally

From a clean checkout on Apple silicon with the pinned Rust toolchain,
Apple clang 21 or later, CMake, Ninja, Git and Python 3.11 or later:

```sh
scripts/package-release.sh
```

It fetches the pinned sources, builds the SDK and `hgl`, and prints the
path and SHA-256 of the archive in `target/release-bundles`. Pass an
output directory as the first argument and optionally a version tag as
the second. The script checks a supplied tag against the workspace
version. Set `HGL_BUILD_JOBS` to limit parallel builds.
