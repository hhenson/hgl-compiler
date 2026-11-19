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

    `held` has, by its instant, the value the result held once each
    publication was made, where the result's delta is a type of its own
    (see `_hold`): a cell written whole is compared with it.
    """

    def __init__(self, start, output, held=None):
        self.start = start
        self.output = output
        self.held = {} if held is None else held


class _Whole:
    """A trace's cell written as a whole value, where the cell's type has
    a delta of its own (the language's TST-5: "a T, published whole, or a
    `delta<T>`"). An input publishes it whole, which "replaces what the
    input held" (TST-9), and an expectation's is "compared ... with the
    value the result holds once its cycle has published"."""

    __slots__ = ("value", "fixed")

    def __init__(self, value, fixed):
        self.value = value
        self.fixed = fixed


def whole(value, fixed=False):
    """A trace's cell written as the whole value `value` (TST-5); `fixed`
    where it is a fixed list's, whose delta is each position's."""
    # The cell keeps the value it is given: a copy, so that what the
    # program goes on changing does not change the trace (TYPE-5).
    return _Whole(_core.copied(value), fixed)


class _Published:
    """One publication of an eval's recording, as a trace is compared:
    the delta published, and the value the result held then, `None`
    where it was not read."""

    __slots__ = ("delta", "held")

    def __init__(self, delta, held):
        self.delta = delta
        self.held = held


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
        return [
            (at - trace.start, _Published(published, trace.held.get(at)))
            for at, published in trace.output
        ]
    if isinstance(trace, _Timed):
        entries = []
        for at, published in trace.entries:
            after = at - start if start is not None and isinstance(at, _datetime.datetime) else at
            # TST-7 also holds for keys computed at run time.
            # The catalogue's trace.key_order is a source diagnostic.
            if entries and after <= entries[-1][0]:
                raise _core.HglError("the keys of a timed trace must strictly increase")
            entries.append((after, published))
        return entries
    return [
        (position * _SMALLEST_STEP, cell) for position, cell in enumerate(trace) if cell is not None
    ]


def _replay_entries(trace, start):
    """CEVAL-2: an input replay cannot publish before its run starts.

    A cell written whole is published as the delta that replaces what
    the input held (the language's TST-9: "a T whole, which replaces what
    the input held, and a `delta<T>` as that change"): for a dictionary,
    each key it held that the cell has not is removed, and for a set each
    member. The keys or members it holds are followed through each cell,
    a delta's among them."""
    entries = []
    for after, published in _deltas(_relative(trace, start)):
        if after < _datetime.timedelta(0):
            raise _core.HglError("a timed trace has a key before the run's start")
        entries.append((_core.add__datetime_duration(start, after), published))
    return entries


def _deltas(cells):
    """Each of a trace's `cells`, `(key, publication)`, with what it
    publishes as the delta its publication is: one cell written whole is
    the delta that publishing it whole records (`_core.whole_delta`), and
    one of a recording given again what it recorded. A delta is as it is.

    The keys or members the value holds are followed through each cell,
    a delta's among them, so that a whole dictionary or set removes what
    it has not (TST-9). The same deltas are replayed to an input (CEVAL-2)
    and given to a `const fn` (TST-16: "A T published whole is held as
    its delta too")."""
    keys = set()
    for key, published in cells:
        if isinstance(published, _Published):
            published = published.delta
        if isinstance(published, _Whole):
            published = _core.whole_delta(published.value, keys, published.fixed)
        if isinstance(published, dict):
            for held, change in published.items():
                if _core._removes(change):
                    keys.discard(held)
                else:
                    keys.add(held)
        elif _is_set_delta(published):
            keys = (keys - set(published.removed)) | set(published.added)
        yield key, published


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
    # CRUN-3 reserves the epoch itself for `never`, even for an empty run.
    if start < _EARLIEST_START:
        raise _core.HglError("the run's start is before the earliest evaluation time")
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


class _Unreadable:
    """A value held that is not whole: equal to no value written."""

    def __eq__(self, other):
        return False

    __hash__ = None


_UNREADABLE = _Unreadable()


# What `_held_by` reads, while a run that reads it is under way: the
# function of the result's port that gives the value it holds, and what
# it has read, by instant. A run is one at a time.
_holding = {"read": None, "held": {}}


@_hg.sink_node
def _held_by(published: _hg.TIME_SERIES_TYPE, _clock: _hg.EvaluationClock = None):
    """Reads the value the result holds at each of its publications, as
    core's `record` records its delta: where it is valid and modified."""
    if published.valid and published.modified:
        try:
            held = _holding["read"](published)
        except _core.HglError as failure:
            # A required part that is not valid leaves no whole value (the
            # owner's decision on hgl_spec issue 126; the provider's
            # `unset_read`): a cell written whole is not equal to what is
            # held, and a delta is compared as recorded.
            if failure.code != "value.unset_read":
                raise
            held = _UNREADABLE
        _holding["held"][_clock.evaluation_time] = held


def _ran(target, replays, record, replayed, arguments, start, end, real_time, seed=None, hold=None):
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
        if hold is not None:
            _held_by(published)
        if real_time:
            # The host's stop: hgraph ends the run when the source
            # publishes.
            _hg.stop_engine(_stop_source())

    # CRUN-7: "A run has one store, which is empty when its graph is
    # built."
    with _hg.GlobalState() as store:
        # The language's UNIT-14: what an application puts in the store
        # before its run statement is there "before the run's graph is
        # built".
        if seed is not None:
            seed(store)
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


def run(target, _record, _real_time, /, _config=None, _store=None, **arguments):
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

    `_store` is what the application sets the run's store with before
    its run statement (the language's UNIT-14), a function of the store,
    `None` where it writes none. Core's CRUN-7: "When its graph is built
    the store holds what the run's application put there".
    """
    start, end = _bounds(_config, _present() if _real_time else _EARLIEST_START)
    if end is not None and end <= start:
        # No cycle runs. hgraph has no run that ends where it starts, so
        # none is made: the graph is not built, started or stopped.
        return None if _record is None else []
    return _ran(target, {}, _record, {}, arguments, start, end, _real_time, _store)


def evaluate(target, _replays, _record, /, _config=None, _real_time=False, _hold=None, **arguments):
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
    # CEVAL-2: "Each tuple holds the absolute evaluation time and the
    # publication's delta ... dense position i is the start plus i
    # smallest steps. A dense placeholder locates any later entries and
    # adds no replay entry or run bound."
    replayed = {
        name: _replay_entries(arguments[name], start)
        for name in _replays
        if name in arguments
    }
    if end is not None and end <= start:
        # Validate replay before graph start, including an empty run
        # (TST-9). hgraph has no run that ends where it starts.
        return _Recording(start, [])
    # `_hold`, a name no input has, reads the value the result holds
    # (see `_Recording`), for a result whose delta is a type of its own.
    _holding["read"], _holding["held"] = _hold, {}
    try:
        output = _ran(
            target, _replays, _record, replayed, arguments, start, end, _real_time, hold=_hold
        )
        held = _holding["held"]
    finally:
        _holding["read"], _holding["held"] = None, {}
    return _Recording(start, [] if output is None else output, held)


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
    # A written trace has no run to admit its keys against. Preserve
    # its absolute instants and anchor only offsets to the earliest start.
    entries = []
    # TST-16: "A T published whole is held as its delta too".
    for key, published in _deltas(_relative(trace)):
        if isinstance(key, _datetime.timedelta):
            key = _core.add__datetime_duration(_EARLIEST_START, key)
        entries.append((key, published))
    return entries


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
    # TST-5: "An expectation's cell written as a `delta<T>` is compared
    # with the delta its cycle published, and one written as a T with the
    # value the result holds once its cycle has published."
    if isinstance(left, _Whole) or isinstance(right, _Whole):
        whole, other = (left, right) if isinstance(left, _Whole) else (right, left)
        if isinstance(other, _Whole):
            return _core._equal(whole.value, other.value)
        if isinstance(other, _Published):
            return other.held not in (None, _UNREADABLE) and _core._equal(whole.value, other.held)
        return _core._equal(whole.value, other)
    if isinstance(left, _Published):
        left = left.delta
    if isinstance(right, _Published):
        right = right.delta
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
    for found in _core._causes(error):
        if isinstance(found, AssertionError) or getattr(found, _REFUSED, False):
            return None
        if isinstance(found, _core.HglError):
            return found.code
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
