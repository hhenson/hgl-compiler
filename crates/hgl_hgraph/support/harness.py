"""The eval harness of a generated test, for the hgraph target.

Written by the hgl compiler beside the modules it generates.
"""

import datetime as _datetime
import logging as _logging

import hgraph as _hg

import _hgl._core_native as _core

# Where an eval's run starts, and how far apart two of its cycles are
# (core's CRUN-3): "The earliest start is one smallest step after
# `1970-01-01T00:00:00Z`", and the smallest step is a microsecond. They
# are hgraph's own `MIN_ST` and `MIN_TD`, which a simulation of hgraph's
# runs from and by. An instant is a `datetime` in UTC that names no zone,
# as hgraph has them.
_SMALLEST_STEP = _datetime.timedelta(microseconds=1)
_EARLIEST_START = _datetime.datetime(1970, 1, 1) + _SMALLEST_STEP

# The key a run's recording is put under (core's CRUN-8): "The run wires
# `record` to the entry's result under the key `"run.output"`, and its
# output is the value under that key once the run has stopped."
_RUN_OUTPUT = "run.output"

# The longest dense trace an eval gives. A dense trace has a cell for
# each smallest step from the run's start to its last publication, so a
# publication a second after the start is a million cells on.
_DENSE_LIMIT = 1_000_000


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


# The replay and record nodes made so far, each by the hgraph type of what
# it publishes or takes.
_REPLAYS = {}
_RECORDS = {}


def _replay(published):
    """Core's `replay` for an input hgraph has as `published` (CEVAL-2,
    CEVAL-3): a source of the publications a replay list holds.

    Core has `replay` as HGL, a source that yields each entry at its
    time. Until that is source in core this is the same source written
    here: a node that the provider resumes over the entries, as it does
    a source's own statements.
    """
    key = repr(published)
    if key not in _REPLAYS:

        @_hg.compute_node
        def replay(
            _begin: _hg.TS[bool],
            items: object,
            _state: _hg.STATE = None,
            _clock: _hg.EvaluationClock = None,
            _scheduler: _hg.SCHEDULER = None,
        ) -> published:
            due = _core.resume(_state.source, _clock.evaluation_time, _scheduler)
            if due is not None:
                return due[0]

        @replay.start
        def start(items: object, _state: _hg.STATE = None):
            _state.source = _core.Source(iter(items.value))

        _REPLAYS[key] = replay
    return _REPLAYS[key]


def _record(taken):
    """Core's `record` for a result hgraph has as `taken` (CEVAL-4): a
    sink that "starts with an empty recording. Each publication of its
    input adds a tuple of the evaluation time and an independently
    retained delta ... At stop it puts the recording in the store under
    its key, an empty one included." Written here until it is source in
    core, as `_replay` is.
    """
    key = repr(taken)
    if key not in _RECORDS:

        @_hg.sink_node
        def record(
            input: taken,
            key: str,
            _state: _hg.STATE = None,
            _clock: _hg.EvaluationClock = None,
        ):
            _state.entries.append((_clock.evaluation_time, input.delta_value))

        @record.start
        def start(_state: _hg.STATE = None):
            _state.entries = []

        @record.stop
        def stop(key: str, _state: _hg.STATE = None, _store: _hg.GlobalState = None):
            _core.put_GlobalState_str_V(_store, key, tuple(_state.entries))

        _RECORDS[key] = record
    return _RECORDS[key]


def evaluate(target, _inputs, _result, /, _config=None, **arguments):
    """Runs `target` with a dense trace for each temporal input.

    `_inputs` has the hgraph type of each temporal input of the target,
    by its name, and `_result` that of its result, `None` where it has
    none. They and `target` are given by position alone, so that an
    input of the target may have any name, and the run's configuration
    as `_config`, a name no input has.

    A trace is a list with one cell for each smallest step from the run's
    start, and `None` where nothing is published. The result is the dense
    trace of what the target published: it spans the run's start through
    the last publication, and is empty when there was none (the language's
    TST-6).

    Core's CEVAL-1: "Eval is a run of an entry it generates: core's
    `replay` wired to each temporal input of the target, and the
    target's result as the entry's." The entry is wired here, and run as
    a simulation of hgraph's under the configuration. Core's CRUN-2:
    "`start` is inclusive and `end` exclusive", as hgraph's own are.
    """
    start, end = _bounds(_config)
    result = _Trace()
    result.start = _EARLIEST_START if start is None else start
    if end is not None and end <= result.start:
        # No cycle runs. hgraph has no run that ends where it starts, so
        # none is made: the graph is not built, started or stopped.
        return result
    # CEVAL-2: "Each tuple holds the absolute evaluation time and the
    # publication's delta ... dense position i is the start plus i
    # smallest steps. A dense placeholder locates any later entries and
    # adds no replay entry or run bound."
    replayed = {
        name: tuple(
            (result.start + position * _SMALLEST_STEP, cell)
            for position, cell in enumerate(arguments[name])
            if cell is not None
        )
        for name in _inputs
        if name in arguments
    }

    @_hg.graph
    def entry():
        given = {
            name: (
                _replay(_inputs[name])(_hg.const(True), _core.Held(replayed[name]))
                if name in replayed
                else value
            )
            for name, value in arguments.items()
        }
        published = target(**given)
        if _result is not None:
            _record(_result)(published, _RUN_OUTPUT)

    # CRUN-7: "A run has one store, which is empty when its graph is
    # built."
    with _hg.GlobalState() as store:
        _hg.evaluate_graph(
            entry,
            _hg.GraphConfiguration(
                start_time=result.start,
                end_time=end,
                # hgraph writes its own account of a run at the level
                # below this one.
                default_log_level=_logging.INFO,
            ),
        )
        if _result is None:
            return result
        # CEVAL-6: "A failed run or a missing recording makes eval fail."
        if not _core.has_GlobalState_str(store, _RUN_OUTPUT):
            raise _core.HglError("the run stopped with no recording")
        output = _core.get_GlobalState_str(store, _RUN_OUTPUT)
    # TST-6: dense, from the run's start through the last publication.
    for at, published in output:
        position = (at - result.start) // _SMALLEST_STEP
        if position >= _DENSE_LIMIT:
            raise RuntimeError(
                f"the dense trace of this run would have {position + 1} cells: its last "
                "publication is that many smallest steps after its start"
            )
        result.extend([None] * (position + 1 - len(result)))
        result[position] = published
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
