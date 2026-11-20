"""Run the pinned core suites on a target and check its known gaps (TST-14, TST-15).

`--target hgraph` (the default) runs them on the Python target, with the
interpreter `--python` names, and checks them against
`unsupported_cases.toml` beside this file. `--target cpp` runs them on the
C++ target and checks them against `crates/hgl_cpp/ci/unsupported_cases.toml`;
its build needs the SDK `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name only for
a case it can run. `--write-list` writes the target's list from the run
instead of checking it: each case not run for a form the target does not
write, under that form; a case that fails is never listed.
"""

import argparse
from collections import Counter
from pathlib import Path
import re
import subprocess
import sys
import tomllib


# These are the CLI's case rows, shared by discovery and execution. A target
# gap is source-located by the CLI and worded by the target's emitter.
CASE = re.compile(
    r"^(passed|FAILED|not run) +(rejection|executable) +([^\n]+)(?:\n    ([^\n]+))?$",
    re.MULTILINE,
)
# Each target, and the list of the cases it is known not to run.
TARGETS = {
    "hgraph": Path(__file__).with_name("unsupported_cases.toml"),
    "cpp": Path(__file__).parents[2] / "hgl_cpp" / "ci" / "unsupported_cases.toml",
}


def unsupported(target):
    """The row detail of a case `target` does not write all of."""
    return re.compile(rf".+\.hgl:\d+:\d+: the {re.escape(target)} target does not write (.+) yet")

# A rejection case's row names its file, its line and the declaration it
# is, as its source writes it: `.../tests/conformance/rejections/x.hgl:46
# fn name`, or `... const fn name`, the name the last word. Its failure to
# be rejected is "expected category/code on line N of file: not reported".
REJECTION = re.compile(r"^(?:.*/)?tests/([^/]+)/(.+?):\d+ (?:\S+ )+(\S+)$")
NOT_REJECTED = re.compile(r"^expected \S+ on line \d+ of .+: not reported$")
SUITES = {"conformance": "conformance", "implementation": "core_implementation"}


def known_gaps(target="hgraph"):
    """Every listed case, by name, with its reason, and the names of the
    rejection cases among them: the executable cases `target` does not
    write, and, under `[rejections]`, the rejection cases the compiler
    does not yet reject, each `suite::file::declaration` with `file` under
    the suite's directory."""
    with TARGETS[target].open("rb") as file:
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


def verify(discovered, result, expected, unrejected=frozenset(), target="hgraph"):
    """Require every discovered case and account for every non-passing
    outcome. A rejection case in `unrejected` must fail by not being
    rejected, as listed: rejected, or failing another way, it fails the
    check. A case not run must be so for the form its listing names, in
    `target`'s words. The hgraph target must pass a case; another target
    may yet pass none, where every case it does not run is listed."""
    pattern = unsupported(target)
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
            diagnostic = pattern.fullmatch(detail)
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
    if (target == "hgraph" and not passed) or result.returncode != status:
        problems.append(f"no passing executable or unexpected runner exit status {result.returncode}")
    return problems


def gaps_of(result, target):
    """Each case of `result` not run for a form `target` does not write,
    by name, with that form."""
    pattern = unsupported(target)
    gaps = {}
    for (kind, name), (status, detail) in case_rows(result.stdout).items():
        diagnostic = pattern.fullmatch(detail or "")
        if kind == "executable" and status == "not run" and diagnostic:
            gaps[name] = diagnostic[1]
    return gaps


def toml_string(text):
    """`text` as a TOML basic string."""
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def write_list(path, gaps, unrejected):
    """Writes `gaps`, each case by name with its reason, to the list at
    `path`, keeping its leading comments and its `[rejections]` table,
    whose names are `unrejected` by reason."""
    header = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("#") and line.strip():
            break
        header.append(line)
    while header and not header[-1].strip():
        header.pop()
    by_reason = {}
    for name, reason in gaps.items():
        by_reason.setdefault(reason, []).append(name)
    lines = header + [""]
    for reason in sorted(by_reason):
        lines.append(f"{toml_string(reason)} = [")
        lines.extend(f"    {toml_string(name)}," for name in sorted(by_reason[reason]))
        lines.append("]")
    lines.append("")
    lines.append("[rejections]")
    for reason in sorted(unrejected):
        names = ", ".join(toml_string(name) for name in sorted(unrejected[reason]))
        lines.append(f"{toml_string(reason)} = [{names}]")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hgl", type=Path, help="built HGL compiler")
    parser.add_argument("core", type=Path, help="core checkout containing hgl/ and tests/")
    parser.add_argument("--target", choices=sorted(TARGETS), default="hgraph",
                        help="the target to run the cases on")
    parser.add_argument("--python", default=sys.executable, help="interpreter with hgraph")
    parser.add_argument("--write-list", action="store_true",
                        help="write the target's list of cases not run from this run")
    args = parser.parse_args()
    gaps, unrejected = known_gaps(args.target)
    problems = []
    found = {}
    target = ["--target", args.target]
    if args.target == "hgraph":
        target += ["--python", args.python]
    for suite, module in SUITES.items():
        command = [args.hgl.resolve(), "test", args.core / "hgl", args.core / "tests" / suite, "--integration"]
        print(f"Running {suite}", flush=True)
        discovered = subprocess.run(command, stdout=subprocess.PIPE, text=True)
        result = subprocess.run(command + target, stdout=subprocess.PIPE, text=True)
        print(result.stdout, end="")
        found.update(gaps_of(result, args.target))
        expected = {name: reason for name, reason in gaps.items() if name.startswith(module + "::")}
        problems.extend(verify(discovered, result, expected, unrejected, args.target))
    if args.write_list:
        with TARGETS[args.target].open("rb") as file:
            rejections = tomllib.load(file).get("rejections", {})
        write_list(TARGETS[args.target], found, rejections)
        print(f"wrote {len(found)} cases to {TARGETS[args.target]}")
        return 0
    for problem in problems:
        print(problem, file=sys.stderr)
    return int(bool(problems))


if __name__ == "__main__":
    sys.exit(main())
