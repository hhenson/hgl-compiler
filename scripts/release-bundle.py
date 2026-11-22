"""Prepare, verify and find reusable release assets."""

import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
from urllib.parse import urlencode
from urllib.request import urlopen

MACOS_TARGET = "aarch64-apple-darwin"
LINUX_TARGET = "x86_64-unknown-linux-gnu"
ARTIFACTS = ("hgl-macos-arm64", "hgl-linux-x86_64")
LINUX_GLIBC = (2, 43)
LINUX_SYSTEM_LIBRARIES = {
    "ld-linux-x86-64.so.2", "libc.so.6", "libdl.so.2", "libgcc_s.so.1",
    "libm.so.6", "libpthread.so.0", "librt.so.1", "libstdc++.so.6",
}
NATIVE_LIBRARIES = ("fmt", "spdlog", "simdjson")
NATIVE_DIRECTORIES = (
    "libexec/hgraph-sdk",
    "libexec/hgraph-deps",
    *(f"licenses/{name}" for name in NATIVE_LIBRARIES),
)
NOTICE = re.compile(r"^(?:copying|license|notice|unlicense)", re.IGNORECASE)
SYSTEM_LIBRARIES = ("/usr/lib/", "/System/Library/")
SYSTEM_PATHS = tuple(map(Path, (
    "/System/Library", "/usr/bin", "/usr/include", "/usr/lib", "/usr/libexec",
    "/usr/sbin", "/usr/share", "/bin", "/dev", "/sbin",
)))
RELEASE_METADATA = {".cmake", ".conf", ".ini", ".json", ".pc", ".toml", ".txt", ".yaml", ".yml"}
# A normal URL starts with two slashes after its scheme. A file URL names
# a filesystem path, so check it like a plain absolute path.
ABSOLUTE_PATH = re.compile(
    rb'''(?:^|(?<=[=:"';, (\t\n]))(?:file://)?(/(?!/)[A-Za-z0-9._~-]+(?:/[^\s"'\[\]{}()>;\x00,]*)*)''',
    re.MULTILINE,
)


def command(*args):
    return subprocess.check_output(args, text=True)


def version():
    with Path("Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["workspace"]["package"]["version"]


def native_cache_key():
    """Hash the SDK recipe and the effective compiler/SDK configuration."""
    recipe = [
        "HGRAPH_TAG",
        "scripts/hgraph-dependencies.sh",
        "scripts/provision-hgraph-sdk.sh",
        "scripts/package-release.sh",
        "scripts/release-bundle.py",
    ]
    environment = [
        "CC", "CXX", "CFLAGS", "CXXFLAGS", "LDFLAGS",
        "DEVELOPER_DIR", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET",
    ]
    inputs = {"target": MACOS_TARGET}
    inputs.update({name: os.environ.get(name, "") for name in environment})
    for name, args in [
        ("compiler", [os.environ.get("CXX") or "/usr/bin/clang++", "--version"]),
        ("sdk", ["/usr/bin/xcrun", "--sdk", "macosx", "--show-sdk-version"]),
    ]:
        try:
            inputs[name] = subprocess.check_output(args, stderr=subprocess.DEVNULL, text=True)
        except (OSError, subprocess.CalledProcessError):
            # The package script diagnoses an unusable compiler or SDK.
            inputs[name] = "unavailable"
    inputs.update({name: hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in recipe})
    return hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()


def copy_native(source, destination):
    """Keep SDK and dependency libraries, their relative links and notices together."""
    for directory in NATIVE_DIRECTORIES:
        shutil.copytree(source / directory, destination / directory, symlinks=True)


def save_native(bundle, cache):
    """Publish the verified native tree atomically on the cache's filesystem."""
    cache.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".hgl-native-", dir=cache.parent) as scratch:
        staged = Path(scratch) / "native"
        copy_native(bundle, staged)
        try:
            staged.rename(cache)
        except OSError as error:
            # Another build can populate the same cache key first.
            if error.errno not in (errno.EEXIST, errno.ENOTEMPTY) or not all(
                (cache / directory).is_dir() for directory in NATIVE_DIRECTORIES
            ):
                raise


def github_items(endpoint, key):
    """Read every page of a GitHub Actions list, surfacing API errors."""
    rows = command("gh", "api", "--paginate", "--jq", f".{key}[] | @json", endpoint)
    return (json.loads(row) for row in rows.splitlines())


def verified_run(repository, commit, current_run):
    """Find a completed main push that verified both live artifacts for this commit."""
    base = f"repos/{repository}/actions"
    query = urlencode({
        "branch": "main", "event": "push", "head_sha": commit,
        "status": "success", "per_page": 100,
    })
    for run in github_items(f"{base}/workflows/release.yml/runs?{query}", "workflow_runs"):
        if (run["head_sha"], run["head_branch"], run["event"], run["conclusion"]) != (
            commit, "main", "push", "success",
        ) or str(run["id"]) == current_run:
            continue
        live = {
            artifact["name"]: artifact["id"]
            for artifact in github_items(f"{base}/runs/{run['id']}/artifacts?per_page=100", "artifacts")
            if not artifact["expired"]
        }
        names = [f"{artifact}-{run['run_attempt']}" for artifact in ARTIFACTS]
        if all(name in live for name in names):
            return run["id"], *(live[name] for name in names)
    return None


def rust_licenses(destination, triple):
    """Collect notices from the Rust distribution and the Cargo build graph."""
    std = Path(command("rustc", "--print", "sysroot").strip()) / "share/doc/rust"
    shutil.copytree(std / "licenses", destination / "rust")
    for copyright_file in std.glob("COPYRIGHT*.html"):
        shutil.copy2(copyright_file, destination / "rust")

    target = ["--target", triple, "--locked"]
    tree = command(
        "cargo", "tree", "-p", "hgl_cli", *target,
        "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
    )
    used = {tuple(line.split()[:2]) for line in tree.splitlines()}
    metadata = json.loads(command(
        "cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", triple,
    ))
    for package in metadata["packages"]:
        name, version = package["name"], package["version"]
        if package["source"] is None or (name, f"v{version}") not in used:
            continue
        source = Path(package["manifest_path"]).parent
        target_dir = destination / "crates" / f"{name}-{version}"
        target_dir.mkdir(parents=True)
        notices = {f for f in source.iterdir() if f.is_file() and NOTICE.match(f.name)}
        if package["license_file"]:
            notices.add(source / package["license_file"])
        for file in sorted(notices):
            shutil.copy2(file, target_dir)
        if not notices:
            upstream_notice(package, source, target_dir)


def upstream_notice(package, source, target_dir):
    """Some objc2 packages ship the license only in their source repository."""
    if package["repository"] != "https://github.com/madsmtm/objc2":
        raise ValueError(f"no license files for {package['name']} {package['version']}")
    vcs = json.loads((source / ".cargo_vcs_info.json").read_text())
    revision = vcs["git"]["sha1"]
    if not re.fullmatch("[0-9a-f]{40}", revision):
        raise ValueError(f"no upstream license revision for {package['name']}")
    url = f"https://raw.githubusercontent.com/madsmtm/objc2/{revision}/LICENSE.md"
    with urlopen(url, timeout=30) as response:
        (target_dir / "LICENSE.md").write_bytes(response.read())


def verify(bundle, forbidden, target):
    """Check relocatability, architecture and system library requirements."""
    bundle = bundle.resolve(strict=True)
    bin_file = bundle / "bin/hgl"
    sdk = bundle / "libexec/hgraph-sdk"
    deps = bundle / "libexec/hgraph-deps"
    for path in [
        bin_file, sdk / "hgl-sdk.txt", sdk / "lib/cmake/hgraph/hgraphConfig.cmake",
        deps / "include/fmt", deps / "include/spdlog", deps / "include/simdjson.h",
        bundle / "licenses/hgl-compiler.txt",
    ]:
        if not path.exists():
            raise ValueError(f"missing {path.relative_to(bundle)}")

    for library in NATIVE_LIBRARIES:
        notices = bundle / "licenses" / library
        if not any(file.is_file() and NOTICE.match(file.name) for file in notices.glob("*")):
            raise ValueError(f"missing license notice for {library}")

    macos = target == MACOS_TARGET
    suffix = "*.dylib" if macos else "*.so*"
    library_version = r"(?:\.[0-9]+)*\.dylib" if macos else r"\.so(?:\.[0-9]+)*"
    libraries = [*sdk.glob(f"lib/{suffix}"), *deps.glob(f"lib/{suffix}")]
    names = {lib.name for lib in libraries}
    for prefix in ("libhgraph_runtime", "libhgraph_wiring", "libfmt", "libspdlog", "libsimdjson"):
        if not any(name.startswith(prefix) for name in names):
            raise ValueError(f"missing {prefix} shared library")
    if sorted(p.name for p in deps.iterdir()) != ["include", "lib"]:
        raise ValueError("unexpected dependency prefix contents")
    for file in (deps / "lib").iterdir():
        if file.name != "cmake" and not re.fullmatch(
            r"lib(?:fmt|spdlog|simdjson)" + library_version, file.name,
        ):
            raise ValueError(f"unexpected dependency library: {file.name}")
    if not macos and names.intersection(LINUX_SYSTEM_LIBRARIES):
        raise ValueError("system C or C++ runtime bundled on Linux")

    prefixes = {
        str(candidate).encode()
        for path in forbidden
        for candidate in (path, path.resolve())
        if str(candidate) not in ("", "/")
    }
    # Ignore URL paths inside license notices, such as crates.io/crates/home.
    path_start = rb"(?:file://|(?<![\w./~%-]))"
    scratch_path = re.compile(
        path_start + rb"(?:(?:/private)?/var/folders/|/(?:Users|home|root)/|/tmp/hgl-release\.)[^\s\x00]+"
    )
    local_path = re.compile(path_start + rb"(?:" + b"|".join(map(re.escape, sorted(prefixes)))
                            + rb")(?![\w.~-])") if prefixes else None
    for path in bundle.rglob("*"):
        if path.is_symlink():
            if path.readlink().is_absolute() or not path.resolve(strict=True).is_relative_to(bundle):
                raise ValueError(f"link escapes the bundle: {path.relative_to(bundle)}")
            continue
        if path.is_file():
            data = path.read_bytes()
            if scratch_path.search(data) or (local_path and local_path.search(data)):
                raise ValueError(f"build path in {path.relative_to(bundle)}")
            if path.suffix in RELEASE_METADATA:
                verify_metadata_paths(bundle, path, data)

    for binary in {bin_file, *(lib.resolve(strict=True) for lib in libraries)}:
        if macos:
            if command("lipo", "-archs", binary).strip() != "arm64":
                raise ValueError(f"{binary.relative_to(bundle)} is not arm64")
            verify_load_paths(bundle, binary, names)
        else:
            verify_elf(bundle, binary, names)
    print(f"Verified {bundle.name}")


def verify_metadata_paths(bundle, path, data):
    """Metadata can refer to paths relative to its prefix or to macOS."""
    for match in ABSOLUTE_PATH.findall(data):
        absolute = Path(os.fsdecode(match)).resolve()
        in_system = any(absolute.is_relative_to(root) for root in SYSTEM_PATHS)
        if absolute.is_relative_to(bundle) or in_system:
            continue
        raise ValueError(f"external path in {path.relative_to(bundle)}")


def verify_load_paths(bundle, binary, names):
    """Each Mach-O name or run path must resolve within the bundle or macOS."""
    load_paths = command("otool", "-l", binary)
    for minimum in re.findall(r"^\s+minos ([0-9.]+)$", load_paths, re.MULTILINE):
        major, minor, *_ = (int(part) for part in minimum.split("."))
        if (major, minor) > (26, 0):
            raise ValueError(f"{binary.relative_to(bundle)} requires macOS {minimum}")
    for path in re.findall(r"^\s+(?:name|path) (.+) \(offset \d+\)$", load_paths, re.MULTILINE):
        if path.startswith(SYSTEM_LIBRARIES):
            continue
        if path.startswith("@rpath/") and path.removeprefix("@rpath/") in names:
            continue
        parent, _, relative = path.partition("/")
        if parent in ("@loader_path", "@executable_path"):
            base = binary.parent if parent == "@loader_path" else bundle / "bin"
            if (base / relative).resolve().is_relative_to(bundle):
                continue
        raise ValueError(f"external Mach-O load path in {binary.relative_to(bundle)}: {path}")


def verify_elf(bundle, binary, names):
    """Check x86-64 ELF linkage against the bundle and Ubuntu's system runtime."""
    relative = binary.relative_to(bundle)
    header = command("readelf", "--file-header", binary)
    if not re.search(r"^\s*Machine:\s+Advanced Micro Devices X86-64$", header, re.MULTILINE):
        raise ValueError(f"{relative} is not x86-64")
    dynamic = command("readelf", "--dynamic", "--wide", binary)
    for library in re.findall(r"\(NEEDED\).*\[(.+)\]", dynamic):
        if library not in names and library not in LINUX_SYSTEM_LIBRARIES:
            raise ValueError(f"external ELF library in {relative}: {library}")
    for paths in re.findall(r"\((?:RPATH|RUNPATH)\).*\[(.*)\]", dynamic):
        for path in paths.split(":"):
            origin, _, child = path.partition("/")
            if origin != "$ORIGIN" or not (binary.parent / child).resolve().is_relative_to(bundle):
                raise ValueError(f"external ELF load path in {relative}: {path}")
    versions = command("readelf", "--version-info", "--wide", binary)
    for version in re.findall(r"\bName: GLIBC_(\d+(?:\.\d+)+)\b", versions):
        if tuple(map(int, version.split("."))) > LINUX_GLIBC:
            raise ValueError(f"{relative} requires glibc {version}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    modes.add_parser("version")
    modes.add_parser("native-key")
    for name in ("copy-native", "save-native"):
        native = modes.add_parser(name)
        native.add_argument("source", type=Path)
        native.add_argument("destination", type=Path)
    reuse = modes.add_parser("find-verified-run")
    reuse.add_argument("repository")
    reuse.add_argument("commit")
    reuse.add_argument("current_run")
    licenses = modes.add_parser("licenses")
    licenses.add_argument("destination", type=Path)
    audit = modes.add_parser("verify")
    audit.add_argument("bundle", type=Path)
    audit.add_argument("--forbid", type=Path, action="append", default=[])
    for mode in (licenses, audit):
        mode.add_argument("--target", choices=[MACOS_TARGET, LINUX_TARGET], default=MACOS_TARGET)
    args = parser.parse_args()
    if args.mode == "version":
        print(version())
    elif args.mode == "native-key":
        print(native_cache_key())
    elif args.mode == "copy-native":
        copy_native(args.source, args.destination)
    elif args.mode == "save-native":
        save_native(args.source, args.destination)
    elif args.mode == "find-verified-run":
        found = verified_run(args.repository, args.commit, args.current_run)
        if found:
            run, macos, linux = found
            print(f"run-id={run}\nartifact-id={macos}\nlinux-artifact-id={linux}")
    elif args.mode == "licenses":
        rust_licenses(args.destination, args.target)
    else:
        verify(args.bundle, args.forbid, args.target)


if __name__ == "__main__":
    main()
