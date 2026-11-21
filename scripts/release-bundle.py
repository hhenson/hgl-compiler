"""Collect release license notices and check the finished macOS bundle."""

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tomllib
from urllib.request import urlopen

TARGET = "aarch64-apple-darwin"
NOTICE = re.compile(r"^(?:copying|license|notice|unlicense)", re.IGNORECASE)


def command(*args):
    return subprocess.check_output(args, text=True)


def version():
    with Path("Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["workspace"]["package"]["version"]


def rust_licenses(destination):
    """Collect notices from the Rust distribution and the Cargo build graph."""
    std = Path(command("rustc", "--print", "sysroot").strip()) / "share/doc/rust"
    shutil.copytree(std / "licenses", destination / "rust")
    for copyright_file in std.glob("COPYRIGHT*.html"):
        shutil.copy2(copyright_file, destination / "rust")

    target = ["--target", TARGET, "--locked"]
    tree = command(
        "cargo", "tree", "-p", "hgl_cli", *target,
        "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
    )
    used = {tuple(line.split()[:2]) for line in tree.splitlines()}
    metadata = json.loads(command(
        "cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET,
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


def verify(bundle, forbidden):
    """Check paths, dylib install names, architecture and bundled libraries."""
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

    libraries = [*sdk.glob("lib/*.dylib"), *deps.glob("lib/*.dylib")]
    names = {lib.name for lib in libraries}
    for prefix in ("libhgraph_runtime", "libhgraph_wiring", "libfmt", "libspdlog", "libsimdjson"):
        if not any(name.startswith(prefix) for name in names):
            raise ValueError(f"missing {prefix} dylib")
    if any(not lib.name.startswith(("libfmt.", "libspdlog.", "libsimdjson."))
           for lib in deps.glob("lib/*.dylib")):
        raise ValueError("unexpected dependency dylib")
    if sorted(p.name for p in deps.iterdir()) != ["include", "lib"]:
        raise ValueError("unexpected dependency prefix contents")

    prefixes = {str(candidate).encode() for path in forbidden for candidate in (path, path.resolve())
                if str(candidate) not in ("", "/")}
    scratch_path = re.compile(rb"(?:/private)?/var/folders/[^\s\x00]+|/Users/[^\s\x00]+")
    for path in bundle.rglob("*"):
        if path.is_symlink():
            if path.readlink().is_absolute() or not path.resolve(strict=True).is_relative_to(bundle):
                raise ValueError(f"link escapes the bundle: {path.relative_to(bundle)}")
            continue
        if path.is_file():
            data = path.read_bytes()
            if any(prefix in data for prefix in prefixes) or scratch_path.search(data):
                raise ValueError(f"build path in {path.relative_to(bundle)}")

    for binary in {bin_file, *(lib.resolve(strict=True) for lib in libraries)}:
        if command("lipo", "-archs", binary).strip() != "arm64":
            raise ValueError(f"{binary.relative_to(bundle)} is not arm64")
        verify_load_paths(bundle, binary, names)
    print(f"Verified {bundle.name}")


def verify_load_paths(bundle, binary, names):
    """Each Mach-O name or run path must resolve within the bundle or macOS."""
    load_paths = command("otool", "-l", binary)
    for path in re.findall(r"^\s+(?:name|path) (.+) \(offset \d+\)$", load_paths, re.MULTILINE):
        if path.startswith(("/usr/lib/", "/System/Library/")):
            continue
        if path.startswith("@rpath/") and path.removeprefix("@rpath/") in names:
            continue
        parent, slash, relative = path.partition("/")
        if parent in ("@loader_path", "@executable_path"):
            base = binary.parent if parent == "@loader_path" else bundle / "bin"
            if (base / relative).resolve().is_relative_to(bundle):
                continue
        raise ValueError(f"external Mach-O load path in {binary.relative_to(bundle)}: {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    modes.add_parser("version")
    modes.add_parser("licenses").add_argument("destination", type=Path)
    audit = modes.add_parser("verify")
    audit.add_argument("bundle", type=Path)
    audit.add_argument("--forbid", type=Path, action="append", default=[])
    args = parser.parse_args()
    if args.mode == "version":
        print(version())
    elif args.mode == "licenses":
        rust_licenses(args.destination)
    else:
        verify(args.bundle, args.forbid)


if __name__ == "__main__":
    main()
