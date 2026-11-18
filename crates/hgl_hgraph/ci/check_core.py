"""Run the pinned core suites and check hgraph's known target gaps (TST-14, TST-15)."""

import argparse
from collections import Counter
from pathlib import Path
import re
import subprocess
import sys
import tomllib


# These are the CLI's case rows, shared by discovery and execution. A target
# gap is source-located by the CLI and worded by the hgraph emitter.
CASE = re.compile(
    r"^(passed|FAILED|not run) +(rejection|executable) +([^\n]+)(?:\n    ([^\n]+))?$",
    re.MULTILINE,
)
UNSUPPORTED = re.compile(r".+\.hgl:\d+:\d+: the hgraph target does not write (.+) yet")
SUITES = {"conformance": "conformance", "implementation": "core_implementation"}


def known_gaps():
    with Path(__file__).with_name("unsupported_cases.toml").open("rb") as file:
        groups = tomllib.load(file)
    gaps = {}
    for reason, names in groups.items():
        for name in names:
            if name in gaps or name.partition("::")[0] not in SUITES.values():
                raise ValueError(f"duplicate or unknown case in unsupported_cases.toml: {name}")
            gaps[name] = reason
    return gaps


def case_rows(output):
    rows = {}
    for status, kind, name, detail in CASE.findall(output):
        key = (kind, name)
        if key in rows:
            raise ValueError(f"duplicate case in runner output: {name}")
        rows[key] = (status, detail)
    return rows


def verify(discovered, result, expected):
    """Require every discovered case and account for every non-passing outcome."""
    problems = []
    inventory = case_rows(discovered.stdout)
    actual = case_rows(result.stdout)
    if discovered.returncode != 2 or not any(kind == "executable" for kind, _ in inventory):
        problems.append("the runner could not list the suite's executable cases")
    for kind, name in sorted(inventory.keys() ^ actual.keys()):
        problems.append(f"discovery and execution disagree on {kind} {name}")

    counts = Counter()
    blocked = set()
    for (kind, name), (status, detail) in actual.items():
        counts[kind, status] += 1
        if status == "FAILED":
            problems.append(f"{kind} failed: {name}")
        if status == "not run":
            diagnostic = UNSUPPORTED.fullmatch(detail)
            if kind != "executable" or not diagnostic or expected.get(name) != diagnostic[1]:
                problems.append(f"unexpected reason or case not run: {name}: {detail}")
            blocked.add(name)

    for name in sorted(expected.keys() - blocked):
        status = actual.get(("executable", name), ("missing", ""))[0]
        problems.append(f"listed target gap is now {status}: {name}")
    passed = counts["executable", "passed"]
    failed = counts["executable", "FAILED"]
    waiting = counts["executable", "not run"]
    summaries = (
        f"rejection cases: {counts['rejection', 'passed']} passed, {counts['rejection', 'FAILED']} failed",
        f"executable cases: {passed} passed, {failed} failed, {waiting} not run",
    )
    if result.stdout.splitlines()[-2:] != list(summaries):
        problems.append("the runner's final counts do not match its case rows")
    if not passed or result.returncode != (2 if waiting else 0):
        problems.append(f"no passing executable or unexpected runner exit status {result.returncode}")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hgl", type=Path, help="built HGL compiler")
    parser.add_argument("core", type=Path, help="core checkout containing hgl/ and tests/")
    parser.add_argument("--python", default=sys.executable, help="interpreter with hgraph")
    args = parser.parse_args()
    gaps = known_gaps()
    problems = []
    for suite, module in SUITES.items():
        command = [args.hgl.resolve(), "test", args.core / "hgl", args.core / "tests" / suite, "--integration"]
        print(f"Running {suite}", flush=True)
        discovered = subprocess.run(command, stdout=subprocess.PIPE, text=True)
        result = subprocess.run(command + ["--target", "hgraph", "--python", args.python],
                                stdout=subprocess.PIPE, text=True)
        print(result.stdout, end="")
        expected = {name: reason for name, reason in gaps.items() if name.startswith(module + "::")}
        problems.extend(verify(discovered, result, expected))
    for problem in problems:
        print(problem, file=sys.stderr)
    return int(bool(problems))


if __name__ == "__main__":
    sys.exit(main())
