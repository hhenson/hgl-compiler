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

## The debug archive

Beside each platform's archive a release has an optional second one,
`hgl-debug-VERSION-PLATFORM.tar.gz`, with its SHA-256. Nobody needs it to
build or run a program. It unpacks where the release did: its top directory
is the release's, `hgl-VERSION-PLATFORM`. It holds:

- hgraph's SDK at the pinned tag and fmt, spdlog and simdjson, built for
  debugging (CMake's `Debug`: no optimisation, debug information, not
  stripped), in `libexec/hgraph-debug`, laid out as `libexec` is
  (`hgraph-sdk`, `hgraph-deps`), with their licences in
  `libexec/hgraph-debug/licenses` and hgraph's in the SDK. fmt and spdlog
  name a debug build's libraries `libfmtd` and `libspdlogd`, as they do. A
  debug build of a program (`hgl run --debug-build`) links these, and says
  so ([architecture](architecture.md), "The build driver").
- The symbol files of the release's own libraries, beside each: on Linux
  `lib/.debug/LIBRARY.debug`, which the library's debug link names and gdb
  and lldb look for there; on macOS `LIBRARY.dSYM`, matched by UUID. With
  them a debugger attached to an ordinary run, or reading a core file of
  one, gives an engine frame's file and line; without them it names the
  function.

The release's libraries are built with debug information, which is split
out of them into the symbol files. They keep their symbol tables and build
IDs, and are otherwise as a build without debug information makes them but
for two things: the build ID, which is of the build with it, and on Linux
the debug link, 112 bytes a library. On Linux the debug sections of both
the symbol files and the debug libraries are compressed (zlib), which gdb
and lldb read as they are.

A frame's file is under `/hgl-release/hgraph/` for hgraph's source and
`/hgl-release/` for the dependencies' (`-ffile-prefix-map`). To see the
source, give the debugger a checkout of the tag: `set substitute-path
/hgl-release/hgraph DIR` in gdb, `settings set target.source-map
/hgl-release/hgraph DIR` in lldb.

On Linux (hgraph 0.9.23, GCC 15.2) the release archive is 10.1 MB and the
debug archive 126.6 MB: the debug libraries 106.4 MB unpacked (62.1 MB in
the archive), the symbol files 68.1 MB unpacked (64.5 MB in the archive),
of which hgraph's runtime library's is 52.2 MB. The two could be two
packages.

## Cut a release

Set the workspace version in `Cargo.toml` and update `Cargo.lock`. A push to
`main` builds and verifies both bundles, each with its debug archive, and
keeps each archive and SHA-256 file as a workflow artifact for 30 days. A fresh Ubuntu 26.04 runner
checks the ELF files' glibc requirements against 2.43. Each platform runs
the C++ target from a different unpack path with an empty build cache.
When `HGL_SELF_HOSTED_MACOS` is true, the C++ verification on macOS uses
the release runner. A GitHub-hosted macOS runner independently runs
`hgl --version` and `hgl check`. A job on each platform checks the debug
archive as a user has it, `scripts/check-debug-release.sh`: the release
alone links the optimised libraries for a debug build and says why; with
the debug archive unpacked into it, the debug tree is verified, a debug
build links the debug libraries, lldb or gdb stops in an engine frame of it
with its file, line and arguments (gdb: the locals of the engine's loop),
the release's own library resolves an engine function to its file and line,
and on Linux so does a core file of an ordinary run, whose process loads
the release's own engine library. The debuggers look for symbol files
beside the libraries alone, never in the machine's directories or, on
macOS, through Spotlight, which finds a dSYM of the same UUID wherever it
has seen one (a release cache keeps the release's). The macOS job runs
lldb unattended, so the release runner must let its account debug with no
prompt: `DevToolsSecurity -status` says developer mode is enabled
(`sudo DevToolsSecurity -enable`, once), and the account is in the
`_developer` group (`sudo dseditgroup -o edit -a ACCOUNT -t user
_developer`). Without both, `process launch` waits for an administrator's
password in a dialog no one sees, and the job ends at its 30-minute
limit. The debugged programs are this repository's own, signed ad hoc
without the hardened runtime, so nothing else is needed. Run **Release
bundles** by hand to rehearse these steps without publishing.

Push the bare workspace version as a tag, such as `0.1.0`. The tag run
checks the version and publishes the four archives and their checksums
from the successful `main` run for that commit. If there is no run with
both verified artifacts available, the tag run builds and verifies both
bundles before publishing.

## Update the Homebrew tap

1. Once the compiler release is published, prepare a tap branch with the
   formula's new version and each platform's published archive URL and
   SHA-256. The Linux formula uses Ubuntu 26.04's GCC 15 and C++ runtime.
   Remove the previous version's bottle stanza. Move the `hgl-debug`
   formula in the same commit to the same version, with the release's
   `hgl-debug-VERSION-PLATFORM.tar.gz` URL and SHA-256: it depends on
   `hgl` and installs only for the same version, so the two move together
   and `brew upgrade` takes both. Its keg is not `hgl`'s, so `hgl`'s
   formula links its `libexec/libexec/hgraph-debug`, and each library's
   symbol file beside it, to `hgl-debug`'s, links that lead nowhere until
   `hgl-debug` is installed. Have that commit reviewed in
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
contains the SDK and its libraries. **Homebrew bottle** bottles `hgl` alone;
`hgl-debug` installs from its archive, which needs no build.

## Build an archive locally

Build from a clean checkout with the pinned Rust toolchain, CMake, Ninja,
Git and Python 3.11 or later. On Apple silicon use Apple clang 21 or
later, with the command line tools' `dsymutil`. On x86-64 Ubuntu 26.04
install `gcc`, `g++-15` and binutils.

```sh
scripts/package-release.sh
```

Before fetching sources, the script compiles and runs a C++23 probe with
the configured toolchain.

It fetches the pinned sources, builds the SDK and its dependencies twice,
optimised with their debug information split out and for debugging, builds
`hgl`, and prints the path and SHA-256 of the release archive and of the
debug archive in `target/release-bundles`. Pass an
output directory as the first argument and optionally a version tag as
the second. The script checks a supplied tag against the workspace
version. Set `HGL_BUILD_JOBS` to limit parallel builds.

For repeated local builds, set `HGL_RELEASE_NATIVE_CACHE` to a directory
to keep the verified SDK, dependency prefix and native license notices,
and the debug archive's tree under `debug`. With gdb on Linux or lldb on
macOS, `scripts/check-debug-release.sh DIRECTORY` checks the two archives
in DIRECTORY as the release jobs do.
An unpacked release bundle can also supply that prebuilt tree. The
macOS workflow job keeps its native and Rust caches in the runner's tool
cache. Use distinct local cache directories for each build platform.
