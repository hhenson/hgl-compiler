"""Run the pinned core suites on the cpp target and check its known gaps (TST-14, TST-15).

The suites run on the C++ target, `hgl test --target cpp`, and are checked
against `unsupported_cases.toml` beside this file; the build needs the SDK
`HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name only for a case it can run.
`--write-list` writes the list from the run instead of checking it: each
case not run for a form the target does not write, under that form; a case
that fails is never listed.
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
# The target the cases run on, and the list of those it is known not to run.
TARGET = "cpp"
LIST = Path(__file__).with_name("unsupported_cases.toml")
# The row detail of a case the target does not write all of.
UNSUPPORTED = re.compile(rf".+\.hgl:\d+:\d+: the {TARGET} target does not write (.+) yet")

# A rejection case's row names its file, its line and the declaration it
# is, as its source writes it: `.../tests/conformance/rejections/x.hgl:46
# fn name`, or `... const fn name`, the name the last word. Its failure to
# be rejected is "expected category/code on line N of file: not reported".
REJECTION = re.compile(r"^(?:.*/)?tests/([^/]+)/(.+?):\d+ (?:\S+ )+(\S+)$")
NOT_REJECTED = re.compile(r"^expected \S+ on line \d+ of .+: not reported$")
SUITES = {"conformance": "conformance", "implementation": "core_implementation"}


def known_gaps():
    """Every listed case, by name, with its reason, and the names of the
    rejection cases among them: the executable cases the target does not
    write, and, under `[rejections]`, the rejection cases the compiler
    does not yet reject, each `suite::file::declaration` with `file` under
    the suite's directory."""
    with LIST.open("rb") as file:
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


def verify(discovered, result, expected, unrejected=frozenset()):
    """Require every discovered case and account for every non-passing
    outcome. A rejection case in `unrejected` must fail by not being
    rejected, as listed: rejected, or failing another way, it fails the
    check. A case not run must be so for the form its listing names, in
    the target's words."""
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
    if result.returncode != status:
        problems.append(f"unexpected runner exit status {result.returncode}")
    return problems


def gaps_of(result):
    """Each case of `result` not run for a form the target does not write,
    by name, with that form."""
    gaps = {}
    for (kind, name), (status, detail) in case_rows(result.stdout).items():
        diagnostic = UNSUPPORTED.fullmatch(detail or "")
        if kind == "executable" and status == "not run" and diagnostic:
            gaps[name] = diagnostic[1]
    return gaps


def toml_string(text):
    """`text` as a TOML basic string."""
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def write_list(path, gaps, unrejected):
    """Writes `gaps`, each case by name with its reason, to the list at
    `path`, keeping its first comment and the comment above its
    `[rejections]` table, whose names are `unrejected` by reason."""
    lines = path.read_text(encoding="utf-8").splitlines()
    header = []
    for line in lines:
        if not line.startswith("#"):
            break
        header.append(line)
    table = lines.index("[rejections]") if "[rejections]" in lines else len(lines)
    above = []
    for line in reversed(lines[len(header):table]):
        if not line.startswith("#"):
            break
        above.insert(0, line)
    by_reason = {}
    for name, reason in gaps.items():
        by_reason.setdefault(reason, []).append(name)
    written = header + [""]
    for reason in sorted(by_reason):
        written.append(f"{toml_string(reason)} = [")
        written.extend(f"    {toml_string(name)}," for name in sorted(by_reason[reason]))
        written.append("]")
    written += [""] + above + ["[rejections]"]
    for reason in sorted(unrejected):
        names = ", ".join(toml_string(name) for name in sorted(unrejected[reason]))
        written.append(f"{toml_string(reason)} = [{names}]")
    path.write_text("\n".join(written) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hgl", type=Path, help="built HGL compiler")
    parser.add_argument("core", type=Path, help="core checkout containing hgl/ and tests/")
    parser.add_argument("--target", choices=[TARGET], default=TARGET,
                        help="the target to run the cases on, the one there is")
    parser.add_argument("--write-list", action="store_true",
                        help="write the list of cases not run from this run")
    args = parser.parse_args()
    gaps, unrejected = known_gaps()
    problems = []
    found = {}
    target = ["--target", args.target]
    for suite, module in SUITES.items():
        command = [args.hgl.resolve(), "test", args.core / "hgl", args.core / "tests" / suite, "--integration"]
        print(f"Running {suite}", flush=True)
        discovered = subprocess.run(command, stdout=subprocess.PIPE, text=True)
        result = subprocess.run(command + target, stdout=subprocess.PIPE, text=True)
        print(result.stdout, end="")
        found.update(gaps_of(result))
        expected = {name: reason for name, reason in gaps.items() if name.startswith(module + "::")}
        problems.extend(verify(discovered, result, expected, unrejected))
    if args.write_list:
        with LIST.open("rb") as file:
            rejections = tomllib.load(file).get("rejections", {})
        write_list(LIST, found, rejections)
        print(f"wrote {len(found)} cases to {LIST}")
        return 0
    for problem in problems:
        print(problem, file=sys.stderr)
    return int(bool(problems))


if __name__ == "__main__":
    sys.exit(main())
