"""Linux comparison of hand-written HGL target output. Uses a fresh scratch directory."""

import argparse
import hashlib
import json
import os
import shutil
import statistics
import subprocess
import time
from pathlib import Path


def command(args, **kwargs):
    result = subprocess.run(
        list(map(str, args)), capture_output=True, text=True, check=False, **kwargs
    )
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed\n{result.stdout}\n{result.stderr}")
    return result.stdout


def elapsed(fn, *args):
    start = time.perf_counter_ns()
    value = fn(*args)
    return (time.perf_counter_ns() - start) / 1e6, value


def edit(source, changes, enabled):
    for name, old, new in changes:
        path = source / name
        original = path.read_text()
        a, b = (old, new) if enabled else (new, old)
        if original.count(a) != 1:
            raise RuntimeError(f"expected one occurrence of {a!r} in {name}")
        path.write_text(original.replace(a, b))


EDITS = {
    "wiring": {
        "cpp": [("cpp/app.cpp", "wiring_bias = 0", "wiring_bias = 1")],
        "rust": [
            ("rust/app/src/main.rs", "WIRING_BIAS: u32 = 0", "WIRING_BIAS: u32 = 1")
        ],
    },
    "body": {
        "cpp": [("cpp/nodes.cpp", "body_bias = 0", "body_bias = 1")],
        "rust": [("rust/nodes/src/lib.rs", "BODY_BIAS: i64 = 0", "BODY_BIAS: i64 = 1")],
    },
    "signature": {
        "cpp": [
            (
                "cpp/nodes.hpp",
                "wire_offset(unsigned base)",
                "wire_offset(unsigned base, unsigned extra)",
            ),
            (
                "cpp/nodes.cpp",
                "wire_offset(unsigned base) { return base; }",
                "wire_offset(unsigned base, unsigned extra) { return base + extra; }",
            ),
            ("cpp/app.cpp", "wire_offset(base)", "wire_offset(base, 1)"),
        ],
        "rust": [
            (
                "rust/nodes/src/lib.rs",
                "wire_offset(base: u32) -> u32 {\n    base",
                "wire_offset(base: u32, extra: u32) -> u32 {\n    base + extra",
            ),
            ("rust/app/src/main.rs", "wire_offset(base)", "wire_offset(base, 1)"),
        ],
    },
}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("sdk", type=Path)
    cli.add_argument("dependencies", type=Path)
    cli.add_argument(
        "work", type=Path, help="a new directory for build trees and raw results"
    )
    cli.add_argument("--pairs", type=int, default=9)
    cli.add_argument("--repetitions", type=int, default=50000)
    args = cli.parse_args()
    if args.pairs < 5 or args.repetitions < 1:
        cli.error("use at least five pairs and a positive repetition count")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    source = work / "source"
    template = Path(__file__).resolve().parent
    if any(work.is_relative_to(template / name) for name in ("rust", "cpp")):
        cli.error("place the work directory outside the application source directories")
    source.mkdir()
    for filename in ("CMakeLists.txt", "native.hpp", "runtime.cpp", "bridge.cpp"):
        shutil.copy2(template / filename, source / filename)
    for name in ("cpp", "rust"):
        shutil.copytree(
            template / name, source / name, ignore=shutil.ignore_patterns("target")
        )
    sdk, deps = args.sdk.resolve(), args.dependencies.resolve()
    rustc = command(["rustc", "-vV"])
    host = next(x[6:] for x in rustc.splitlines() if x.startswith("host: "))
    ld = (
        Path(command(["rustc", "--print", "sysroot"]).strip())
        / "lib/rustlib"
        / host
        / "bin/gcc-ld/ld.lld"
    )
    ld_version = command([ld, "--version"]).strip()
    if not ld_version.startswith("LLD 23.1.5 "):
        raise RuntimeError(f"unexpected linker: {ld_version}")
    provider = work / "provider"
    ld_flags = f"-B{ld.parent} -fuse-ld=lld"
    trace = work / "link-time.json"
    link_trace = f"-Wl,--time-trace={trace}"
    env = dict(
        os.environ,
        LD_LIBRARY_PATH=os.pathsep.join(
            map(str, [provider, sdk / "lib", deps / "lib"])
        ),
        CARGO_TARGET_DIR=str(work / "cargo"),
        CARGO_BUILD_JOBS="2",
        RUSTFLAGS=f"-L native={provider} -C linker=c++ -C link-arg=-B{ld.parent} -C link-arg=-fuse-ld=lld -C link-arg={link_trace}",
    )

    def execute(args):
        return command(args, env=env)

    def configure(build, flavor, options):
        return execute(
            [
                "cmake",
                "-G",
                "Ninja",
                "-S",
                source,
                "-B",
                build,
                "-DCMAKE_BUILD_TYPE=" + flavor,
                "-DCMAKE_CXX_FLAGS=" + ld_flags,
                "-DCMAKE_EXE_LINKER_FLAGS=" + link_trace,
                *options,
            ]
        )

    sdk_options = [
        f"-DCMAKE_PREFIX_PATH={sdk};{deps}",
        "-DHGRAPH_USE_PYARROW_ARROW=OFF",
    ]
    provider_ms, _ = elapsed(
        lambda: (
            configure(provider, "Release", sdk_options),
            execute(["cmake", "--build", provider, "--target", "bench_runtime", "-j2"]),
        )
    )
    lib = provider / "libbench_runtime.so"
    dev = work / "cpp-dev"
    release = work / "cpp-release"
    cargo = [
        "cargo",
        "build",
        "--locked",
        "--offline",
        "--manifest-path",
        source / "rust/Cargo.toml",
        "--message-format=json",
    ]
    # Both build systems and linkers must be able to find dependent shared libraries.
    profile = {False: ("Debug", dev), True: ("Release", release)}

    def build(lang, opt=False):
        if lang == "cpp":
            flavor, builddir = profile[opt]
            if not (builddir / "build.ninja").exists():
                configure(builddir, flavor, ["-DBENCH_RUNTIME=" + str(lib)])
            log = builddir / ".ninja_log"
            before = len(log.read_text().splitlines()) if log.exists() else 1
            execute(["ninja", "-C", builddir, "-j2", "cpp_case"])
            steps = log.read_text().splitlines()[before:]
            return [
                {
                    "unit": s.split("\t")[3],
                    "ms": int(s.split("\t")[1]) - int(s.split("\t")[0]),
                }
                for s in steps
            ]
        log = execute([*cargo, *(["--release"] if opt else [])])
        return [
            v["target"]["name"]
            for line in log.splitlines()
            if (v := json.loads(line)).get("reason") == "compiler-artifact"
            and not v["fresh"]
        ]

    def run(lang, case, reps, offset, opt=False):
        binary = (
            profile[opt][1] / "cpp_case"
            if lang == "cpp"
            else work / "cargo" / ("release" if opt else "debug") / "rust_case"
        )
        fields = list(map(int, execute([binary, case, reps, offset]).split()))
        return {"ticks": fields[0], "ns": fields[1], "outputs": fields[2:]}

    facade_ms, _ = elapsed(lambda: execute([*cargo, "-p", "facade"]))
    results = {
        "provider_ms": provider_ms,
        "facade_ms": facade_ms,
        "pairs": args.pairs,
        "repetitions": args.repetitions,
        "linker": ld_version,
        "linker_sha256": hashlib.sha256(ld.read_bytes()).hexdigest(),
        "rustc": rustc.splitlines()[0],
        "cxx": execute(["c++", "--version"]).splitlines()[0],
        "first": {},
        "edits": [],
        "rates": [],
    }
    langs = ("cpp", "rust")
    for lang in langs:
        ms, units = elapsed(build, lang)
        results["first"][lang] = {"build_ms": ms, "units": units}
        for case in ("scalar", "keyed"):
            run(lang, case, 1, 0)
    for name in ("unchanged", *EDITS):
        for i in range(args.pairs):
            for lang in langs if i % 2 == 0 else reversed(langs):
                altered = i % 2 == 0 and name != "unchanged"
                if name != "unchanged":
                    edit(source, EDITS[name][lang], altered)
                trace.unlink(missing_ok=True)
                wall, (build_ms, units, samples) = elapsed(
                    timed_edit, build, run, lang, int(altered)
                )
                events = (
                    json.loads(trace.read_text())["traceEvents"]
                    if trace.exists()
                    else []
                )
                link_ms = (
                    max((e["dur"] for e in events if "dur" in e), default=0) / 1000
                )
                if name == "unchanged" and (units or events):
                    raise RuntimeError(f"unchanged build compiled {units}")
                results["edits"].append(
                    {
                        "scenario": name,
                        "lang": lang,
                        "pair": i,
                        "total_ms": wall,
                        "build_ms": build_ms,
                        "link_ms": link_ms,
                        "units": units,
                        "launch_ms": samples,
                    }
                )
        if name != "unchanged" and args.pairs % 2:
            for lang in langs:
                edit(source, EDITS[name][lang], False)
                build(lang)

    for lang in langs:
        build(lang, True)
    for case in ("scalar", "keyed"):
        for lang in langs:
            run(lang, case, args.repetitions // 10 or 1, 0, True)
        for i in range(args.pairs):
            pair = {}
            for lang in langs if i % 2 == 0 else reversed(langs):
                pair[lang] = run(lang, case, args.repetitions, 0, True)
            if (
                pair["cpp"]["outputs"] != pair["rust"]["outputs"]
                or pair["cpp"]["ticks"] != pair["rust"]["ticks"]
            ):
                raise RuntimeError("traces differ")
            results["rates"].append(dict(case=case, pair=i, **pair))
    (work / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    for case in ("scalar", "keyed"):
        rows = [r for r in results["rates"] if r["case"] == case]
        print(
            case,
            "median paired Rust/C++",
            statistics.median(r["cpp"]["ns"] / r["rust"]["ns"] for r in rows),
        )


def timed_edit(build, run, lang, offset):
    ms, units = elapsed(build, lang)
    launches = {
        case: elapsed(run, lang, case, 1, offset)[0] for case in ("scalar", "keyed")
    }
    return ms, units, launches


if __name__ == "__main__":
    main()
