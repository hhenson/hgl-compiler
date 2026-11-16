"""Runs named tests of a program written by the hgl compiler for hgraph.

Usage: _run_tests.py OUTPUT_DIRECTORY PLAN RESULTS

PLAN names one test on each line: its Python module, a space, and its
function. RESULTS is written as the tests run, one line for each thing
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
"""

import importlib
import os
import sys
import traceback


def _escape(text):
    return text.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")


def _described(error):
    """An error as a person reads it."""
    if isinstance(error, AssertionError) and not error.args:
        return "the assertion does not hold"
    return f"{type(error).__name__}: {error}"


def _run(module_name, function_name, say, number):
    try:
        module = importlib.import_module(module_name)
        function = getattr(module, function_name)
    except Exception as error:
        say("unloaded", number, _escape(_described(error)))
        return
    try:
        function()
    except Exception as error:
        # The line of the test's own function that was running: the
        # failing statement of the test, whatever it called.
        line = 0
        for frame, frame_line in traceback.walk_tb(error.__traceback__):
            if frame.f_code is function.__code__:
                line = frame_line
        say("failed", number, line, _escape(_described(error)))
    else:
        say("passed", number)


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

        for number, (module_name, function_name) in enumerate(tests):
            say("start", number)
            _run(module_name, function_name, say, number)


if __name__ == "__main__":
    main(*sys.argv[1:4])
