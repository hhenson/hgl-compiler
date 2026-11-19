"""What the two drivers of a program written for hgraph share: the one
that runs its tests (`run_tests.py`) and the one that runs an
application (`run_application.py`).

It is written beside them, as `_hgl/_driver.py` in the output
directory. Python puts a driver's own directory first on its path, so
a driver imports this module from there, as `_driver`, before
`find_program` takes that directory off the path.
"""

import os
import sys
import threading


def escape(text):
    r"""`text` as one line of a results file: a backslash, a line feed
    and a carriage return as `\\`, `\n` and `\r`."""
    return text.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")


def find_program(directory):
    """Puts the output `directory` first on Python's path, and takes the
    drivers' own directory, this module's too, off it: Python put it
    first when it started the driver. The program is found from the
    output directory and nowhere else."""
    own = os.path.dirname(os.path.abspath(__file__))
    sys.path[:] = [entry for entry in sys.path if os.path.abspath(entry or ".") != own]
    sys.path.insert(0, directory)


def recorder(out):
    """A function, `say`, that writes a line to the results file `out`:
    its parts as text, with a space between. Each line is flushed as it
    is written, so that what was recorded is there however the
    interpreter ends, and a lock keeps the lines of two threads apart."""
    lock = threading.Lock()

    def say(*parts):
        with lock:
            out.write(" ".join(str(part) for part in parts) + "\n")
            out.flush()

    return say
