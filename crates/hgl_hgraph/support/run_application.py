"""Runs an application of a program written by the hgl compiler for hgraph.

Usage: _run_application.py OUTPUT_DIRECTORY MODULE FUNCTION RESULTS MODE

MODULE is the application's Python module and FUNCTION the function its
run statement is written as. MODE is `real-time` or `simulation`, as the
statement's word has it. RESULTS is written as the run goes, one line
for each thing that happens:

    start               the application is about to be loaded and run
    published TIME TEXT one publication of the run's output, in time
                        order: its instant, and what was published
    stopped             the run ended, and every publication of its
                        output is above
    stopped none        the run ended, and its entry has no result
    host                the host asked the run to stop, before `stopped`
    failed TEXT         the run failed, and has no output
    unloaded TEXT       the module or the function could not be loaded

TEXT has `\\`, `\n` and `\r` for a backslash, a line feed and a
carriage return. A `start` with nothing after it is a run that was
under way when the interpreter stopped.

What the program logs and what hgraph prints goes to standard error,
whichever of the two streams it was written to: standard output is
left to what reports the run's output.

The run is on a thread of its own, so that this one is free to take an
interrupt: hgraph waits for a run's next cycle where no signal reaches
Python. The host asks the run to stop in one of two ways: an interrupt
of this process, or a line `stop` on standard input, which what started
this process writes for a request that reached it alone. Standard
input closed is such a request too: what started the run is gone.

A run in real time that is asked to stop ends as it does at its end. A
simulation waits for nothing, so the interpreter ends at once, and a
second interrupt ends it at once whatever the mode.
"""

import datetime
import enum
import importlib
import json
import os
import signal
import sys
import threading

# What this driver shares with the other, from beside it: Python put
# this script's directory first on its path.
import _driver


def _described(error):
    """An error as a person reads it: the HGL error with a code under
    what hgraph raised, where there is one."""
    # An HGL error is the provider's: where one was raised, the provider
    # is loaded.
    core = sys.modules.get("_hgl._core_native")
    if core is not None:
        for found in core._causes(error):
            if isinstance(found, core.HglError) and found.code is not None:
                return f"{found.code}: {found}"
    return f"{type(error).__name__}: {error}"


def _instant(value):
    """A `datetime` as HGL writes one: in UTC, with the fraction of a
    second where it has one."""
    return value.isoformat() + "Z"


def _duration(value):
    """A `duration` as HGL writes one: `1h30m`, `500ms`, `0s`."""
    micros = (value.days * 86_400 + value.seconds) * 1_000_000 + value.microseconds
    if micros == 0:
        return "0s"
    sign, micros = ("-", -micros) if micros < 0 else ("", micros)
    parts = []
    for unit, size in (
        ("d", 86_400_000_000),
        ("h", 3_600_000_000),
        ("m", 60_000_000),
        ("s", 1_000_000),
        ("ms", 1_000),
        ("us", 1),
    ):
        count, micros = divmod(micros, size)
        if count:
            parts.append(f"{count}{unit}")
    return sign + "".join(parts)


def _shown(value, core):
    """A published value as text, written as HGL writes one where it has
    a way to. A scalar that converts to `str` is its conversion, the
    provider `core`'s (OPS-6)."""
    if value is None or type(value).__name__ == "_Null":
        return "null"
    if isinstance(value, bool):
        return core.convert_bool_to_str(value)
    if isinstance(value, int):
        return core.convert_i64_to_str(value)
    if isinstance(value, float):
        return core.convert_f64_to_str(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, datetime.datetime):
        return _instant(value)
    if isinstance(value, datetime.timedelta):
        return _duration(value)
    if isinstance(value, (datetime.date, datetime.time)):
        return value.isoformat()
    if isinstance(value, enum.Enum):
        return f".{value.name}"
    kind = type(value)
    fields = getattr(kind, "_hgl_fields", None)
    if fields is not None:
        inside = ", ".join(f"{field}: {_shown(getattr(value, field), core)}" for field in fields)
        return f"{kind.__name__}({inside})"
    if kind.__name__ == "Ok" and hasattr(value, "value"):
        return f"ok({_shown(value.value, core)})"
    if kind.__name__ == "Err" and hasattr(value, "error"):
        return f"err({_shown(value.error, core)})"
    if core._is_set_delta(value):
        # A set's delta: what the publication added and what it removed.
        added, removed = _shown(set(value.added), core), _shown(set(value.removed), core)
        return f"delta(added: {added}, removed: {removed})"
    if isinstance(value, dict) and hasattr(value, "size_modified"):
        # A list's delta (TYPE-16): the positions updated, those that lost
        # their value, and the new size where it moved.
        updated, removed = _shown(core.updated(value), core), _shown(set(core.removed(value)), core)
        size = _shown(value.size_modified, core)
        return f"delta(updated: {updated}, removed: {removed}, size_modified: {size})"
    if isinstance(value, tuple):
        return "(" + ", ".join(_shown(part, core) for part in value) + ")"
    if isinstance(value, list):
        return "[" + ", ".join(_shown(part, core) for part in value) + "]"
    if isinstance(value, (set, frozenset)):
        return "{" + ", ".join(sorted(_shown(part, core) for part in value)) + "}"
    if isinstance(value, dict):
        shown = (f"{_shown(key, core)}: {_shown(held, core)}" for key, held in value.items())
        return "{" + ", ".join(shown) + "}"
    return str(value)


def main(directory, module_name, function_name, results, mode):
    _driver.find_program(directory)
    # Standard output is made standard error, for Python and for what
    # is under it: hgraph logs to the first.
    sys.stdout.flush()
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    with open(results, "w", encoding="utf-8") as out:
        say = _driver.recorder(out)
        say("start")
        try:
            module = importlib.import_module(module_name)
            function = getattr(module, function_name)
            harness = importlib.import_module("_hgl._harness")
            # The provider, which the harness imports: its conversions
            # write what is published.
            core = importlib.import_module("_hgl._core_native")
        except Exception as error:
            say("unloaded", _driver.escape(_described(error)))
            return 1

        outcome = {}

        def work():
            try:
                outcome["output"] = function()
            except BaseException as error:
                outcome["error"] = error

        asked = []

        def stop(again):
            # The host asks the run to stop. A run in real time ends as
            # it does at its end: the request is kept for a run that has
            # not started yet. A simulation has nothing to ask, and
            # `again` is a second interrupt: the interpreter ends as it
            # is.
            if mode != "real-time" or (again and asked):
                say("failed", "interrupted")
                os._exit(130)
            if not asked:
                say("host")
            asked.append(True)
            harness.stop()

        def interrupted(signum, frame):
            stop(True)

        def listen():
            # A request passed down standard input, and its end.
            try:
                for line in sys.stdin:
                    if line.strip() == "stop":
                        stop(False)
            except (OSError, ValueError):
                pass
            if "output" not in outcome and "error" not in outcome:
                stop(False)

        signal.signal(signal.SIGINT, interrupted)
        if hasattr(signal, "SIGTERM"):
            signal.signal(signal.SIGTERM, interrupted)
        thread = threading.Thread(target=work, name="hgl-run", daemon=True)
        thread.start()
        threading.Thread(target=listen, name="hgl-stop", daemon=True).start()
        # A wait with an end, so that a signal's handler is run.
        while thread.is_alive():
            thread.join(0.1)

        if "error" in outcome:
            say("failed", _driver.escape(_described(outcome["error"])))
            return 1
        output = outcome.get("output")
        if output is None:
            say("stopped", "none")
            return 0
        for at, published in output:
            say("published", _instant(at), _driver.escape(_shown(published, core)))
        say("stopped")
        return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:6]))
