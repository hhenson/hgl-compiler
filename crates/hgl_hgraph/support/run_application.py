"""Runs an application of a program written by the hgl compiler for hgraph.

Usage: _run_application.py OUTPUT_DIRECTORY MODULE FUNCTION RESULTS

MODULE is the application's Python module and FUNCTION the function its
run statement is written as. RESULTS is written as the run goes, one
line for each thing that happens:

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
Python. An interrupt asks a run in real time to stop, which ends it as
its end does. A simulation waits for nothing, and a second interrupt
asks twice: either ends the interpreter at once.
"""

import datetime
import enum
import importlib
import json
import math
import os
import signal
import sys
import threading


def _escape(text):
    return text.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")


def _described(error):
    """An error as a person reads it: the HGL error under what hgraph
    raised, where there is one."""
    seen = set()
    found = error
    while found is not None and id(found) not in seen:
        seen.add(id(found))
        if getattr(found, "code", None) is not None and type(found).__name__ == "HglError":
            return f"{found.code}: {found}"
        found = found.__cause__ or found.__context__
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


def _shown(value):
    """A published value as text, written as HGL writes one where it has
    a way to."""
    if value is None or type(value).__name__ == "_Null":
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if math.isnan(value):
            return "nan"
        if math.isinf(value):
            return "inf" if value > 0 else "-inf"
        return repr(value)
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
        inside = ", ".join(f"{field}: {_shown(getattr(value, field))}" for field in fields)
        return f"{kind.__name__}({inside})"
    if kind.__name__ == "Ok" and hasattr(value, "value"):
        return f"ok({_shown(value.value)})"
    if kind.__name__ == "Err" and hasattr(value, "error"):
        return f"err({_shown(value.error)})"
    if hasattr(value, "added") and hasattr(value, "removed"):
        # A set's delta: what the publication added and what it removed.
        return f"delta(added: {_shown(set(value.added))}, removed: {_shown(set(value.removed))})"
    if isinstance(value, tuple):
        return "(" + ", ".join(_shown(part) for part in value) + ")"
    if isinstance(value, list):
        return "[" + ", ".join(_shown(part) for part in value) + "]"
    if isinstance(value, (set, frozenset)):
        return "{" + ", ".join(sorted(_shown(part) for part in value)) + "}"
    if isinstance(value, dict):
        return "{" + ", ".join(f"{_shown(key)}: {_shown(held)}" for key, held in value.items()) + "}"
    return str(value)


def main(directory, module_name, function_name, results):
    # Python puts this script's directory first on its path. The
    # program is found from the output directory and nowhere else.
    own = os.path.dirname(os.path.abspath(__file__))
    sys.path[:] = [entry for entry in sys.path if os.path.abspath(entry or ".") != own]
    sys.path.insert(0, directory)
    # Standard output is made standard error, for Python and for what
    # is under it: hgraph logs to the first.
    sys.stdout.flush()
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    with open(results, "w", encoding="utf-8") as out:
        lock = threading.Lock()

        def say(*parts):
            with lock:
                out.write(" ".join(str(part) for part in parts) + "\n")
                out.flush()

        say("start")
        try:
            module = importlib.import_module(module_name)
            function = getattr(module, function_name)
            harness = importlib.import_module("_hgl._harness")
        except Exception as error:
            say("unloaded", _escape(_described(error)))
            return 1

        outcome = {}

        def work():
            try:
                outcome["output"] = function()
            except BaseException as error:
                outcome["error"] = error

        asked = []

        def interrupted(signum, frame):
            # The first asks a run in real time to stop. A simulation
            # has nothing to ask, and a second interrupt has asked
            # already: the interpreter ends as it is.
            if asked or not harness.stop():
                say("failed", "interrupted")
                os._exit(130)
            asked.append(signum)
            say("host")

        signal.signal(signal.SIGINT, interrupted)
        if hasattr(signal, "SIGTERM"):
            signal.signal(signal.SIGTERM, interrupted)
        thread = threading.Thread(target=work, name="hgl-run", daemon=True)
        thread.start()
        # A wait with an end, so that a signal's handler is run.
        while thread.is_alive():
            thread.join(0.1)

        if "error" in outcome:
            say("failed", _escape(_described(outcome["error"])))
            return 1
        output = outcome.get("output")
        if output is None:
            say("stopped", "none")
            return 0
        for at, published in output:
            say("published", _instant(at), _escape(_shown(published)))
        say("stopped")
        return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:5]))
