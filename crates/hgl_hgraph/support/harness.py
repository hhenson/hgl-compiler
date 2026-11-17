"""The eval harness of a generated test, for the hgraph target.

Written by the hgl compiler beside the modules it generates.
"""

import datetime as _datetime

from hgraph.test import eval_node

# Where an eval's run starts, and how far apart two of its cycles are
# (core's CRUN-3): "The earliest start is one smallest step after
# `1970-01-01T00:00:00Z`", and the smallest step is a microsecond. They
# are hgraph's own `MIN_ST` and `MIN_TD`, which `eval_node` runs from and
# by. An instant is a `datetime` in UTC that names no zone, as hgraph has
# them.
_SMALLEST_STEP = _datetime.timedelta(microseconds=1)
_EARLIEST_START = _datetime.datetime(1970, 1, 1) + _SMALLEST_STEP


def evaluate(target, /, **arguments):
    """Runs `target` with a dense trace for each temporal input.

    `target` is given by position alone, so that an input of the target
    may itself be named `target`.

    A trace is a list with one cell for each engine step from the run's
    start, and `None` where nothing is published. The result is the dense
    trace of what the target published: it spans the run's start through
    the last publication, and is empty when there was none (the language's
    TST-6).
    """
    result = eval_node(target, **arguments)
    if result is None:
        return []
    result = list(result)
    while result and result[-1] is None:
        result.pop()
    return result


def publications(trace):
    """What a `const fn` is given for a dense trace (the language's TST-16).

    A list with a tuple for each publication, in time order: its instant,
    and what was published then. Position i of a dense trace is the run's
    start plus i smallest steps (TST-6), and a position with no
    publication gives no tuple.
    """
    return [
        (_EARLIEST_START + position * _SMALLEST_STEP, published)
        for position, published in enumerate(trace)
        if published is not None
    ]


def same(left, right):
    """Whether two dense traces are the same (the language's TST-4).

    They are when each cell is: both silent, or both the same
    publication. A `None` at the end of either adds nothing: "Trailing
    placeholders have no effect" (TST-6).
    """
    left, right = _trimmed(left), _trimmed(right)
    return len(left) == len(right) and all(map(_same_cell, left, right))


def _trimmed(trace):
    trace = list(trace)
    while trace and trace[-1] is None:
        trace.pop()
    return trace


def _same_cell(left, right):
    if left is None or right is None:
        return left is None and right is None
    if _is_set_delta(left) or _is_set_delta(right):
        # hgraph's own equality takes `Removed(1)` for `1`, so a delta
        # that removed an element would equal one that added it. What
        # each added and what each removed are compared instead.
        return (
            _is_set_delta(left)
            and _is_set_delta(right)
            and frozenset(left.added) == frozenset(right.added)
            and frozenset(left.removed) == frozenset(right.removed)
        )
    return left == right


def _is_set_delta(value):
    return hasattr(value, "added") and hasattr(value, "removed")
