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
# A rejection case's row names its file, its line and the declaration it
# is: `.../tests/conformance/rejections/x.hgl:46 fn name`. Its failure to
# be rejected is "expected category/code on line N of file: not reported".
REJECTION = re.compile(r"^(?:.*/)?tests/([^/]+)/(.+):\d+ \S+ (\S+)$")
NOT_REJECTED = re.compile(r"^expected \S+ on line \d+ of .+: not reported$")
SUITES = {"conformance": "conformance", "implementation": "core_implementation"}


def known_gaps():
    """Every listed case, by name, with its reason, and the names of the
    rejection cases among them: the executable cases the hgraph target does
    not write, and, under `[rejections]`, the rejection cases the compiler
    does not yet reject, each `suite::file::declaration` with `file` under
    the suite's directory."""
    with Path(__file__).with_name("unsupported_cases.toml").open("rb") as file:
        groups = tomllib.load(file)
    rejections = groups.pop("rejections", {})
    gaps = {}
    for reason, names in [*groups.items(), *rejections.items()]:
        for name in names:
            if name in gaps or name.partition("::")[0] not in SUITES.values():
                raise ValueError(f"duplicate or unknown case in unsupported_cases.toml: {name}")
            gaps[name] = reason
    return gaps, {name for names in rejections.values() for name in names}


def rejection_key(name):
    """A rejection case's row name as `unsupported_cases.toml` lists it,
    `conformance::rejections/x.hgl::name`, or `None` for another row."""
    found = REJECTION.match(name)
    if not found:
        return None
    suite, file, declaration = found.groups()
    return f"{SUITES.get(suite, suite)}::{file}::{declaration}"


def case_rows(output):
    rows = {}
    for status, kind, name, detail in CASE.findall(output):
        key = (kind, name)
        if key in rows:
            raise ValueError(f"duplicate case in runner output: {name}")
        rows[key] = (status, detail)
    return rows


def verify(discovered, result, expected, unrejected):
    """Require every discovered case and account for every non-passing
    outcome. A rejection case in `unrejected` must fail by not being
    rejected, as listed: rejected, or failing another way, it fails the
    check."""
    problems = []
    inventory = case_rows(discovered.stdout)
    actual = case_rows(result.stdout)
    # Discovery runs no executable case, so it exits 2, or 1 where a
    # rejection case failed, which is accounted for below.
    unrejected_rows = any(
        kind == "rejection" and status == "FAILED" for (kind, _), (status, _) in inventory.items()
    )
    listing = 1 if unrejected_rows else 2
    if discovered.returncode != listing or not any(kind == "executable" for kind, _ in inventory):
        problems.append("the runner could not list the suite's executable cases")
    for kind, name in sorted(inventory.keys() ^ actual.keys()):
        problems.append(f"discovery and execution disagree on {kind} {name}")

    counts = Counter()
    blocked = set()
    for (kind, name), (status, detail) in actual.items():
        counts[kind, status] += 1
        listed = rejection_key(name) if kind == "rejection" else None
        if listed in unrejected:
            if not (status == "FAILED" and NOT_REJECTED.fullmatch(detail or "")):
                problems.append(f"listed rejection case is now {status}: {name}")
            blocked.add(listed)
            continue
        if status == "FAILED":
            problems.append(f"{kind} failed: {name}")
        if status == "not run":
            diagnostic = UNSUPPORTED.fullmatch(detail)
            if kind != "executable" or not diagnostic or expected.get(name) != diagnostic[1]:
                problems.append(f"unexpected reason or case not run: {name}: {detail}")
            blocked.add(name)

    for name in sorted(expected.keys() - blocked):
        if name in unrejected:
            problems.append(f"listed rejection case is now missing: {name}")
            continue
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
    # The runner exits 1 where a case failed, a listed rejection among them.
    status = 1 if counts["rejection", "FAILED"] or failed else 2 if waiting else 0
    if not passed or result.returncode != status:
        problems.append(f"no passing executable or unexpected runner exit status {result.returncode}")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hgl", type=Path, help="built HGL compiler")
    parser.add_argument("core", type=Path, help="core checkout containing hgl/ and tests/")
    parser.add_argument("--python", default=sys.executable, help="interpreter with hgraph")
    args = parser.parse_args()
    gaps, unrejected = known_gaps()
    problems = []
    for suite, module in SUITES.items():
        command = [args.hgl.resolve(), "test", args.core / "hgl", args.core / "tests" / suite, "--integration"]
        print(f"Running {suite}", flush=True)
        discovered = subprocess.run(command, stdout=subprocess.PIPE, text=True)
        result = subprocess.run(command + ["--target", "hgraph", "--python", args.python],
                                stdout=subprocess.PIPE, text=True)
        print(result.stdout, end="")
        expected = {name: reason for name, reason in gaps.items() if name.startswith(module + "::")}
        problems.extend(verify(discovered, result, expected, unrejected))
    for problem in problems:
        print(problem, file=sys.stderr)
    return int(bool(problems))


if __name__ == "__main__":
    sys.exit(main())
