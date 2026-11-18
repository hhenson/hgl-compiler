"""Runs named tests of a program written by the hgl compiler for hgraph.

Usage: _run_tests.py OUTPUT_DIRECTORY PLAN RESULTS

PLAN names one test on each line: its Python module, its function, and
its time limit in seconds, or `-` for a test with none, with a space
between. RESULTS is written as the tests run, one line for each thing
that happens to a test, which is named by its position in the plan
from 0:

    start N            the test is about to be loaded and run
    passed N           it ran to its end
    failed N LINE TEXT it raised: LINE is the line of the test's own
                       function that was running, 0 when unknown
    unloaded N TEXT    its module or its function could not be loaded

TEXT has `\\`, `\n` and `\r` for a backslash, a line feed and a
carriage return. A test with a `start` and nothing after it was running
when the interpreter stopped.

A test with a time limit is run on a thread of its own, and this one
waits for it no longer than the limit. One still running then is asked
to stop: a run in real time ends as it does at its end, and the test
goes on to its own end. It has failed whatever it then does. One that
does not end soon after is left, and the interpreter ends: the tests
after it are not run.
"""

import importlib
import os
import sys
import threading
import traceback

# How long a test that has passed its limit is given to end, once its
# run was asked to stop.
_GRACE = 5.0


def _escape(text):
    return text.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")


def _described(error):
    """An error as a person reads it."""
    if isinstance(error, AssertionError) and not error.args:
        return "the assertion does not hold"
    return f"{type(error).__name__}: {error}"


def _seconds(limit):
    """A limit as a person reads it: `1m`, `90s`, `0.5s`."""
    if limit >= 60 and limit % 60 == 0:
        return f"{int(limit // 60)}m"
    return f"{limit:g}s"


def _called(function):
    """Calls a test, and gives `None` where it ran to its end, or the
    line of it that was running and what was raised."""
    try:
        function()
    except Exception as error:
        # The line of the test's own function that was running: the
        # failing statement of the test, whatever it called.
        line = 0
        for frame, frame_line in traceback.walk_tb(error.__traceback__):
            if frame.f_code is function.__code__:
                line = frame_line
        return line, _described(error)
    return None


def _bounded(function, limit, give_up):
    """Calls a test on a thread of its own, and waits for it no longer
    than `limit` seconds. Gives what `_called` does for a test that ends
    in time, and for one that does not, the line of it that was running
    and that it timed out. `give_up` is called with that, for a test
    that does not end even once its run was asked to stop: it does not
    return.
    """
    outcome = []
    thread = threading.Thread(
        target=lambda: outcome.append(_called(function)), name="hgl-test", daemon=True
    )
    thread.start()
    thread.join(limit)
    if not thread.is_alive():
        return outcome[0]
    # The statement of the test that is running now.
    line = 0
    frame = sys._current_frames().get(thread.ident)
    while frame is not None:
        if frame.f_code is function.__code__:
            line = frame.f_lineno
        frame = frame.f_back
    timed_out = line, f"timed out: it was still running after its limit of {_seconds(limit)}"
    # A run in real time is asked to stop, as its host: it then ends as
    # it does at its end, and the test goes on to its own end.
    harness = sys.modules.get("_hgl._harness")
    if harness is not None:
        harness.stop()
    thread.join(_GRACE)
    if thread.is_alive():
        give_up(timed_out)
    if harness is not None:
        # A stop that met no run is not kept for the next test's.
        harness.stop_withdrawn()
    return timed_out


def _run(module_name, function_name, limit, say, number):
    try:
        module = importlib.import_module(module_name)
        function = getattr(module, function_name)
    except Exception as error:
        say("unloaded", number, _escape(_described(error)))
        return

    def give_up(failure):
        # The test's thread cannot be ended, and what it holds may be
        # another test's too: the interpreter ends here.
        say("failed", number, failure[0], _escape(failure[1]))
        os._exit(3)

    failure = _called(function) if limit is None else _bounded(function, limit, give_up)
    if failure is None:
        say("passed", number)
    else:
        say("failed", number, failure[0], _escape(failure[1]))


def main(directory, plan, results):
    # Python puts this script's directory first on its path. The
    # program is found from the output directory and nowhere else.
    own = os.path.dirname(os.path.abspath(__file__))
    sys.path[:] = [entry for entry in sys.path if os.path.abspath(entry or ".") != own]
    sys.path.insert(0, directory)
    with open(plan, encoding="utf-8") as file:
        tests = [line.split(" ") for line in file.read().splitlines() if line]
    with open(results, "w", encoding="utf-8") as out:

        def say(*parts):
            out.write(" ".join(str(part) for part in parts) + "\n")
            out.flush()

        for number, (module_name, function_name, limit) in enumerate(tests):
            say("start", number)
            _run(module_name, function_name, None if limit == "-" else float(limit), say, number)


if __name__ == "__main__":
    main(*sys.argv[1:4])
