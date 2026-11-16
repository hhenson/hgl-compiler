"""Runs every `test_*` function of generated modules, for the target's tests.

Usage: run_tests.py OUTPUT_DIRECTORY MODULE...

Prints `ok` or `FAILED` for each test, and exits with 1 when one failed.
"""

import importlib
import sys
import traceback

sys.path.insert(0, sys.argv[1])
failed = 0
for module_name in sys.argv[2:]:
    module = importlib.import_module(module_name)
    for name in sorted(vars(module)):
        if name.startswith("test_"):
            try:
                getattr(module, name)()
                print("ok", module_name, name)
            except Exception:
                failed += 1
                print("FAILED", module_name, name)
                traceback.print_exc()
sys.exit(1 if failed else 0)
