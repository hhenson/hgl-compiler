"""The eval harness of a generated test, for the hgraph target.

Written by the hgl compiler beside the modules it generates.
"""

import datetime as _datetime

import _hgl._core_native as _core
from hgraph.test import eval_node

# Where an eval's run starts, and how far apart two of its cycles are
# (core's CRUN-3): "The earliest start is one smallest step after
# `1970-01-01T00:00:00Z`", and the smallest step is a microsecond. They
# are hgraph's own `MIN_ST` and `MIN_TD`, which `eval_node` runs from and
# by. An instant is a `datetime` in UTC that names no zone, as hgraph has
# them.
_SMALLEST_STEP = _datetime.timedelta(microseconds=1)
_EARLIEST_START = _datetime.datetime(1970, 1, 1) + _SMALLEST_STEP


class _Trace(list):
    """The dense trace an eval gives, with the instant its run started at.

    Position i of it is the run's start plus i smallest steps (the
    language's TST-6), so what reads its instants needs the start.
    """

    start = _EARLIEST_START


def _bounds(config):
    """The start and the end a run's configuration gives it, each `None`
    where it gives none (core's CRUN-2).

    "`length` gives the end counted from the start the run has, given or
    not ... A `RunConfig` with both an `end` and a `length` is refused
    before the run's graph is built, an HGL error with code
    `run.end_and_length`."
    """
    if config is None:
        return None, None
    start, end, length = config.start, config.end, config.length
    if end is not None and length is not None:
        raise _core.HglError(
            "a run's configuration gives both an `end` and a `length`",
            code="run.end_and_length",
        )
    if length is not None:
        end = _core.add__datetime_duration(_EARLIEST_START if start is None else start, length)
    return start, end


def evaluate(target, /, _config=None, **arguments):
    """Runs `target` with a dense trace for each temporal input.

    `target` is given by position alone, so that an input of the target
    may itself be named `target`, and the run's configuration as
    `_config`, a name no input has.

    A trace is a list with one cell for each engine step from the run's
    start, and `None` where nothing is published. The result is the dense
    trace of what the target published: it spans the run's start through
    the last publication, and is empty when there was none (the language's
    TST-6).

    Core's CRUN-2: "`start` is inclusive and `end` exclusive: no cycle
    runs before `start`, and none at or after `end`." hgraph's own end is
    exclusive too, and its start is where the first cell of each trace
    falls.
    """
    start, end = _bounds(_config)
    result = _Trace()
    result.start = _EARLIEST_START if start is None else start
    if end is not None and end <= result.start:
        # No cycle runs. hgraph has no run that ends where it starts, so
        # none is made: the graph is not built, started or stopped.
        return result
    bounds = {}
    if start is not None:
        bounds["__start_time__"] = start
    if end is not None:
        bounds["__end_time__"] = end
    try:
        ran = eval_node(target, **bounds, **arguments)
    except RuntimeError as error:
        if "too many cycles to" not in str(error):
            raise
        # hgraph 0.9.0 keeps a dense recording from the earliest start,
        # whatever start the run has, and holds no more than about a
        # second of cycles of it.
        raise RuntimeError(
            "the hgraph target cannot give the dense trace of a run that starts this far "
            "after the earliest start"
        ) from error
    if ran is None:
        return result
    result.extend(ran)
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
    start = getattr(trace, "start", _EARLIEST_START)
    return [
        (start + position * _SMALLEST_STEP, published)
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


# The name an error is marked with once an `assert raises` has refused it.
# The assertion failed, and the error is let out as it is, so that the test
# reports it where it was raised. An enclosing `assert raises` must not then
# take it for the failure it expects, whatever its code: the language's
# TST-11, "A failed nested assertion cannot satisfy an enclosing raises".
_REFUSED = "_hgl_refused_by_assert_raises"


def _escaping_code(error):
    """The execution code of what escaped, or `None` where it has none.

    A failure of a node comes out of hgraph as an error of hgraph's own,
    with the HGL error it met as its cause. The cause is followed to the
    HGL error, whose code is the one asked about.

    The language's TST-11: "A failed nested assertion cannot satisfy an
    enclosing raises", and no more can any other failure that is no HGL
    error with a code.
    """
    seen = set()
    while error is not None and id(error) not in seen:
        seen.add(id(error))
        if isinstance(error, AssertionError) or getattr(error, _REFUSED, False):
            return None
        if isinstance(error, _core.HglError):
            return error.code
        error = error.__cause__ or error.__context__
    return None


def raised(error, code):
    """What `assert raises("code")` does with an error its block let out.

    The language's TST-10: "A matching escaping error passes ...; then
    continue after the assertion. ... a different code ... fails the
    test." An error that does not match is let out again as it is, so
    that the test fails where it was raised and with what it says.
    """
    if _escaping_code(error) != code:
        try:
            setattr(error, _REFUSED, True)
        except AttributeError:
            # An error that takes no attribute is carried by one that
            # does, and that is an assertion's failure.
            raise AssertionError(
                f"the block raised {type(error).__name__}: {error}, and `{code}` was expected"
            ) from error
        raise error


def not_raised(code):
    """What `assert raises("code")` does when its block completes.

    The language's TST-10: "Normal completion ... fails the test."
    """
    raise AssertionError(f"the block raised nothing, and `{code}` was expected")
