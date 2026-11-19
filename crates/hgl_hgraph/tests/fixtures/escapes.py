"""ERR-1, ERR-7: a failure that is no HGL error escapes every capture.

Run with the directory that holds the provider, `_core_native.py`.
"""

import logging
import sys

sys.path.insert(0, sys.argv[1])
# hgraph says on the standard output as it starts and ends each run.
logging.disable(logging.CRITICAL)

import _core_native as _core
from hgraph import TS, compute_node, graph, try_except
from hgraph.test import eval_node


@compute_node
def failing(value: TS[int]) -> TS[int]:
    if value.value == 0:
        raise NotImplementedError("a form the target does not write")
    if value.value < 0:
        raise _core.HglError("negative", code="demo.negative")
    return value.value


@graph
def captured(value: TS[int]) -> TS[str]:
    @graph
    def child(value: TS[int]) -> TS[int]:
        return failing(value)

    boundary = _core.captured(try_except(child, value=value))
    return _messages(_core.captured_error(boundary.exception))


@graph
def nested(value: TS[int]) -> TS[str]:
    @graph
    def inner(value: TS[int]) -> TS[str]:
        return captured(value)

    boundary = _core.captured(try_except(inner, value=value))
    return boundary.out


@compute_node
def _messages(failure: TS[_core.Error]) -> TS[str]:
    return f"{failure.value.message} {failure.value.code}"


def outcome(entry, values):
    try:
        return repr(eval_node(entry, value=values))
    except Exception as error:
        seen = set()
        while error is not None and id(error) not in seen:
            seen.add(id(error))
            if isinstance(error, _core.Escaped):
                return "escaped"
            error = error.__cause__ or error.__context__
        return "failed otherwise"


# An HGL error is captured; any other failure fails the run, through a
# capture and through a capture around that one.
print(outcome(captured, [1, -1, 2]))
print(outcome(captured, [1, 0, 2]))
print(outcome(nested, [1, 0, 2]))
