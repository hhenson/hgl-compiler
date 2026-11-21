# Release bundles

Each release contains `hgl`, the pinned hgraph C++ SDK without Arrow,
fmt, spdlog, simdjson and their licenses. Run `bin/hgl` from the unpacked
bundle or link to it from a directory on `PATH`. `hgl --version` prints
the workspace version and hgraph tag.

| Archive suffix | Supported system | C++ compiler |
| --- | --- | --- |
| `macos-arm64` | Apple silicon macOS 26.0 or later | Apple clang 21 or later |
| `linux-x86_64` | x86-64 Ubuntu 26.04 LTS, glibc 2.43 | Ubuntu's GCC 15 |

Building and running HGL programs with the [C++ target](cpp-target.md)
also requires CMake and Ninja. Set `HGL_HGRAPH_SDK` or `HGL_HGRAPH_DEPS`
to override the corresponding bundled prefix. The Linux bundle and
generated programs use Ubuntu's system glibc and libstdc++. Ubuntu 24.04
ships GCC 13 and glibc 2.39, and Ubuntu 22.04 ships GCC 11 and glibc 2.35.
Those releases are outside the Linux archive and tap support floor.

## Cut a release

Set the workspace version in `Cargo.toml` and update `Cargo.lock`. A push to
`main` builds and verifies both bundles and keeps each archive and SHA-256
file as a workflow artifact for 30 days. A fresh Ubuntu 26.04 runner
checks the ELF files' glibc requirements against 2.43. Each platform runs
the C++ target from a different unpack path with an empty build cache.
When `HGL_SELF_HOSTED_MACOS` is true, the C++ verification on macOS uses
the release runner. A GitHub-hosted macOS runner independently runs
`hgl --version` and `hgl check`. Run **Release bundles** by hand to
rehearse these steps without publishing.

Push the bare workspace version as a tag, such as `0.1.0`. The tag run
checks the version and publishes both original archives and checksums
from the successful `main` run for that commit. If there is no run with
both verified artifacts available, the tag run builds and verifies both
bundles before publishing.

## Update the Homebrew tap

1. Once the compiler release is published, prepare a tap branch with the
   formula's new version and each platform's published archive URL and
   SHA-256. The Linux formula uses Ubuntu 26.04's GCC 15 and C++ runtime.
   Remove the previous version's bottle stanza. Have that commit reviewed in
   [the tap PR](https://github.com/hhenson/homebrew-hgraph/pulls) and keep
   the PR open for the bottle update.
2. Set `version` to the bare release tag and `tap_sha` to the full SHA of
   that reviewed commit in the tap. Dispatch **Homebrew bottle** on `main`
   with publishing enabled.

   ```sh
   gh workflow run homebrew-bottle.yml --repo hhenson/hgl-compiler --ref main \
     -f release_tag="$version" -f tap_sha="$tap_sha" -F publish=true
   ```

   The hosted Apple silicon macOS 26 job checks the formula against the
   published archive, installs it with `brew install --build-bottle`, and
   builds a relocatable `arm64_tahoe` bottle. The artifact keeps the bottle,
   SHA-256, generated stanza and manifest. The publish job verifies those
   bottle bytes and adds the tarball under Homebrew's download filename
   beside the source archive. Omitting the `publish` input keeps the run
   read-only.
3. Add the generated `bottle.rb` stanza **from the successful publish
   run** to the same tap PR. Its bottle SHA-256 is for the bottle tarball.
   Keep the published source archive's SHA-256 as the formula's source
   checksum. The stanza's `root_url` points to the matching compiler
   release. Verify an ordinary `brew install hhenson/hgraph/hgl` pours
   from the updated tap in an empty Homebrew prefix. Check `hgl --version`
   and `hgl check` after the pour. Run `brew audit --strict` and `brew test`
   on the formula before landing the tap PR.

Pouring the bottle needs Homebrew. Building or running an HGL program on
the C++ target requires Apple Clang 21 or later, CMake and Ninja. The bottle
contains the SDK and its libraries.

## Build an archive locally

Build from a clean checkout with the pinned Rust toolchain, CMake, Ninja,
Git and Python 3.11 or later. On Apple silicon use Apple clang 21 or
later. On x86-64 Ubuntu 26.04 install `gcc`, `g++-15` and binutils.

```sh
scripts/package-release.sh
```

Before fetching sources, the script compiles and runs a C++23 probe with
the configured toolchain.

It fetches the pinned sources, builds the SDK and `hgl`, and prints the
path and SHA-256 of the archive in `target/release-bundles`. Pass an
output directory as the first argument and optionally a version tag as
the second. The script checks a supplied tag against the workspace
version. Set `HGL_BUILD_JOBS` to limit parallel builds.

For repeated local builds, set `HGL_RELEASE_NATIVE_CACHE` to a directory
to keep the verified SDK, dependency prefix and native license notices.
An unpacked release bundle can also supply that prebuilt tree. The
macOS workflow job keeps its native and Rust caches in the runner's tool
cache. Use distinct local cache directories for each build platform.
