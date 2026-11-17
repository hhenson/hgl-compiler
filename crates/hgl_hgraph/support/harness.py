"""The eval harness of a generated test, for the hgraph target.

Written by the hgl compiler beside the modules it generates.
"""

from hgraph.test import eval_node


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
