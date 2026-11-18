"""The run of an entry, for the hgraph target: a generated test's eval,
and an application's run statement.

Written by the hgl compiler beside the modules it generates.
"""

import datetime as _datetime
import logging as _logging
import threading as _threading

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


class _Recording:
    """The trace an eval gives: its run's publications, each with its
    instant, and the instant the run started at.

    A trace is kept as what was published and when, and never as a cell
    for each smallest step: a publication a second after the start
    would be a million cells on. Where two traces are compared each is
    read as its publications and how long after its run's start each
    is (`_relative`).
    """

    def __init__(self, start, output):
        self.start = start
        self.output = output


class _Timed:
    """A trace written out timed, `[0s: 7, 2s: 9]` (the language's
    TST-7): its publications in the order written, each with its key, a
    duration from the run's start or an instant."""

    def __init__(self, entries):
        self.entries = entries


def timed(entries):
    """A trace written out timed: `entries` is each publication with its
    key, `(key, publication)`."""
    return _Timed(entries)


def _relative(trace, start=None):
    """A trace as its publications, each with how long after its run's
    start it was published, in time order.

    A trace written out dense, `[7, _, 9]`, is a list with a cell for
    each smallest step: cell i is i steps after the start, and a `None`
    is no publication (the language's TST-6). An eval's trace has each
    publication's instant, which is cast to how long after the run's
    start it is. A timed trace's duration key is that already, and its
    instant is cast by `start`, the start of the run it is read
    against; with none it is left an instant, which another instant is
    compared with.
    """
    if isinstance(trace, _Recording):
        return [(at - trace.start, published) for at, published in trace.output]
    if isinstance(trace, _Timed):
        return [
            (at - start if start is not None and isinstance(at, _datetime.datetime) else at, published)
            for at, published in trace.entries
        ]
    return [
        (position * _SMALLEST_STEP, cell) for position, cell in enumerate(trace) if cell is not None
    ]


def _bounds(config, unstated):
    """The start a run has, and the end its configuration gives it,
    `None` where it gives none (core's CRUN-2). `unstated` is the start
    of a run whose configuration gives none.

    "`length` gives the end counted from the start the run has, given or
    not ... A `RunConfig` with both an `end` and a `length` is refused
    before the run's graph is built, an HGL error with code
    `run.end_and_length`."
    """
    if config is None:
        return unstated, None
    start, end, length = config.start, config.end, config.length
    if end is not None and length is not None:
        raise _core.HglError(
            "a run's configuration gives both an `end` and a `length`",
            code="run.end_and_length",
        )
    start = unstated if start is None else start
    if length is not None:
        end = _core.add__datetime_duration(start, length)
    return start, end


def _present():
    """The host clock's present time, as hgraph has an instant: in UTC,
    naming no zone."""
    return _datetime.datetime.now(_datetime.timezone.utc).replace(tzinfo=None)


# What asks the run in real time that is under way to stop: the sender
# of its stop source, once the source has started. `_stop_wanted` is set
# by a request that came before then, which the source then sends
# itself.
_senders = []
_stop_wanted = _threading.Event()


@_hg.push_queue(_hg.TS[bool])
def _stop_source(sender):
    """A source the host's stop is sent through. hgraph wakes a run in
    real time for what is sent to one, from any thread."""
    _senders.append(sender)
    if _stop_wanted.is_set():
        sender(True)


def stop():
    """Asks the run in real time that is under way to stop, as its host
    (the language's RUN-3: "until the configuration's end or until its
    host stops it"). The run then ends as any does: every node that
    started is stopped (core's CRUN-6), and its output is its recording
    up to then.

    It may be called from any thread, and from a signal's handler. False
    where no run in real time is under way.
    """
    _stop_wanted.set()
    for sender in list(_senders):
        sender(True)
    return bool(_senders)


def stop_withdrawn():
    """Withdraws a stop that was asked for and met no run: the next run
    in real time is then not stopped by it."""
    _stop_wanted.clear()


def _ran(target, replays, record, replayed, arguments, start, end, real_time):
    """Runs the entry that wires `target`, from `start` and before
    `end`, and gives its output: the run's recording, or `None` where
    `record` is `None`.

    Core's CRUN-1: "It builds the entry's graph, starts it, evaluates
    zero or more cycles, and stops it, once each." Core's CEVAL-1: "Eval
    is a run of an entry it generates: core's `replay` wired to each
    temporal input of the target, and the target's result as the
    entry's." The entry is wired here, and run by hgraph: as a
    simulation of its, or in real time (core's CRUN-4, CRUN-5). Core's
    CRUN-2: "`start` is inclusive and `end` exclusive", as hgraph's own
    are.
    """

    @_hg.graph
    def entry():
        given = {
            name: (replays[name](items=replayed[name]) if name in replayed else value)
            for name, value in arguments.items()
        }
        published = target(**given)
        if record is not None:
            # CRUN-8: "The run wires `record` to the entry's result under
            # the key `"run.output"`".
            record(input=published, key=_RUN_OUTPUT)
        if real_time:
            # The host's stop: hgraph ends the run when the source
            # publishes.
            _hg.stop_engine(_stop_source())

    # CRUN-7: "A run has one store, which is empty when its graph is
    # built."
    with _hg.GlobalState() as store:
        try:
            _hg.evaluate_graph(
                entry,
                _hg.GraphConfiguration(
                    run_mode=_hg.EvaluationMode.REAL_TIME
                    if real_time
                    else _hg.EvaluationMode.SIMULATION,
                    start_time=start,
                    end_time=end,
                    # hgraph writes its own account of a run at the level
                    # below this one.
                    default_log_level=_logging.INFO,
                ),
            )
        finally:
            _senders.clear()
            _stop_wanted.clear()
        if record is None:
            return None
        # CEVAL-6: "A failed run or a missing recording makes eval fail."
        if not _core.has_GlobalState_str(store, _RUN_OUTPUT):
            raise _core.HglError("the run stopped with no recording")
        return _core.get_GlobalState_str(store, _RUN_OUTPUT)


def run(target, _record, _real_time, /, _config=None, **arguments):
    """Runs an application's entry `target`, and gives the run's output.

    The language's RUN-1: "The entry's result, if any, is the run's
    output: for a result type T a `list<tuple<datetime, delta<T>>>`, one
    tuple for each publication, in time order". It is `None` for an
    entry with no result. `_record` is core's `record` for the result's
    type, as the compiler wrote it, `None` where the entry has none.
    They are given by position alone, so that a parameter of the entry
    may have any name, and the run's configuration as `_config`.

    RUN-3: "The word of a run statement is its mode": `_real_time` is
    true for `run` and false for `eval`. Core's CRUN-2: "A null `start`
    is the earliest start in simulation and the host clock's present
    time in real time."
    """
    start, end = _bounds(_config, _present() if _real_time else _EARLIEST_START)
    if end is not None and end <= start:
        # No cycle runs. hgraph has no run that ends where it starts, so
        # none is made: the graph is not built, started or stopped.
        return None if _record is None else []
    return _ran(target, {}, _record, {}, arguments, start, end, _real_time)


def evaluate(target, _replays, _record, /, _config=None, _real_time=False, **arguments):
    """Runs `target` with a dense trace for each temporal input.

    `_replays` has core's `replay` for each temporal input the eval
    gives a trace, by the input's name, and `_record` core's `record`
    for the target's result, `None` where it has none: each as the
    compiler wrote it for the type it is of. They and `target` are
    given by position alone, so that an input of the target may have
    any name, and the run's configuration as `_config`, a name no input
    has.

    A trace given is a list with one cell for each smallest step from the
    run's start, and `None` where nothing is published: each publication
    of it is replayed at the run's start plus its position. The result
    is the trace of what the target published (`_Recording`).

    The run is a simulation under the configuration (`_ran`). With
    `_real_time`, a name no input has, it is `run(...)` in an
    integration test (the language's TST-17): a run in real time, from
    the host clock's present time where the configuration gives no
    start.
    """
    start, end = _bounds(_config, _present() if _real_time else _EARLIEST_START)
    if end is not None and end <= start:
        # No cycle runs. hgraph has no run that ends where it starts, so
        # none is made: the graph is not built, started or stopped.
        return _Recording(start, [])
    # CEVAL-2: "Each tuple holds the absolute evaluation time and the
    # publication's delta ... dense position i is the start plus i
    # smallest steps. A dense placeholder locates any later entries and
    # adds no replay entry or run bound."
    replayed = {
        name: [
            (start + after, published) for after, published in _relative(arguments[name], start)
        ]
        for name in _replays
        if name in arguments
    }
    output = _ran(target, _replays, _record, replayed, arguments, start, end, _real_time)
    return _Recording(start, [] if output is None else output)


def publications(trace):
    """What a `const fn` is given for a trace (the language's TST-16).

    A list with a tuple for each publication, in time order: its instant,
    and what was published then. An eval's trace has them as they were
    recorded. Position i of a trace written out dense is the earliest
    start plus i smallest steps (TST-6), and a position with no
    publication gives no tuple.
    """
    if isinstance(trace, _Recording):
        return list(trace.output)
    # A trace written out: an offset is from the earliest start, and an
    # instant is itself.
    return [
        (_EARLIEST_START + after, published)
        for after, published in _relative(trace, _EARLIEST_START)
    ]


def same(left, right):
    """Whether two traces are the same (the language's TST-4).

    They are when they have the same publications, each as long after
    its run's start as the other's. A `_` at the end of a trace written
    out is no publication, and adds nothing: "Trailing placeholders have
    no effect" (TST-6).
    """
    # A timed trace's instants are read against the run the other trace
    # is of, where it is one.
    start = next((trace.start for trace in (left, right) if isinstance(trace, _Recording)), None)
    left, right = _relative(left, start), _relative(right, start)
    return len(left) == len(right) and all(
        ours[0] == theirs[0] and _same_cell(ours[1], theirs[1])
        for ours, theirs in zip(left, right)
    )


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
