"""What the hgraph target's generated code names by hand, for the target.

Written by the hgl compiler beside the modules it generates, with
`_core_native.py`, which imports it: errors and results, structs,
deltas, endpoints, places and copies, sources and the classes of
capabilities. A generated module reaches each as `_core.name`, through
`_core_native`. Core's natives, each named after its candidate, are in
`_core_native.py`.

A `result` is an `Ok` or an `Err`, and an `error` an `Error`: the values
the language's `ok`, `err` and `error` make (ERR-2, ERR-4).

A struct is a class `struct` makes. One of at most two scalar fields is
a value that is never changed, so that one held in two places is as good
as a copy in each; any other is changed where its place holds it, and
what keeps it elsewhere keeps a copy (TYPE-5, `copied`).
"""

import calendar as _calendar
import dataclasses as _dataclasses
import datetime as _datetime
import functools as _functools
import json as _json
import math as _math
import re as _re
import sys as _sys
import zoneinfo as _zoneinfo
from fractions import Fraction as _Fraction

from hgraph import (
    EvaluationClock as _EvaluationClock,
    NodeError as _NodeError,
    Null as _Null,
    REMOVE as _REMOVE,
    REMOVE_IF_EXISTS as _REMOVE_IF_EXISTS,
    K as _K,
    SCALAR as _SCALAR,
    SIZE as _SIZE,
    TIME_SERIES_TYPE as _TIME_SERIES_TYPE,
    TS as _TS,
    TSD as _TSD,
    TSL as _TSL,
    TSLDelta as _TSLDelta,
    TS_OUT as _TS_OUT,
    TimeSeriesSchema as _TimeSeriesSchema,
    set_delta as _set_delta,
    compute_node as _compute_node,
    sink_node as _sink_node,
)

# `abs` is the name of an option of core's `eq_` and `ne_`, and so of a
# parameter below: Python's own is kept under another name.
_magnitude = abs

_I64_MIN = -(2**63)
_I64_MAX = 2**63 - 1
# The smallest duration: one microsecond (TYPE-2).
_MICROSECOND = _datetime.timedelta(microseconds=1)
_NAN = float("nan")
_INFINITY = float("inf")


# ==== What the compiler names by hand ====


def constant(function):
    """A constant of a module (UNIT-13), whose value `function` gives.

    The constant "has that one value wherever it is read": `function` is
    run when the constant is first read, and what it gave is what every
    later read gives. Reading it then, and not where the module is
    loaded, lets a constant name one written after it (UNIT-6).
    """
    return _functools.cache(function)


# The start of the note an `HglError` carries, which says what error a
# capture of the failure gives (see `captured_error`).
_NOTE = "hgl-error: "


class HglError(Exception):
    """A failure of an HGL operation while a program runs.

    `code` is the catalogued code of the failure, where it has one, and
    `context` the provenance of the error it fails with, where that was
    an error retained from an earlier failure (ERR-8: "A consumer that
    propagates a retained error with `?` makes a new failure event,
    whose error keeps its first provenance").

    hgraph gives a capture of a node's failure as text, the failure's
    traceback, and no object: the error is written as one line of JSON
    in a note of the exception, which the traceback ends with, and
    `captured_error` reads it back.
    """

    def __init__(self, message, code=None, context=None):
        super().__init__(message if code is None else f"{code}: {message}")
        self.code = code
        self.context = context
        written = {"message": message, "code": code}
        if context is not None:
            written["context"] = [
                context.node,
                context.phase,
                context.evaluation_time.isoformat(),
            ]
        self.add_note(_NOTE + _json.dumps(written))


def _is_set_delta(value):
    """Whether `value` is a set's delta as hgraph gives one: a
    `frozenset` of its changes, with what it `added` and what it
    `removed` (TYPE-16)."""
    return isinstance(value, frozenset) and hasattr(value, "added") and hasattr(value, "removed")


def _equal(lhs, rhs):
    """`lhs == rhs` for two values of one type, part by part.

    Python's own `==` on a list, a tuple or a dictionary takes two parts
    that are one object to be equal and does not compare them: a NaN
    held in both would then equal itself (NUM-5: "NaN is neither equal
    nor close to any value, including itself"), and whether two values
    are equal would turn on whether one was retained. Each part is
    compared here as its own type compares.

    A set's delta is equal to another where what each added and what
    each removed are (TYPE-16: "`delta<set<T>>` has `added` and
    `removed`"). hgraph's own equality of two takes `Removed(1)` for
    `1`, so a delta that removed a member would equal one that added it.
    """
    if _is_set_delta(lhs) or _is_set_delta(rhs):
        return (
            _is_set_delta(lhs)
            and _is_set_delta(rhs)
            and frozenset(lhs.added) == frozenset(rhs.added)
            and frozenset(lhs.removed) == frozenset(rhs.removed)
        )
    if isinstance(lhs, (list, tuple)):
        return (
            isinstance(rhs, (list, tuple))
            and len(lhs) == len(rhs)
            and all(_equal(left, right) for left, right in zip(lhs, rhs))
        )
    if isinstance(lhs, dict):
        return (
            isinstance(rhs, dict)
            # A list's delta also holds its `size_modified` (see `list_delta`).
            and size_modified(lhs) == size_modified(rhs)
            and lhs.keys() == rhs.keys()
            and all(_equal(value, rhs[key]) for key, value in lhs.items())
        )
    # A key or a position removed is one change, however hgraph marks it:
    # a delta made here removes a key with `REMOVE_IF_EXISTS` (`dict_delta`),
    # and one hgraph publishes with `REMOVE`.
    if _removes(lhs) or _removes(rhs):
        return _removes(lhs) and _removes(rhs)
    return lhs == rhs


def _causes(error):
    """`error`, and then each error it was raised from or while handling,
    in turn: what Python keeps as its `__cause__`, or else as its
    `__context__`. hgraph raises an error of its own for a node's
    failure, with what the node raised under it. Each is given once,
    where an error is among its own causes."""
    seen = set()
    while error is not None and id(error) not in seen:
        seen.add(id(error))
        yield error
        error = error.__cause__ or error.__context__


class Result:
    """A `result`: an `Ok` or an `Err` (ERR-2). The class a temporal
    `result` is a time series of."""

    __slots__ = ()


class Ok(Result):
    """A success of a `result`, which owns its value (ERR-2)."""

    __slots__ = ("value",)

    def __init__(self, value):
        self.value = value

    # ERR-2: "Equality ... compares tag then selected payload. This
    # adds no ordering/hash": with `__eq__` and no `__hash__`, Python
    # makes the class unhashable.
    def __eq__(self, other):
        return type(other) is Ok and _equal(self.value, other.value)

    def __repr__(self):
        return f"ok({self.value!r})"


class Err(Result):
    """A failure of a `result`, which owns its error (ERR-2)."""

    __slots__ = ("error",)

    def __init__(self, error):
        self.error = error

    def __eq__(self, other):
        return type(other) is Err and _equal(self.error, other.error)

    def __repr__(self):
        return f"err({self.error!r})"


class Error:
    """An `error`: a message, and a code that may be null (ERR-4).

    "Equality excludes provenance": two errors are equal when their
    messages and their codes are. `context` is the provenance, an
    `ErrorContext`, which "a constructed error has none" of.
    """

    __slots__ = ("message", "code", "context")

    def __init__(self, message, code=None, context=None):
        self.message = message
        self.code = code
        self.context = context

    def __eq__(self, other):
        return type(other) is Error and (self.message, self.code) == (other.message, other.code)

    def __repr__(self):
        return f"error(message: {self.message!r}, code: {self.code!r})"


def _raise(error):
    """Fails with `error`, an `Error`: the HGL error it is, with its
    message, its code and its provenance (ERR-8: a failure "keeps its
    first provenance")."""
    raise HglError(error.message, code=error.code, context=error.context)


class ErrorContext:
    """The provenance of an error a graph failure gave (ERR-4): "`.node:
    str`, `.phase: str`, one of `"evaluate"`, `"start"`, and `"stop"`,
    and `.evaluation_time: datetime`"."""

    __slots__ = ("node", "phase", "evaluation_time")

    def __init__(self, node, phase, evaluation_time):
        self.node = node
        self.phase = phase
        self.evaluation_time = evaluation_time

    # OPS-2: "`==` and `!=` compare two values of one type": two
    # contexts are equal when their three parts are. With `__eq__` and
    # no `__hash__`, Python makes the class unhashable, as `Error` is.
    def __eq__(self, other):
        return type(other) is ErrorContext and (self.node, self.phase, self.evaluation_time) == (
            other.node,
            other.phase,
            other.evaluation_time,
        )

    def __repr__(self):
        return f"context(node: {self.node!r}, phase: {self.phase!r}, evaluation_time: {self.evaluation_time!r})"


# A frame of a traceback, with the name of the function it is in.
_FRAME = _re.compile(r'\s*File ".*", line \d+, in (\S+)$')


def _phase_of(failure):
    """The phase hgraph's `NodeError` `failure` was met in (ERR-4): one
    of `"evaluate"`, `"start"`, and `"stop"`.

    hgraph captures a failure of the start or the stop of a child that
    the evaluation of what it captures starts or stops, a branch of a
    `switch_` that is selected or left, as it captures a failure of an
    evaluation. The node's `start` and `stop` are the target's functions
    `_start_name` and `_stop_name`, which no name of a program is
    written as, and the first frame of the traceback is the function
    hgraph called.
    """
    for line in failure.error_msg.splitlines():
        frame = _FRAME.match(line)
        if frame is not None:
            called = frame.group(1)
            if called.startswith("_start_"):
                return "start"
            if called.startswith("_stop_"):
                return "stop"
            return "evaluate"
    return "evaluate"


class Escaped(Exception):
    """A failure that is no HGL error, let out of every capture.

    ERR-1: "Distinguish source rejection, execution failure, ordinary
    result data, and test/infrastructure failure." An execution failure
    is an `HglError`; anything else a node raises, a form the target
    does not write or a fault of the target's own, is no failure of the
    program, and a capture does not take it (ERR-7). It carries no code,
    so no `assert raises` takes it either (TST-11).
    """


def error_of(failure, evaluation_time):
    """The `error` a capture gives for hgraph's `NodeError` `failure`,
    met at `evaluation_time` (ERR-4, ERR-7).

    An `HglError` says its error in the note it carries, which is the
    last line of the traceback (see `HglError`); the note of one met
    while another exception was handled is not. "On first graph
    failure, attach original node identity, phase, and evaluation time;
    propagation/capture preserve them": an error with no context of its
    own is given the failing node's, in the phase it was met in
    (`_phase_of`). Any other failure is no HGL error, and is raised
    again as `Escaped`.
    """
    lines = failure.error_msg.rstrip().splitlines()
    written = lines[-1] if lines and lines[-1].startswith(_NOTE) else None
    if written is None:
        raise Escaped(f"{failure.label} failed with what is no HGL error:\n{failure.error_msg}")
    written = _json.loads(written[len(_NOTE):])
    context = written.get("context")
    if context is None:
        context = ErrorContext(failure.label, _phase_of(failure), evaluation_time)
    else:
        node, phase, at = context
        context = ErrorContext(node, phase, _datetime.datetime.fromisoformat(at))
    return Error(written["message"], written["code"], context)


@_compute_node
def captured_error(failure: _TS[_NodeError], _clock: _EvaluationClock = None) -> _TS[Error]:
    """ERR-7: a capture's `.error`, "a connection of `error`". "Each
    evaluation failure ticks its canonical error; before failure the
    stream is invalid, and later success leaves its retained error valid
    but unmodified": hgraph's stream of the child's failures, each made
    an `error`."""
    return error_of(failure.value, _clock.evaluation_time)


@_sink_node
def _escapes(failure: _TS[_NodeError], _clock: _EvaluationClock = None):
    """What hgraph's `try_except` captures and a capture does not.

    ERR-8: "Start/stop failures escape, including lifecycle work
    triggered during evaluation". hgraph's `try_except` captures the
    failure of a `start` or a `stop` it meets while it evaluates the
    child: it is raised again here, with its error and the context of
    its first failure, which says its phase. ERR-1: a failure that is no
    HGL error is raised again too, as `Escaped` (`error_of`). A capture
    around this one meets the failure again, as this node's, and lets it
    out again: each boundary lets it out."""
    error = error_of(failure.value, _clock.evaluation_time)
    if error.context.phase != "evaluate":
        _raise(error)


def captured(attempt):
    """A capture: `attempt`, what hgraph's `try_except` gives, with what
    lets out of it the failures no capture takes (`_escapes`) wired
    beside it (ERR-1, ERR-7, ERR-8)."""
    _escapes(attempt.exception)
    return attempt


class Propagated(Exception):
    """Leaves a value function with the failure a `?` in it met.

    The function catches it and returns `failure` (ERR-5).
    """

    def __init__(self, failure):
        super().__init__("a failure propagated by `?`")
        self.failure = failure


class Failed:
    """What a function with `var` parameters gives in its result's place
    where it fails, beside what each of them held then: `error`, what
    was raised. The language's FN-15: "A function that fails leaves the
    place holding what its parameter held when it failed: no change it
    had made is undone." The call assigns each place, and then raises
    `error` again (`raise_if_failed`)."""

    __slots__ = ("error",)

    def __init__(self, error):
        self.error = error


def raise_if_failed(result):
    """Raises again what a function with `var` parameters raised, where
    `result`, what it gave in its result's place, says it failed
    (`Failed`). The call has assigned its places by then."""
    if type(result) is Failed:
        raise result.error


def propagated(result):
    """`result?` in a value function: the value of a success. A failure
    leaves the function, which returns it (ERR-5)."""
    if type(result) is Ok:
        return result.value
    raise Propagated(result)


def succeeded(result):
    """`result?` in a `when`: the value of a success. A failure fails
    the node with the error it holds (ERR-5)."""
    if type(result) is Ok:
        return result.value
    _raise(result.error)


@_compute_node
def lifted_is_ok(value: _TS[Result]) -> _TS[bool]:
    """ERR-3: "In composition a test of a connection is an ordinary
    lifted call (FN-6): it gives a connection of `bool` and no fact":
    `is_ok` of each result `value` publishes."""
    return type(value.value) is Ok


@_compute_node
def lifted_is_err(value: _TS[Result]) -> _TS[bool]:
    """`is_err` lifted over a connection of `result`, as `lifted_is_ok`
    is (ERR-3, FN-6)."""
    return type(value.value) is Err


def member(enum, value):
    """`Side(1)` or `Side("buy")`: the member of `enum` with that number
    or that exact name (TYPE-7). "A value that is none fails where it
    is evaluated."

    A member's name is kept from `Enum`'s own as the compiler keeps
    it: one that starts with `_` or is `mro`, under any number of
    `hgl_`, has one more `hgl_` before it.
    """
    try:
        if isinstance(value, str):
            rest = value
            while rest.startswith("hgl_"):
                rest = rest[4:]
            kept = rest.startswith("_") or rest == "mro"
            return enum["hgl_" + value if kept else value]
        return enum(value)
    except (KeyError, ValueError):
        # The class is named with its module's path, `demo::Side`.
        declared = enum.__name__.rsplit("::", 1)[-1]
        raise HglError(f"`{declared}` has no member {value!r}") from None


def struct(name, fields, by_value=False):
    """A struct of the program, as a class (TYPE-3: "Structs are
    nominal, with public ordered fields and no methods").

    `fields` is every field the struct has, its parents' first, each by
    its Python name. A value is made with each field by name.

    A struct that is `by_value`, the compiler's word for one of at most
    two scalar fields (the owner's rule on the compiler's issue 151), is
    never changed after it is made: a change of a field makes a new one
    (`set_field`). A value of any other struct that a `var`, a `state`
    or a `cache` holds is that place's own, and is changed where it is;
    what keeps it elsewhere keeps a copy (`copied`): "Retained results
    own their data; later mutation of a source cannot change an earlier
    retained value" (TYPE-5). The class is frozen all the same, so that
    nothing but `set_field` changes a value, and none that hgraph holds.

    Two are equal when they are of one struct, with the same generic
    arguments (see `applied`), and each field is equal. The fields are
    always compared: a value is not taken to equal itself, since a
    field of it may be a NaN.

    The class is of the module that makes it, the module of the struct's
    own: hgraph knows a class, and a bundle's schema (see `schema`), by
    its module and its name, so two modules' structs of one name are two
    types to it too.
    """

    def equal(self, other):
        return type(other) is type(self) and all(
            _equal(getattr(self, field), getattr(other, field)) for field in fields
        )

    def hashed(self):
        return hash(tuple(getattr(self, field) for field in fields))

    return _dataclasses.make_dataclass(
        name,
        fields,
        frozen=True,
        slots=True,
        kw_only=True,
        eq=False,
        # `_hgl_fields` names the fields, for what looks inside a value
        # (`copied`), and `_hgl_by_value` says whether a value is ever
        # changed where it is (`set_field`).
        namespace={
            "__eq__": equal,
            "__hash__": hashed,
            "_hgl_fields": tuple(fields),
            "_hgl_by_value": by_value,
        },
        module=_sys._getframe(1).f_globals.get("__name__", __name__),
    )


# The class of each generic struct with its arguments in place, by the
# struct and the arguments.
_applications = {}


def applied(struct, arguments):
    """A generic struct with its arguments in place, `Box<i64>`, as a
    class of its own (TYPE-3: "Generic applications are invariant by
    origin and complete canonical arguments").

    `arguments` is the arguments as the compiler writes them, one text
    for each application. The class is made once, so a `Box<i64>` is no
    `Box<f64>`, whatever the two hold.
    """
    key = (struct, arguments)
    made = _applications.get(key)
    if made is None:
        made = type(
            f"{struct.__name__}<{arguments}>",
            (struct,),
            {"__slots__": (), "__module__": struct.__module__},
        )
        _applications[key] = made
    return made


def with_field(value, /, **field):
    """`place.field = new` (EXPR-3) for a struct that is a value (see
    `struct`), and for one that is no place's own: the struct `value`
    with one field another value. `value` itself is as it was, for
    whatever else holds it; the compiler assigns what this gives to the
    place.

    `value` is taken by position only: a field may be named `value`.
    The new struct keeps the field's value, which the compiler gives it
    as a copy where something else holds it.
    """
    return _dataclasses.replace(value, **field)


def set_field(value, field, new, /):
    """`place.field = new` (EXPR-3) where the place is a field of the
    struct a `var`, a `state` or a `cache` holds itself: the struct is
    changed where it is, and keeps `new`, which the compiler gives as a
    copy where something else holds it. What took the struct before
    took a copy of it.

    A struct that is a value (see `struct`) is not changed: it may be
    held under the type of an abstract parent, which is changed where it
    is, and other names may hold it. It is given a new one, as
    `with_field` gives. The compiler assigns what this gives to the
    place.
    """
    if type(value)._hgl_by_value:
        return _dataclasses.replace(value, **{field: new})
    # The class is frozen: `object.__setattr__` sets a field of it all
    # the same, as the dataclass's own `__init__` does.
    object.__setattr__(value, field, new)
    return value


# The bundle schema of each struct's class, made once (see `schema`).
_schemas = {}


def schema(struct, fields):
    """The schema of hgraph's bundle, `TSB`, that a temporal value of the
    struct `struct` is (the language's TYPE-9: "Temporalization
    recursively maps tuples/structs to child fields").

    `fields` is each field's name and its temporal type, in the
    struct's order. The schema is made once for a class, so that every
    annotation of the struct names one type to hgraph. It is of the
    class's module, named after the class: hgraph knows a schema by its
    module and its name, so two modules' structs of one name, and two
    applications of a generic struct, are two types (TYPE-3: "Structs
    are nominal").
    """
    made = _schemas.get(struct)
    if made is None:
        made = type(
            f"{struct.__name__}_ts",
            (_TimeSeriesSchema,),
            {"__annotations__": dict(fields), "__module__": struct.__module__},
        )
        _schemas[struct] = made
    return made


def narrowed(port, bundle, fields):
    """A temporal struct's connection, `port`, where an abstract struct
    it inherits from is expected (the language's TYPE-3: "A struct is a
    value where an abstract struct it inherits from is expected"), as
    hgraph's bundle `bundle` of the abstract struct: each of its
    `fields` the struct's own field's connection. It is made where the
    graph is built, and adds no node."""
    return bundle.from_ts(**{field: port[field] for field in fields})


def struct_delta(**fields):
    """A struct's delta made from its properties (the language's
    TYPE-16), as hgraph's delta of a bundle: a `dict` of each field that
    changed with its delta. A property that is null, "where the field
    did not change", is left out; a published null, `Null`, is kept."""
    return {field: delta for field, delta in fields.items() if delta is not None}


def changed(delta, field):
    """The property `field` of a struct's delta (TYPE-16): the field's
    delta, or null "where the field did not change"."""
    return delta.get(field)


def dict_delta(updated=None, removed=()):
    """A dictionary's delta made from its properties (the language's
    TYPE-16: "`updated`, a `dict<K, delta<V>>` of each key added or
    changed with its delta, and `removed`, a `set<K>`"), as hgraph's
    delta of a `TSD`: one `dict` of each key updated with its delta, and
    of each removed with `REMOVE_IF_EXISTS`: "A `removed` that names a key
    a dictionary does not hold is ignored, and is no failure" (TYPE-16).
    A property left out is empty. What is updated is copied: the program
    may go on changing the dictionary it was given in."""
    changes = {} if updated is None else copied(updated)
    for key in removed:
        changes[key] = _REMOVE_IF_EXISTS
    return changes


def _marked(changes, removed):
    """`changes`, a `dict` of each key or position updated with its
    delta, with each of `removed` given hgraph's `REMOVE`: a `TSD`'s and
    a `TSL`'s delta (TYPE-16's `updated` and `removed`)."""
    for key in removed:
        changes[key] = _REMOVE
    return changes


def list_delta(updated=None, removed=(), size_modified=None):
    """A list's delta made from its properties (TYPE-16: "`updated`,
    each changed position with its delta; `removed`, the positions that
    lost their value ... and `size_modified`"), as hgraph's own delta of
    a `TSL`, its `TSLDelta`: a `dict` of each position updated with its
    delta, and of each removed with `REMOVE`, that holds `size_modified`
    too. A property left out is empty, or null for `size_modified`.
    hgraph's `TSLDelta` copies the `dict` it is given, which the program
    may go on changing; a delta in it is never changed."""
    return _TSLDelta({} if updated is None else updated, removed=tuple(removed), size_modified=size_modified)


def size_modified(delta):
    """The property `size_modified` of a list's delta (TYPE-16): the
    new size, or null where the size did not change. hgraph's delta of a
    fixed list is a plain `dict`, whose size never moves."""
    return getattr(delta, "size_modified", None)


def _held_size(port):
    """How many positions an unsized list holds, given its port: none
    where it is not valid. END-4: "Invalidation clears validity,
    modification, payload, and delta", and hgraph keeps an invalidated
    `TSL`'s positions, each not valid."""
    return len(port) if port.valid else 0


def _ticked(port):
    """Makes the unsized list at `port` tick in this evaluation where
    what is published next would change nothing hgraph can see.

    TIME-7 has a write publish ("No write means no tick"), and TIME-9 has
    "A published empty list ... valid". hgraph 0.9.5's unbounded `TSL`
    does not tick for a publication that leaves it as it was, the empty
    list it holds or holds no value yet among them, and so is never
    valid where an empty list is published first. It does tick for a
    position added and dropped again in one evaluation, which leaves
    nothing in the delta read: so the list is grown by one position here,
    and what is published next gives it its size again. Where the port
    has published in this evaluation already, it has ticked."""
    if not port.modified:
        port.value = _TSLDelta({}, size_modified=len(port) + 1)


def _whole_list(value, port):
    """What an unsized list of elements that publish whole publishes for
    `value`, a whole list of it (TIME-7: "a complete value of the
    result's type T, published whole"): a tuple of its elements, which
    hgraph's unbounded `TSL` takes as the whole list, a position for each
    element, dropping those past them. The tuple is a copy: hgraph keeps
    no list the program goes on changing.

    An empty list published where the list holds none, or holds no value
    yet, would not tick (`_ticked`), so `port`, the list's port of the
    node's output, is made to: with no port, assigned to an entry of a
    dictionary `out` or a position of a list `out`, which
    `assigned_to_out` names once, it is not written. A source's
    statements give `DEFERRED`: its node publishes the value through its
    port when it is due (`published_into`)."""
    if not value and port is not DEFERRED:
        if port is None:
            raise NotImplementedError(
                "the hgraph target does not write an empty unsized list assigned to an entry "
                "or a list's position of `out` yet"
            )
        if len(port) == 0:
            _ticked(port)
    return tuple(value)


# The port a source's statements bundle a value with (`bundled`): they make
# it before it is due, and its node publishes it through its own port then
# (`published_into`).
DEFERRED = object()


def list_replaced(port, value):
    """What a node publishes to its unsized list at `port` for `value`, a
    whole list (see `_whole_list`)."""
    return _whole_list(value, port)


def list_applied(port, delta):
    """What a node publishes to its unsized list at `port`, of elements
    of any type, for `delta`, the list's delta (TYPE-16), as hgraph's
    unbounded `TSL` takes it: a `TSLDelta` of the positions updated and
    removed, and the size the list has after it, counted from the size it
    holds (`_held_size`).

    TYPE-16: "A delta written with a smaller `size_modified` and no
    `removed` shrinks the list. A negative `size_modified` written
    reduces the list by that count: `size_modified: -1` drops its last
    position, and a count more than the list holds leaves it empty, which
    is no failure. An `updated` entry written at the position after a
    `list<V>`'s last, with no `size_modified`, extends the list to hold
    it." hgraph fails a run given a negative size, so the count is the
    size here. `removed` alone, "the positions that lost their value",
    leaves each not valid and the size as it was. TYPE-16, agreed on
    hhenson/hgl_spec#124: "an `updated` entry written further past a
    `list<V>`'s last position extends the list as well, and the positions
    between hold no value and are not valid", as hgraph's `TSL` grows.

    A negative position fails, as an index outside a list does (EXPR-3).
    An entry at or past a size written with it contradicts the delta, and
    fails (the owner's decision on hhenson/hgl_spec#124). A removed
    position the list does not hold lost no value, and is ignored (the
    owner's decision there). A delta that changes nothing hgraph sees
    still ticks (`_ticked`)."""
    held = _held_size(port)
    changes = updated(delta)
    gone = removed(delta)
    for position in (*changes, *gone):
        if position < 0:
            raise HglError(f"the position {position} is outside a list of {held}")
    written = size_modified(delta)
    if written is None:
        size = max(held, max(changes) + 1) if changes else held
    elif written < 0:
        size = max(held + written, 0)
    else:
        size = written
    # The owner's decision on hhenson/hgl_spec#124: an entry at or past the
    # size written with it is "a failure it contradicts itself". No
    # catalogued code is one for it, so the failure has none, as an index
    # outside a list's has none.
    beyond = [position for position in changes if written is not None and position >= size]
    if beyond:
        raise HglError(
            f"the delta updates the position {min(beyond)} and gives the list a size of {size}"
        )
    if size == held and not changes:
        _ticked(port)
    return _TSLDelta(changes, removed=tuple(position for position in gone if position < size), size_modified=size)


def list_published(port, published):
    """What is published to the unsized list at `port` for `published`,
    made before it is due: a list's delta (`list_applied`) or a whole
    list (`list_replaced`), applied to the list as the list is then. A
    source's statements make what it yields so, and so is a part of a
    composite's delta made (`published_into`)."""
    if isinstance(published, dict):
        return list_applied(port, published)
    # A whole list of what a generic replay is given, whose type the
    # compiler did not know: its `_whole_list` takes elements that publish
    # whole and are not null.
    for part in published:
        if part is None or isinstance(part, (list, tuple, dict, frozenset)) or hasattr(part, "_hgl_fields"):
            raise NotImplementedError(
                "the hgraph target does not write a whole unsized list of nullable or "
                "composite elements yielded by a source yet"
            )
    return list_replaced(port, published)


def published_into(port, value, shape):
    """What is published through `port`, a node's output, for `value`, a
    whole value bundled (`bundled`) or a delta, of a struct or a tuple
    that holds an unsized list, or of the list itself (TIME-7): each list
    in it as `list_published` makes it, counted from the size it holds
    (TYPE-16), and every other part as it is.

    `shape` is what the compiler wrote for where the lists are: `"list"`
    for one, a `dict` of the fields that hold one for a struct, a tuple
    of each position's for a tuple, and `None` for a part that holds
    none. A struct's value and its delta are each a `dict` of its fields,
    a tuple's each a tuple, a null where a position did not change."""
    if value is None or shape is None:
        return value
    if shape == "list":
        return list_published(port, value)
    if isinstance(shape, dict):
        return {field: published_into(port[field], part, shape.get(field)) for field, part in value.items()}
    return tuple(
        published_into(port[position(index)], part, within)
        for index, (part, within) in enumerate(zip(value, shape))
    )


def _emptied(port, shape):
    """Drops each position of each unsized list `shape` names in what
    `port` holds (`invalidated`)."""
    if shape is None:
        return
    if shape == "list":
        if len(port) > 0:
            port.value = _TSLDelta({}, size_modified=0)
        return
    parts = shape.items() if isinstance(shape, dict) else ((position(index), part) for index, part in enumerate(shape))
    for key, part in parts:
        _emptied(port[key], part)


def invalidated(port, shape):
    """`invalidate(out)` of an output that holds an unsized list (END-4:
    "Invalidation clears validity, modification, payload, and delta").
    hgraph's unbounded `TSL` keeps its positions when it is invalidated,
    and the next delta read would name them removed: each list is emptied
    first, which no one reads, since the list is not valid after.
    `shape` is as `published_into` has it."""
    _emptied(port, shape)
    port.invalidate()


def list_position(port, index):
    """The endpoint of an unsized list's position `index`, hgraph's child
    of the list's port. TIME-1: "A child of a collection ... is an
    endpoint whether or not it is valid": a position the list does not
    hold, before it publishes, while it is short, or after a shrink drops
    it (TYPE-16), is a child that is not valid, as a dictionary's absent
    key is (`entry_port`). A negative index is no position: "An index
    outside a list fails where it is evaluated" (EXPR-3)."""
    if port is _ABSENT_ENTRY:
        return port
    if index < 0:
        _index(index, _held_size(port))
    if index >= _held_size(port):
        return _ABSENT_ENTRY
    return port[index]


def list_child(port, index):
    """The port of the position `index` of an unsized list's port, which
    a part of `out` is assigned through (the language's TIME-7). An index
    outside what the list holds fails (EXPR-3: "An index outside a list
    fails where it is evaluated"); hgraph's own would add positions up to
    it."""
    _index(index, _held_size(port))
    return port[index]


# What `list_value` is given where a list's element type has no value
# for a position that is not set.
_NO_HOLE = object()


def list_value(port, hole=_NO_HOLE):
    """A temporal list of elements that publish whole, read whole at its
    port, a fixed list or an unsized one: a Python list of its elements,
    made anew. The one reading of a list whole: a hook's, a lifted value
    function's argument and a trace cell's held value.

    The language's TIME-1 (the owner's decision on hhenson/hgl_spec#124):
    a position of an unsized list that holds no value, a gap or one
    removed, reads as the element type's value for "not set" where the
    type has one, `hole`, NaN for an `f64`; an optional element reads
    null, of a fixed list too (TYPE-15); and the read fails where there is
    none, as for an `i64` (`unset_read`). A list that is not valid by its
    own `valid` (`valid_of`) has no whole value either."""
    if port.all_valid:
        return list(port.value)
    if hole is _NO_HOLE or not valid_of(port, ("list", None)):
        unset_read()
    return [
        child.value if child.valid else hole for child in (port[index] for index in range(len(port)))
    ]


def list_within(delta, size):
    """`delta`, a fixed list's of `size` positions, as it is published
    (TYPE-16): "A `list<V, N>` keeps its bounds: ... one in a delta made as
    the program runs fails the write that publishes it, as an index
    outside a list does (EXPR-3)". A removed position is one of its
    positions, which hgraph's bounded `TSL` makes not valid. A
    `size_modified` that moves the size, a negative count among them,
    would not keep the bounds, and fails too. The catalogue has no
    execution code for either, so the failure has none, as `item`'s has
    none. A whole list given for the delta, as a replay is given a
    trace's cell written whole, is published as it is."""
    if not isinstance(delta, dict):
        return delta
    for position in delta:
        if not 0 <= position < size:
            raise HglError(f"the position {position} is outside a list of {size}")
    written = size_modified(delta)
    if written is not None and (written if written >= 0 else max(size + written, 0)) != size:
        raise HglError(f"a list of {size} keeps its size, which the delta moves")
    # hgraph's bounded `TSL` takes the positions alone.
    return dict(delta)


def valid_of(port, shape):
    """`valid` of a temporal value at `port` that holds a list (TIME-9,
    END-7): "true where any element, field or entry is valid, and for a
    published empty list or dictionary, which has none", each part by its
    own `valid`.

    hgraph's `TSL` stays valid once it has ticked, where each position
    has since lost its value, and its `TSB` once a field has, so hgraph's
    own `valid` is not this where a list is in what the port holds.
    `shape` is what the compiler wrote for the type: `("list", part)` for
    a list of elements of `part`, `("fields", {name: part})` for each
    field of a struct and position of a tuple, `("entries", part)` for a
    dictionary, and `None` for a part whose `valid` is hgraph's."""
    if not port.valid:
        return False
    if shape is None:
        return True
    kind, part = shape
    if kind == "list":
        # An output's port is read by position, as an input's is.
        size = len(port)
        return size == 0 or any(valid_of(port[index], part) for index in range(size))
    if kind == "fields":
        return any(valid_of(port[field], within) for field, within in part.items())
    keys = list(port.keys())
    return not keys or any(valid_of(port[key], part) for key in keys)


def all_valid_of(port, shape):
    """`all_valid` of a temporal value at `port` whose parts hold a list
    (TIME-9: "true where every one is, by its own `valid`, one level
    deep ... true of a valid empty collection"), each part by `valid_of`.
    `shape` is as `valid_of` has it."""
    if not port.valid:
        return False
    kind, part = shape
    if kind == "list":
        return all(valid_of(port[index], part) for index in range(len(port)))
    if kind == "fields":
        return all(valid_of(port[field], within) for field, within in part.items())
    return all(valid_of(port[key], part) for key in port.keys())


@_sink_node(active=(), valid=())
def discarded(published: _TIME_SERIES_TYPE):
    """Keeps what publishes `published` in the graph, and reads nothing of
    it: hgraph leaves out of a graph a node whose output nothing reads
    (see `each` in the compiler, which gives a loop's child graphs an
    output only so that hgraph runs them)."""


_constants = {}


def constant_of(tp):
    """The node that publishes, once as the graph starts, a constant of
    the temporal type `tp` that holds an unsized list (FN-3: "an ordinary
    value in a branch is then wired as a constant"). hgraph's own `const`
    has no port of its output to make an empty list tick (`_ticked`), so
    this node, given `_core.Held((value, shape))`, publishes
    `bundled(value, shape)` to its own. One node is made for each type."""
    made = _constants.get(tp)
    if made is None:

        def _constant(tick: _TS[bool], given: object, _output: _TS_OUT[tp] = None) -> tp:
            value, shape = given.value
            return bundled(value, shape, _output)

        made = _compute_node(_constant)
        _constants[tp] = made
    return made


def _removes(change):
    return change is _REMOVE or change is _REMOVE_IF_EXISTS


@_compute_node(valid=("index",))
def position_in(
    values: _TSL[_TS[_SCALAR], _SIZE], index: _TS[int], _output: _TS_OUT[_SCALAR] = None
) -> _TS[_SCALAR]:
    """`values[index]` in a graph: the child of an unsized list that
    `index` selects (the language's TYPE-9, "lists to child positions";
    EXPR-7, "Structural projection selects a child connection"). TIME-1:
    "A child of a collection ... is an endpoint whether or not it is
    valid": a position the list does not hold, or that lost its value, is
    a child that is not valid, and nothing is published for it; hgraph's
    own `getitem_` fails the run where the list no longer holds it. It
    publishes where the index ticks or the selected child does; where the
    child goes or loses its value, what it published before is
    invalidated (END-4). It runs where the list is not valid, invalidated
    whole among them, and requires only the index valid. A negative index
    fails, as an index outside a list does (EXPR-3)."""
    selected = index.value
    if selected < 0:
        _index(selected, _held_size(values))
    child = values[selected] if selected < _held_size(values) else None
    if child is None or not child.valid:
        if _output.valid:
            _output.invalidate()
        return None
    if index.modified or child.modified or not _output.valid:
        # A child that holds a null publishes it (see `published`).
        return published(child.value)


@_compute_node(valid=("key",))
def entry_of(
    dictionary: _TSD[_K, _TS[_SCALAR]], key: _TS[_K], _output: _TS[_SCALAR] = None
) -> _TS[_SCALAR]:
    """`dictionary[key]` in a graph: the child of a temporal dictionary
    that `key` selects (the language's TYPE-9, "dictionaries to keyed
    children"; EXPR-7, "Dictionary temporal key selection may produce a
    reference designation"). TIME-1: "A child of a collection ... is an
    endpoint whether or not it is valid": a key that is absent selects a
    child that is not valid, and nothing is published for it. ERR-6's
    strict read is of an ordinary or atomic dictionary, and not this.
    It publishes where the key ticks or the selected child does, so
    another child's tick is silent; where the selected child is gone or
    is not valid, what it published before is invalidated (END-4). It
    runs where the dictionary is not valid, invalidated whole among them,
    and requires only the key valid."""
    selected = key.value
    child = dictionary[selected] if dictionary.valid and selected in dictionary.keys() else None
    if child is None or not child.valid:
        if _output.valid:
            _output.invalidate()
        return None
    if key.modified or child.modified or not _output.valid:
        # A child that holds a null publishes it (see `published`).
        return published(child.value)


class _AbsentEntry:
    """The endpoint of a dictionary's entry whose key the dictionary has
    not: it exists and is not valid (the language's TIME-1: "A child of a
    collection ... is an endpoint whether or not it is valid")."""

    valid = False
    all_valid = False
    modified = False
    value = None
    delta_value = None
    last_modified_time = None

    def __getitem__(self, key):
        # A child of an entry that is absent is absent too: a field, a
        # position, an entry.
        return self

    def keys(self):
        return ()


_ABSENT_ENTRY = _AbsentEntry()


def _index(index, size):
    """Fails where `index` is no position of a sequence of `size`
    elements, a list's or a tuple's or a fixed list's port: "An index
    outside a list fails where it is evaluated" (EXPR-3). Python's own
    indexing would count a negative index back from the end."""
    if not 0 <= index < size:
        raise HglError(f"the index {index} is outside a sequence of {size}")


def _key(dictionary, key):
    """Fails where `dictionary` has no entry for `key`: a strict read
    (ERR-6: "absence fails"), with the code `value.missing_key`."""
    if key not in dictionary:
        raise HglError(f"the dictionary has no key {key!r}", code="value.missing_key")


def position_of(port, index):
    """The endpoint of a fixed list's element at `index`, hgraph's child
    of the list's input (TIME-1). "An index outside a list fails where it
    is evaluated" (EXPR-3). A position of an absent entry is absent."""
    if port is _ABSENT_ENTRY:
        return port
    _index(index, len(port))
    return port[index]


def entry_port(port, key):
    """The endpoint of a dictionary's entry for `key`, hgraph's child of
    the dictionary's input, or one that is not valid where the key is
    absent (TIME-1)."""
    return port[key] if key in port.keys() else _ABSENT_ENTRY


def _child_valid(port, shape):
    """Whether a child's endpoint is valid: hgraph's `valid`, or, where a
    list is in the child, the language's by its `shape` (`valid_of`)."""
    return port.valid if shape is None else valid_of(port, shape)


def child_modified(port, shape=None):
    """`modified` of a child's endpoint (END-8): whether it published in
    this cycle, held to its being valid, as an input's is (END-4)."""
    return _child_valid(port, shape) and port.modified


def child_last_modified(port, shape=None):
    """`last_modified` of a child's endpoint (END-8, END-6): null where it
    is not valid."""
    return port.last_modified_time if _child_valid(port, shape) else None


def _unset(port):
    """The failure of a read of a child that is not valid (TIME-1, ERR-14),
    with the code `value.unset_read`."""
    return HglError("the child read is not valid", code="value.unset_read")


def child_delta(port, shape=None):
    """`delta_value` of a child's endpoint (END-8): what it published in
    this cycle. TIME-1: "a read of a child's value, or of its
    `delta_value`, requires that child's admission"; where that is
    checked when it runs, a child that is not valid fails it."""
    if not _child_valid(port, shape):
        raise _unset(port)
    return port.delta_value


def window_values(port):
    """A window's value read in a hook (the language's TYPE-11): "a
    `list<V>` of the values held, oldest first". hgraph's `TSW` holds them
    as an array, each made the Python value it is."""
    values = port.value
    return list(values.tolist() if hasattr(values, "tolist") else values)


def designation(output, reference):
    """What a node publishes to its `ref` result, `output`, for
    `reference`, the designation a `return` names (the language's FN-16):
    the reference, or nothing where the result holds it already. TIME-10:
    "A designation published again unchanged is no tick, and wakes no
    subscriber", and core's END-10: "A publication of the designation it
    holds is no tick". hgraph's reference output would tick for it, and
    a `ref` parameter given it would be modified."""
    if output.valid and output.value == reference:
        return None
    return reference


def designate(output, reference):
    """`out = items[idx]` of a `ref` result (the language's FN-16,
    proposed): `output` designates what `reference` does, and does not
    tick where it does already (`designation`)."""
    if not (output.valid and output.value == reference):
        output.value = reference


def checked_child(port, read):
    """The read of a child known only when it runs, `parts[i]` (TIME-1):
    `read` of its endpoint `port`. "A child that is not valid fails it
    (`value.unset_read`)"."""
    if not port.valid:
        raise _unset(port)
    return read(port)


def checked_child_result(port, read):
    """The read of a child known only when it runs where its place expects
    `result<T, error>` (ERR-14): "`ok` of the child's value, or, for a
    child that is not valid, `err` of an `error` whose code is
    `value.unset_read`"."""
    if not port.valid:
        return Err(Error("the child read is not valid", code="value.unset_read"))
    try:
        return Ok(read(port))
    except HglError as failure:
        # A part of the child that is not valid, where the child is read
        # whole (see `unset_read`), is the same error.
        if failure.code != "value.unset_read":
            raise
        return Err(Error("a part of the child read is not valid", code="value.unset_read"))


def dict_replaced(output, value):
    """What a temporal dictionary publishes for a whole value of it,
    given its own output (TIME-7, TST-9: a T whole "replaces what the
    input held"): each entry of `value`, and the removal of each key the
    output holds that `value` has not. hgraph's `TSD` would otherwise
    keep those."""
    made = {key: _REMOVE for key in (output.keys() if output.valid else ()) if key not in value}
    made.update({key: published(entry) for key, entry in value.items()})
    return made


def whole_delta(value, keys, fixed=False, size=0, shape=None):
    """The delta that publishing `value` whole records, to a value that
    holds the keys or members `keys` (the language's TST-9: "a T whole,
    which replaces what the input held"; TST-16: "A T published whole is
    held as its delta too"), as hgraph records it and takes it back: a
    dictionary's is each entry, and the removal of each key it has not; a
    set's the members added and removed; a struct's each field, as
    hgraph's bundle takes it (see `bundled`); a tuple's, given its
    `shape`, each position, as hgraph's bundle takes it too, a null
    position a published null; a fixed list's, `fixed`, each position,
    as its `TSL` records it; and an unsized list's, to a list of `size`
    positions, each position, the tail past them removed, and the new
    size where it moves, as hgraph's unbounded `TSL` reads it (TYPE-16:
    "on a shrink names the tail positions it drops in `removed`")."""
    if shape is not None:
        return _bundled(value, shape, None)
    if isinstance(value, (set, frozenset)):
        added, removed = frozenset(value - keys), frozenset(keys - value)
        members = added | removed
        if not members:
            return frozenset()
        return _set_delta(added=added, removed=removed, tp=type(next(iter(members))))
    if isinstance(value, dict):
        made = {key: _REMOVE for key in keys if key not in value}
        made.update({key: published(entry) for key, entry in value.items()})
        return made
    if hasattr(value, "_hgl_fields"):
        return bundled(value)
    if isinstance(value, list) and fixed:
        return {position: published(entry) for position, entry in enumerate(value)}
    if isinstance(value, list):
        return _TSLDelta(
            dict(enumerate(value)),
            removed=tuple(range(len(value), size)),
            size_modified=None if len(value) == size else len(value),
        )
    return value


def list_size(size, delta):
    """How many positions an unsized list of `size` positions holds once
    `delta`, its delta, is published to it (TYPE-16), as `list_applied`
    counts them: a trace's cells are followed through so that a whole
    list removes the tail it has not (`whole_delta`)."""
    written = size_modified(delta)
    if written is None:
        changes = updated(delta)
        return max(size, max(changes) + 1) if changes else size
    return max(size + written, 0) if written < 0 else written


def updated(delta):
    """The property `updated` of a dictionary's delta (TYPE-16): each
    key added or changed, with its delta."""
    return {key: change for key, change in delta.items() if not _removes(change)}


def removed(delta):
    """The property `removed` of a dictionary's delta (TYPE-16): the set
    of each key removed."""
    return frozenset(key for key, change in delta.items() if _removes(change))


def only_value(published, struct):
    """What a temporal struct with no field, `struct`, publishes: its
    one value. A whole value of it and its delta, which has no property
    (the language's TYPE-16), publish it alike, so that a publication
    ticks, and the struct is valid once one has (TIME-9: "A published
    empty list or dictionary has no element and is valid")."""
    return published if isinstance(published, struct) else struct()


def unset_read(message="a part that has not published is read"):
    """The read of a part of a temporal struct or tuple that has not
    published, where a value of it is needed, or of a node's `out` before
    it holds a value (the language's TIME-7), which `message` says: it
    fails, with the code the language catalogues, `value.unset_read`
    ("Executed operation requires an unset ordinary payload"), and no
    value is made up. The compiler reads an optional part that has not
    published as null."""
    raise HglError(message, code="value.unset_read")


def bundled(value, shape=None, port=None):
    """What a temporal struct or tuple publishes for a whole value of it
    (TYPE-9: "Temporalization recursively maps tuples/structs to child
    fields"): each part, as hgraph's bundle takes it. A part that is
    null is published as a null, hgraph's `Null` (see `published`):
    `None` would leave the part as it was.

    With no `shape` the value is a struct whose fields each publish
    whole, every field its class has. Otherwise `shape` is what the
    compiler wrote for the value's type: `None` for a part that
    publishes whole; a `dict` of each field's shape for a struct, which
    names the fields of the type and not of the struct below it that a
    value of an abstract one is; a tuple of each position's for a tuple,
    which hgraph's bundle takes position by position; `[None]` for a
    fixed list of elements that publish whole, as a tuple of them; and
    `"list"` for an unsized list of them (see `_whole_list`). A set is
    given as the set it is, which hgraph's `TSS` takes as its members
    now, removing those it no longer has.

    `port` is the port of the output the value is published to, where
    the node has one: an unsized list in the value is made to tick
    through its own (`_whole_list`). A source's statements give
    `DEFERRED`, and its node publishes the value through its port when
    it is due (`published_into`)."""
    if shape is None:
        # Each field publishes whole.
        shape = dict.fromkeys(value._hgl_fields)
    return _bundled(value, shape, port)


def _bundled(value, shape, port):
    if shape is None:
        return published(value)
    if shape == "list":
        return _whole_list(value, port)
    if port is None or port is DEFERRED:
        within = lambda key: port  # noqa: E731
    else:
        within = lambda key: port[key]  # noqa: E731
    if isinstance(shape, dict):
        return {field: _bundled(getattr(value, field), part, within(field)) for field, part in shape.items()}
    if isinstance(shape, tuple):
        return tuple(
            _bundled(item, part, within(position(index)))
            for index, (item, part) in enumerate(zip(value, shape))
        )
    return tuple(_bundled(element, shape[0], within(index)) for index, element in enumerate(value))


def position(index):
    """The field of a temporal tuple's position `index`, `_0`, `_1`
    (the language's TYPE-9: tuples map "to child fields"), for an index
    that is a constant other than a literal, `pair[1 - 1]`."""
    return f"_{index}"


def positions(delta, shape):
    """hgraph's delta of a temporal value that holds a tuple, as the
    program reads it (the language's TYPE-16: a tuple's delta is
    "`tuple<delta<A>?, delta<B>?>`: each position's delta, null where it
    did not change").

    hgraph's bundle that a tuple is gives a `dict` of each position that
    changed, by the position's field, `_0`, `_1`. `shape` is what the
    compiler wrote for the type: a tuple of each position's shape for a
    tuple, a `dict` of the shape of each field that holds a tuple for a
    struct, whose delta is a `dict` of the fields that changed, and
    `None` for a part that holds no tuple. A null is a part that did not
    change; a published null, hgraph's `Null`, is kept."""
    if delta is None or shape is None:
        return delta
    if isinstance(shape, tuple):
        return tuple(positions(delta.get(position(index)), part) for index, part in enumerate(shape))
    return {field: positions(change, shape.get(field)) for field, change in delta.items()}


def item(sequence, index):
    """`sequence[index]`: the element of a list or a tuple at an `i64`
    index (EXPR-7). An index outside it fails: Python's own would count
    a negative one back from the end."""
    _index(index, len(sequence))
    return sequence[index]


def child(port, index):
    """The port of the position `index` of a fixed list's port, which a
    part of `out` is assigned through (the language's TIME-7). An index
    outside the list fails, as an element's does (EXPR-3: "An index
    outside a list fails where it is evaluated"); Python's own would
    count a negative one back from the end."""
    _index(index, len(port))
    return port[index]


def entry(dictionary, key):
    """`dictionary[key]`: the value a dictionary has for a key. The
    read is strict (ERR-6: "absence fails"), with the code
    `value.missing_key`."""
    _key(dictionary, key)
    return dictionary[key]


def published(value):
    """What publishes `value`, of a nullable type (the language's
    TIME-7): the value, or hgraph's `Null` where it is null.

    To hgraph a node that returns `None` publishes nothing, and one
    that returns `Null` publishes a value that is unset: valid,
    modified, and with no payload. That is HGL's published null.
    """
    return _Null if value is None else value


def unpublished(delta):
    """The `value` of a nullable's delta (the language's TYPE-16): what
    was published, "null for a published null". hgraph's delta of a
    published null is `Null`."""
    return None if delta is _Null else delta


class Source:
    """A source between two evaluations of its node (SOURCE-1).

    `body` is the generator that holds the source's statements: each
    `yield time: value` gives it a `(time, value)`. `pending` is the
    payload it retains for a target still to come, in a tuple of one,
    `target` that target, and `previous` the last target of any kind.
    """

    __slots__ = ("body", "pending", "target", "previous")

    def __init__(self, body):
        self.body = body
        self.pending = None
        self.target = None
        self.previous = None


def resume(source, now, scheduler, port, shape=None):
    """Resumes a source in the evaluation at `now`, and publishes what it
    is due there through `port`, its node's output (`None` for a source
    with no result), as `published_into` has it for `shape`.

    SOURCE-2: "A duration is relative to current evaluation time; a
    datetime is absolute. Negative duration fails. Resolve duration
    addition with checked arithmetic." SOURCE-3: "Targets strictly
    increase across the invocation, including skipped past targets. The
    first has no predecessor. A past absolute target is skipped; a
    current target publishes now; a future target retains its payload
    and suspends. On wake it publishes and resumes after yield."
    SOURCE-4: "A bare return or body completion finishes the source."

    Each publication is made before the statements after its `yield`
    run: where they fail, it stands (ERR-8: "Capture ends the failed pass
    without rollback ... Earlier writes remain visible"). A payload that
    is `None` publishes nothing, as a node that returns `None` does."""

    def publish(value):
        if port is not None and value is not None:
            port.value = published_into(port, value, shape)

    if source.pending is not None:
        if source.target != now:
            return
        (due,), source.pending = source.pending, None
        publish(due)
    while True:
        try:
            time, value = next(source.body)
        except StopIteration:
            return
        if isinstance(time, _datetime.timedelta):
            if time < _datetime.timedelta(0):
                raise HglError(
                    "a `yield` after a negative duration", code="yield.negative_duration"
                )
            target = _shifted(now, time)
        else:
            target = time
        if source.previous is not None and target <= source.previous:
            raise HglError(
                "a `yield` at a time that is not after the one before it",
                code="yield.non_increasing_time",
            )
        source.previous = target
        if target < now:
            continue
        if target == now:
            publish(value)
            continue
        source.pending = (value,)
        source.target = target
        scheduler.schedule(target)
        return


class Wake:
    """A node's scheduler for one evaluation (CAP-4, CAP-5).

    hgraph holds the wake it delivers until the node has run: in its own
    evaluation that wake is still its earliest, so `un_schedule` would
    cancel it, `is_scheduled` count it and `next_scheduled_time` give the
    current time. CAP-4 has a wake pending until it is delivered:
    `is_scheduled` is "True while a wake is pending". So each wake due
    now is taken as the node is entered, and `is_scheduled_now` keeps
    that one was: "whether a wake asked of the scheduler is due in the
    current evaluation", which asking again does not change. The rest is
    hgraph's scheduler, as it now stands."""

    def __init__(self, scheduler):
        self._scheduler = scheduler
        due = False
        # hgraph's `un_schedule()` with no tag takes its earliest wake,
        # and that wake's tag with it.
        while scheduler.is_scheduled_now:
            scheduler.un_schedule()
            due = True
        self.is_scheduled_now = due

    def __getattr__(self, name):
        return getattr(self._scheduler, name)


def _wake_delay(at, starting):
    """`at`, the duration a wake is asked of the scheduler or the alarm
    with, where the program runs (TIME-8): "A wake asked for with a
    `duration` takes one more than zero, but in `start`, which takes zero
    too ... A duration that is not admitted, a negative one in `start`
    included, ... elsewhere fails the call as the program runs." Core's
    CAP-5 holds the alarm to the same. `starting` is true where the call
    is written in `start`. The scheduler's and the alarm's `schedule`
    ask it once every argument of the call is evaluated (EXPR-2).

    hgraph 0.9.5 fails none of them: a zero or negative duration asked
    for in an evaluation, and a negative one in `start`, is a wake at a
    time no cycle can still run at, dropped with no failure."""
    if at < _datetime.timedelta(0) or (at == _datetime.timedelta(0) and not starting):
        raise HglError(
            "a wake asked for with a duration that is not more than zero"
            + (": a negative one" if starting else ", which only `start` takes zero for"),
            code="schedule.non_positive_duration",
        )
    return at


def _tag(tag):
    """CAP-4's `tag`, a `str?`, as hgraph's scheduler keeps it. hgraph
    keeps no wake under the empty tag, which is a tag as any other is:
    each is given a mark before it, and no tag stays none."""
    return None if tag is None else "#" + tag


# The tags an alarm's wake is kept under in hgraph's scheduler: the one
# pending wake, and one just asked for while its instant is read.
_ALARM = "_hgl_alarm"
_ASKED = "_hgl_asked"


def _alarm(scheduler, at):
    """CAP-5: "`schedule(alarm, at)` asks for a wake ... asking again
    keeps the earliest." A node that injects an alarm is given hgraph's
    scheduler, which keeps each wake asked for: the alarm's one wake is
    kept under a tag of its own, and is the earlier of the one pending
    and the one asked for.

    `at` is an instant, or a duration from the current evaluation time.
    hgraph says which instant a duration comes to where it is asked
    for: it is asked under a second tag, and the tag is taken back.
    hgraph keeps none at a time no cycle can still run at, the current
    evaluation time or before: that wake is not delivered, as a
    scheduler's is not, and the one pending stays. The alarm's wake that
    is due was taken as the node was entered (`Wake`).
    """
    scheduler.schedule(at, tag=_ASKED)
    asked = scheduler.pop_tag(_ASKED, None)
    if asked is None:
        return
    pending = scheduler.pop_tag(_ALARM, None)
    scheduler.schedule(asked if pending is None or asked < pending else pending, tag=_ALARM)


class Held:
    """A value given to hgraph to keep as it is.

    hgraph makes a value of its own of what it is given to keep, a
    node's configuration and what its global state holds, and has none
    for some of what an HGL value may be: a tuple that holds a set's
    delta, say. An object of a class it does not know is kept as the
    object it is, so a value is given to it inside one.
    """

    __slots__ = ("value",)

    def __init__(self, value):
        self.value = value


# The types of the values that hold no other: `copied` has nothing to
# copy in one.
_PLAIN = frozenset(
    (
        int,
        float,
        str,
        bool,
        bytes,
        type(None),
        _datetime.date,
        _datetime.datetime,
        _datetime.time,
        _datetime.timedelta,
    )
)


def copied(value):
    """A copy of `value` that shares no list, no dictionary and no
    struct that is changed where it is with it.

    A `var`, a `state` and a `cache` change the list, the dictionary and
    the struct that is no value (see `struct`) they hold where it is,
    and Python's is one object that every name of it sees change. So
    what is kept of a value that something else holds is a copy of it,
    made where it is kept: the compiler says where (the language's
    TYPE-5, "Retained results own their data; later mutation of a source
    cannot change an earlier retained value"). A struct that is a value,
    a tuple, a set and a result are never changed, and are made again
    only to hold the copies of what is inside them; a dictionary's keys
    and a set's members are copied as its values are, since a struct
    may be one.
    """
    kind = type(value)
    if kind in _PLAIN:
        return value
    if kind is list:
        # One pass that Python makes itself tells whether any entry may
        # hold what is changed where it is.
        if set(map(type, value)) <= _PLAIN:
            return value.copy()
        return [copied(part) for part in value]
    if kind is dict:
        if set(map(type, value)) <= _PLAIN:
            return {key: copied(part) for key, part in value.items()}
        return {copied(key): copied(part) for key, part in value.items()}
    if kind is tuple:
        return tuple(copied(part) for part in value)
    if kind is frozenset or kind is set:
        if set(map(type, value)) <= _PLAIN:
            return value
        return kind(copied(part) for part in value)
    if kind is Ok:
        return Ok(copied(value.value))
    if kind is Err:
        return Err(copied(value.error))
    fields = getattr(kind, "_hgl_fields", None)
    if fields is not None:
        if kind._hgl_by_value:
            return value
        return kind(**{field: copied(getattr(value, field)) for field in fields})
    return value


def _step(holder, kind, key):
    """What one step of a place's way in leads to inside `holder`: the
    field `key` of a struct (`"f"`), the entry `key` of a dictionary
    (`"k"`), or the element at `key` of a list or a tuple (`"i"`). A key
    the dictionary has not and an index outside the sequence fail."""
    if kind == "f":
        return getattr(holder, key)
    if kind == "k":
        _key(holder, key)
    else:
        _index(key, len(holder))
    return holder[key]


def _spliced(sequence, index, value):
    """`sequence`, a list or a tuple, with the element at `index` another
    value: a new sequence of its type, and `sequence` as it was."""
    return sequence[:index] + type(sequence)((value,)) + sequence[index + 1 :]


def _put(holder, kind, key, value):
    """What `holder` is to be with the step `kind`, `key` (see `_step`)
    holding `value`. A struct that is a value and a tuple are never
    changed: a new one is made (`set_field`, `_spliced`). A list, a
    dictionary and a struct that is no value are changed where they are,
    and given back."""
    if kind == "f":
        return set_field(holder, key, value)
    if type(holder) is tuple:
        return _spliced(holder, key, value)
    holder[key] = value
    return holder


def _changed_inside(holder, steps, at, change):
    """What `holder` is to be once the value `steps[at:]` leads to inside
    it has been changed by `change`.

    `holder` is held by its place, or by a value that is. A list, a
    dictionary and a struct that is no value (see `struct`) on the way
    are changed where they are; a struct that is a value and a tuple are
    never changed, and a new one is made where a part of it has another
    value (`set_field`). A step that fails,
    an index outside a list or a key a dictionary has not, fails before
    anything is changed.
    """
    if at == len(steps):
        return change(holder)
    kind, key = steps[at]
    inner = _step(holder, kind, key)
    changed = _changed_inside(inner, steps, at + 1, change)
    return holder if changed is inner else _put(holder, kind, key, changed)


def _read_in(holder, steps):
    """What `steps` leads to inside `holder`, read and not changed. A
    step that fails, an index outside a list or a key a dictionary has
    not, fails here, as it does where a place is read."""
    for kind, key in steps:
        holder = _step(holder, kind, key)
    return holder


def set_at(root, steps, value, /):
    """`place = value` where the place is inside the value a `var`, a
    `state` or a `cache` holds, more than one step in: `book.bids[price]
    = size`, `rows[key][0] = 2` (EXPR-3). Each collection on the way is
    changed where it is (`_changed_inside`).

    `steps` is the way in, each step its kind and what it names: `"f"`
    a struct's field, `"k"` a dictionary's key, and `"i"` a list's index
    or a tuple's position. The last step is the place assigned: "A key
    the dictionary has not is added." A struct on the way is changed
    where it is, but for one that is a value (`set_field`).

    `value` gives the value when it is called. It is called once the
    way in has been read, so that a step that fails does so before the
    value is evaluated, as in any assignment; what it gives is kept.
    Only then is anything changed: the value may be, or hold, a
    collection on the way in, `tree.kids[0].kids[0] = tree`, of which
    the compiler gives a copy, taken before the change.
    """
    kind, key = steps[-1]
    _read_in(root, steps[:-1])
    new = value()

    def assign(holder):
        # The place assigned is not read: a key the dictionary has not is
        # added, and only an index is checked.
        if kind == "i":
            _index(key, len(holder))
        return _put(holder, kind, key, new)

    return _changed_inside(root, steps[:-1], 0, assign)


def given_at(root, steps, function, place, arguments, /):
    """A call of `function`, a native function of core with one `var`
    parameter, named `place`, that is given a place inside the value
    `root`: `append(book.orders, order)` (the language's FN-15).

    The function is given the value `steps` leads to, which it changes
    where it is. `arguments` gives its other arguments, by name, when
    it is called: once the way in has been read, as they are written
    after the place. Only then is anything changed: an argument may
    be, or hold, a collection on the way in, `append(tree.kids[0].kids,
    tree)`, of which the compiler gives a copy, taken before the
    change. This gives the function's result and then what `root` is
    to be.
    """
    _read_in(root, steps)
    others = arguments()
    result = []

    def change(target):
        given = function(**{place: target}, **others)
        result.append(given[0])
        return given[1]

    changed = _changed_inside(root, steps, 0, change)
    return (result[0], changed)


def set_item(sequence, index, value, /):
    """`place[index] = value` where the place is an element of the list
    a `var`, a `state` or a `cache` holds itself (EXPR-3). The list is
    changed where it is, and keeps `value`. "An index outside a list
    fails where it is evaluated", and the list is then as it was."""
    _index(index, len(sequence))
    sequence[index] = value
    return sequence


def set_entry(dictionary, key, value, /):
    """`place[key] = value` where the place is an entry of the
    dictionary a `var`, a `state` or a `cache` holds itself (EXPR-3).
    "A key the dictionary has not is added." The dictionary is changed
    where it is."""
    dictionary[key] = value
    return dictionary


def with_item(sequence, index, value, /):
    """`place[index] = value` for a tuple, and for a list that is no
    place's own (EXPR-3): the sequence with one element another value.
    "An index outside a list fails where it is evaluated." `sequence`
    itself is as it was, for whatever else holds it."""
    _index(index, len(sequence))
    return _spliced(sequence, index, value)


def with_entry(dictionary, key, value, /):
    """`place[key] = value` for a dictionary that is no place's own
    (EXPR-3): the dictionary with the key given the value. "A key the
    dictionary has not is added." `dictionary` itself is as it was."""
    return {**dictionary, key: value}


def unmatched(selected):
    """A `switch` with no `default`, where no case has the selector's
    value (CTRL-6: "An unmatched selector uses default or fails")."""
    raise HglError(f"no case of the `switch` has the value {selected!r}")


# What `nullable` has made: a node for each Python type of `T`.
_NULLABLE = {}


def nullable(port, scalar):
    """A connection of `T` where one of `T?` is wanted, as hgraph's
    `TS[T | None]` (the language's TYPE-14: "a T is a value where a T?
    is expected"). hgraph's `switch_` takes branches of one type, and a
    connection of `T` is its `TS[T]`. `port` is given as it is where it
    is one of `T?` already; otherwise a node publishes each value it
    publishes, and is invalid where it is (END-4). `scalar` is the
    Python type of `T`."""
    if port.output_type == _TS[scalar | None]:
        return port
    node = _NULLABLE.get(scalar)
    if node is None:

        @_compute_node(valid=())
        def node(value: _TS[scalar], _output: _TS_OUT[scalar | None] = None) -> _TS[scalar | None]:
            if value.valid:
                return value.value
            _output.invalidate()

        _NULLABLE[scalar] = node
    return node(value=port)


def _shifted(instant, duration):
    """An instant a duration on from another (OPS-1).

    OPS-10: an instant result "outside its type's range is an HGL error
    with code `numeric.out_of_range`". The range is TYPE-2's calendar
    range, the years 1 to 9999, which is the range of Python's own
    `datetime`: Python says when a result leaves it.
    """
    try:
        return instant + duration
    except OverflowError:
        raise HglError(
            "an instant result is outside the years 1 to 9999", code="numeric.out_of_range"
        ) from None
