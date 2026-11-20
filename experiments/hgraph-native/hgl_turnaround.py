"""Time an HGL edit through the normal native application runner and verify each timestamp."""
import argparse
import csv
import json
import os
from datetime import datetime, timedelta
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
SCALAR = (ROOT / "spec/hgl_core/tests/conformance/scalar_eval.hgl").read_text()
KEYED = (ROOT / "crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl").read_text()


def source(name, result, events):
    previous = 0
    lines = [f"fn {name}() -> {result} {{"]
    for step, value in events:
        lines.append(f"    yield {step - previous}us: {value}")
        previous = step
    return "\n".join([*lines, "}"])


def inputs(case, edit, variant):
    """Use the existing fixture declarations as an imported module, exposing the entry."""
    changed = bool(variant)
    body = changed and edit in ("node-body", "signature")
    wiring = changed and edit == "wiring"
    signature = changed and edit == "signature"
    connection = "second" if wiring else "first"
    if case == "scalar":
        module = SCALAR.replace("module conformance part scalar_eval", "module bench.nodes")
        module = module.replace("fn count_publications(value: i64)", "export fn count_publications(value: i64" + (", const increment: i64 = 1" if signature else "") + ")")
        module = module.replace("count += 1", "count += " + ("increment" if signature else "2" if body else "1"))
        regular = [(0, 7), (2, 7), (3, 7)]
        extended = [*regular, (5, 9)]
        events = extended if wiring else regular
        sources = source("primary", "i64", regular) + "\n" + source("alternate", "i64", extended)
        call = "count_publications(" + connection + (", 2" if signature else "") + ")"
        imported = "count_publications"
        expected = [(s, i * (2 if body else 1)) for i, (s, _) in enumerate(events, 1)]
    else:
        module = KEYED.replace("module demo.graph_for", "module bench.nodes")
        module = module.replace("fn each_price(", "export fn each_price(")
        if signature:
            # Specialise a changed public argument into the imported graph's child.
            module = module.replace("fn counted(name: str, price: i64)", "fn counted(name: str, price: i64, const increment: i64 = 1)")
            module = module.replace("const shown: str) -> i64 {\n    for name, value in priced", "const shown: str, const increment: i64 = 1) -> i64 {\n    for name, value in priced")
            module = module.replace("counted(name, value)", "counted(name, value, increment)")
        module = module.replace("seen += 1", "seen += " + ("increment" if signature else "2" if body else "1"), 1)
        keys = ['"a"', '"b"', '"a"', '"a"', '"a"']
        sources = "\n".join([
            source("primary", "str", zip(range(0, 10, 2), keys)),
            source("alternate", "str", zip(range(0, 10, 2), [*keys[:2], '"b"', *keys[3:]])),
            source("prices", "i64", zip(range(0, 10, 2), [1, 7, 2, -1, 5])),
            source("probes", "bool", zip(range(1, 10, 2), ["true"] * 5)),
        ])
        imported = "each_price"
        call = f'each_price({connection}, prices(), probes(), "a"' + (", 2" if signature else "") + ")"
        values = [1, 1, 1 if wiring else 2, 1 if wiring else 2, 1]
        expected = [(s, v * (2 if body else 1)) for s, v in zip(range(1, 10, 2), values)]
    app = f"application bench.app\nuse bench.nodes::{{{imported}}}\n{sources}\nfn entry() -> i64 {{\n    let first = primary()\n    let second = alternate()\n    return {call}\n}}\neval(entry())\n"
    return module, app, expected


def write_changed(file, value):
    if not file.exists() or file.read_text() != value:
        file.write_text(value)


def sample(args, case, edit, variant):
    work = args.work / case
    work.mkdir(exist_ok=True, parents=True)
    module, app, expected = inputs(case, edit, variant)
    nodes, entry = work / "nodes.hgl", work / "app.hgl"
    # Include the HGL edit in the wall clock. Verify the independently specified trace.
    start = time.perf_counter_ns()
    write_changed(nodes, module)
    write_changed(entry, app)
    result = subprocess.run(
        [args.hgl, "run", "--target", "hgraph-native", "--keep", work / "generated", nodes, entry],
        text=True, capture_output=True, env={**os.environ, "HGL_NATIVE_TIMINGS":"1"},
    )
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    recorded = []
    for line in result.stdout.splitlines():
        if line.startswith("ran "):
            continue
        instant, value = line.split()
        delta = datetime.fromisoformat(instant[:-1]) - datetime(1970, 1, 1, microsecond=1)
        recorded.append((delta // timedelta(microseconds=1), int(value)))
    if recorded != expected:
        raise ValueError((case, edit, expected, recorded))
    elapsed = (time.perf_counter_ns() - start) / 1e6
    parts = {}
    for line in result.stderr.splitlines():
        if line.startswith("hgl-native-timing "):
            parts.update(json.loads(line.removeprefix("hgl-native-timing ")))
    return [f"{elapsed:.3f}", *[f"{parts[k]:.3f}" for k in ("frontend_ms","lower_ms","emit_ms","cargo_ms","launch_ms")], " ".join(parts["rebuilt"])]


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("hgl", type=Path)
    cli.add_argument("work", type=Path, help="fresh directory for samples and generated workspaces")
    cli.add_argument("--samples", default=15, type=int)
    args = cli.parse_args()
    for case in ("scalar","keyed"):
        before, after = (inputs(case, "wiring", variant) for variant in (False, True))
        changes = [(a,b) for a,b in zip(before[1].splitlines(), after[1].splitlines(), strict=True) if a != b]
        if before[0] != after[0] or before[2] == after[2] or len(changes) != 1 or changes[0][0].replace("(first", "(second", 1) != changes[0][1]:
            raise ValueError(f"{case}: the wiring edit must change only the selected connection")
    args.hgl = args.hgl.resolve()
    args.work.mkdir(parents=True, exist_ok=False)
    with (args.work / "hgl-turnaround.csv").open("w") as out:
        rows = csv.writer(out)
        rows.writerow(["case", "edit", "sample", "milliseconds","frontend_ms","lower_ms","emit_write_ms","cargo_ms","launch_ms","rebuilt"])
        for case in ["scalar", "keyed"]:
            rows.writerow([case, "first", 1, *sample(args, case, "unchanged", False)])
        for edit in ["unchanged", "wiring", "node-body", "signature"]:
            for case in ["scalar", "keyed"]:
                sample(args, case, edit, False)
            for i in range(args.samples):
                cases = ["scalar", "keyed"] if i % 2 == 0 else ["keyed", "scalar"]
                for case in cases:
                    elapsed = sample(args, case, edit, i % 2 == 0)
                    rows.writerow([case, edit, i + 1, *elapsed])
            out.flush()


if __name__ == "__main__":
    main()
