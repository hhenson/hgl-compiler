"""Core's native value functions, for the hgraph target.

Written by the hgl compiler beside the modules it generates. Each function
is one native candidate of core's `hgl/native/scalar_operators.hgl`, named
after the candidate and the types of its parameters: `add__i64_i64` is
`native impl const fn add_(lhs: i64, rhs: i64) -> i64`. A conversion is
named with what it gives too, `convert_i64_to_f64`: core's candidates of
`convert` take one type and give different ones (OPS-4).

A conversion that can fail takes `invalid`, the name of a member of core's
`InvalidPolicy` (OPS-8): under `"error"` it gives a `result`, an `Err`
carrying the code `convert.invalid` for a failure, and under `"fail"` the
plain value, and a failure is an error raised.

Core's numeric contract has `i64` arithmetic neither wrap nor promote: a
result outside the signed 64-bit range is an error with the code
`numeric.out_of_range` (NUM-2).

A `duration` is a `timedelta`, and its arithmetic is done on its count of
whole microseconds, which is held to the same range (TYPE-2, OPS-10).

A division takes `zero`, the name of a member of core's `ZeroPolicy`,
which says what a zero divisor gives (NUM-3, NUM-12): `"ieee"` what IEEE
arithmetic does, `"nan"` a NaN, and `"fail"` an error with the code
`numeric.division_by_zero`. Under `"error"` a division gives a `result`
(NUM-9): an `Err` for a zero divisor, and an `Ok` of the quotient
otherwise.

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
from fractions import Fraction as _Fraction

from hgraph import (
    EvaluationClock as _EvaluationClock,
    NodeError as _NodeError,
    Null as _Null,
    REMOVE as _REMOVE,
    REMOVE_IF_EXISTS as _REMOVE_IF_EXISTS,
    K as _K,
    SCALAR as _SCALAR,
    TS as _TS,
    TSD as _TSD,
    TimeSeriesSchema as _TimeSeriesSchema,
    compute_node as _compute_node,
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


def _equal(lhs, rhs):
    """`lhs == rhs` for two values of one type, part by part.

    Python's own `==` on a list, a tuple or a dictionary takes two parts
    that are one object to be equal and does not compare them: a NaN
    held in both would then equal itself (NUM-5: "NaN is neither equal
    nor close to any value, including itself"), and whether two values
    are equal would turn on whether one was retained. Each part is
    compared here as its own type compares.
    """
    if isinstance(lhs, (list, tuple)):
        return (
            isinstance(rhs, (list, tuple))
            and len(lhs) == len(rhs)
            and all(_equal(left, right) for left, right in zip(lhs, rhs))
        )
    if isinstance(lhs, dict):
        return (
            isinstance(rhs, dict)
            and lhs.keys() == rhs.keys()
            and all(_equal(value, rhs[key]) for key, value in lhs.items())
        )
    return lhs == rhs


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


class ErrorContext:
    """The provenance of an error a graph failure gave (ERR-4): "`.node:
    str`, `.phase: str`, one of `"evaluate"`, `"start"`, and `"stop"`,
    and `.evaluation_time: datetime`"."""

    __slots__ = ("node", "phase", "evaluation_time")

    def __init__(self, node, phase, evaluation_time):
        self.node = node
        self.phase = phase
        self.evaluation_time = evaluation_time


def error_of(failure, evaluation_time):
    """The `error` a capture gives for hgraph's `NodeError` `failure`,
    met at `evaluation_time` (ERR-4, ERR-7).

    An `HglError` says its error in the last note of the traceback (see
    `HglError`). "On first graph failure, attach original node identity,
    phase, and evaluation time; propagation/capture preserve them": an
    error with no context of its own is given the failing node's, in
    the phase hgraph captures, evaluation. Any other failure is an error
    of the traceback's last line, with no code.
    """
    lines = failure.error_msg.rstrip().splitlines()
    written = next((line for line in reversed(lines) if line.startswith(_NOTE)), None)
    if written is None:
        return Error(lines[-1] if lines else "", None, ErrorContext(failure.label, "evaluate", evaluation_time))
    written = _json.loads(written[len(_NOTE):])
    context = written.get("context")
    if context is None:
        context = ErrorContext(failure.label, "evaluate", evaluation_time)
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
    raise HglError(result.error.message, code=result.error.code, context=result.error.context)


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
        made = type(f"{struct.__name__}<{arguments}>", (struct,), {"__slots__": ()})
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
    annotation of the struct names one type to hgraph.
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
    of each removed with `REMOVE`. A property left out is empty. What is
    updated is copied: the program may go on changing the dictionary it
    was given in."""
    made = {} if updated is None else copied(updated)
    for key in removed:
        made[key] = _REMOVE
    return made


def _removes(change):
    return change is _REMOVE or change is _REMOVE_IF_EXISTS


@_compute_node
def entry_of(dictionary: _TSD[_K, _TS[_SCALAR]], key: _TS[_K]) -> _TS[_SCALAR]:
    """`dictionary[key]` in a graph: the child of a temporal dictionary
    that `key` selects (the language's TYPE-9, "dictionaries to keyed
    children"). The read is strict (ERR-6: "absence fails"), with the
    code `value.missing_key`, where hgraph's own selection would be
    silent. It publishes where the key ticks or the selected child
    does, so another child's tick is silent."""
    selected = key.value
    child = dictionary[selected] if selected in dictionary.keys() else None
    if child is None or not child.valid:
        raise HglError(f"the dictionary has no key {selected!r}", code="value.missing_key")
    if key.modified or child.modified:
        # A child that holds a null publishes it (see `published`).
        return published(child.value)


def updated(delta):
    """The property `updated` of a dictionary's delta (TYPE-16): each
    key added or changed, with its delta."""
    return {key: change for key, change in delta.items() if not _removes(change)}


def removed(delta):
    """The property `removed` of a dictionary's delta (TYPE-16): the set
    of each key removed."""
    return frozenset(key for key, change in delta.items() if _removes(change))


def bundled(value):
    """What a temporal struct publishes for a whole value of it (TYPE-9):
    each field, as hgraph's bundle takes it. The compiler gives it only
    a struct whose fields each publish whole. A field that is null is
    published as a null, hgraph's `Null` (see `published`): `None` would
    leave the field as it was."""
    return {field: published(getattr(value, field)) for field in value._hgl_fields}


def item(sequence, index):
    """`sequence[index]`: the element of a list or a tuple at an `i64`
    index (EXPR-7). An index outside it fails: Python's own would count
    a negative one back from the end."""
    if not 0 <= index < len(sequence):
        raise HglError(f"the index {index} is outside a sequence of {len(sequence)}")
    return sequence[index]


def entry(dictionary, key):
    """`dictionary[key]`: the value a dictionary has for a key. The
    read is strict (ERR-6: "absence fails"), with the code
    `value.missing_key`."""
    try:
        return dictionary[key]
    except KeyError:
        raise HglError(f"the dictionary has no key {key!r}", code="value.missing_key") from None


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


def resume(source, now, scheduler):
    """Resumes a source in the evaluation at `now`: what it publishes
    there, in a tuple of one, or `None` where it publishes nothing.

    SOURCE-2: "A duration is relative to current evaluation time; a
    datetime is absolute. Negative duration fails. Resolve duration
    addition with checked arithmetic." SOURCE-3: "Targets strictly
    increase across the invocation, including skipped past targets. The
    first has no predecessor. A past absolute target is skipped; a
    current target publishes now; a future target retains its payload
    and suspends. On wake it publishes and resumes after yield."
    SOURCE-4: "A bare return or body completion finishes the source."
    """
    published = None
    if source.pending is not None:
        if source.target != now:
            return None
        published, source.pending = source.pending, None
    while True:
        try:
            time, value = next(source.body)
        except StopIteration:
            return published
        if isinstance(time, _datetime.timedelta):
            if time < _datetime.timedelta(0):
                raise HglError(
                    "a `yield` after a negative duration", code="yield.negative_duration"
                )
            target = _moved(now, time)
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
            published = (value,)
            continue
        source.pending = (value,)
        source.target = target
        scheduler.schedule(target)
        return published


def Clock_evaluation_time(clock):
    """CAP-3: a clock's four properties, each read from hgraph's
    `EvaluationClock`, which has them by the same names. An instant is
    a `datetime` in UTC that names no zone, as hgraph has them."""
    return clock.evaluation_time


def Clock_now(clock):
    return clock.now


def Clock_cycle_time(clock):
    return clock.cycle_time


def Clock_next_cycle_evaluation_time(clock):
    return clock.next_cycle_evaluation_time


# CAP-2: core's `LogLevel` members, each with the level Python's
# `logging` has for it, lowest first.
_LOG_LEVELS = (("debug", 10), ("info", 20), ("warning", 30), ("error", 40))


def Logger_level(logger):
    """CAP-2: "`logger.level` is the logger's current level". The name
    of the `LogLevel` member that a record must be at to be written: a
    level above `error` writes none below it, and is `error`."""
    level = logger.getEffectiveLevel()
    for member, number in _LOG_LEVELS:
        if level <= number:
            return member
    return "error"


def fn_log(logger, level, message):
    """CAP-2: `log(logger, level, message)` "writes one record of
    `message` at `level`". The message is given as an argument of a
    fixed format, so that a `%` in it is text."""
    logger.log(dict(_LOG_LEVELS)[level.name], "%s", message)


def Scheduler_next_scheduled_time(scheduler):
    """CAP-4: "The time of the earliest pending wake; null with none".
    hgraph gives its earliest time where none is pending."""
    return scheduler.next_scheduled_time if scheduler.is_scheduled else None


def Scheduler_is_scheduled(scheduler):
    """CAP-4: "True while a wake is pending"."""
    return scheduler.is_scheduled


def Scheduler_is_scheduled_now(scheduler):
    """CAP-4: "whether a wake asked of the scheduler is due in the
    current evaluation, whether or not an input has ticked in it"."""
    return scheduler.is_scheduled_now


def schedule_Scheduler_duration_str__bool(capability, at, tag, on_wall_clock):
    """CAP-4: "With a `duration`, a wake that long after the current
    evaluation time". hgraph counts a `timedelta` from then, and "a wake
    asked for under a tag replaces the pending wake of that tag"."""
    capability.schedule(at, tag=tag, on_wall_clock=on_wall_clock)


def schedule_Scheduler_datetime_str__bool(capability, at, tag, on_wall_clock):
    """CAP-4: "with a `datetime`, a wake then"."""
    capability.schedule(at, tag=tag, on_wall_clock=on_wall_clock)


def fn_un_schedule(scheduler, tag):
    """CAP-4: "`un_schedule(scheduler)` and `un_schedule(scheduler, tag:
    name)` cancel a pending wake": the earliest, or the one asked for
    under `name`."""
    if tag is None:
        scheduler.un_schedule()
    else:
        scheduler.un_schedule(tag)


def fn_reset(scheduler):
    """CAP-4: "`reset(scheduler)` every one"."""
    scheduler.reset()


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
    """
    # In the evaluation a wake is due in, hgraph still holds it: it is
    # the alarm's own wake, taken now, and no longer one that is
    # pending.
    due = scheduler.next_scheduled_time if scheduler.is_scheduled_now else None
    scheduler.schedule(at, tag=_ASKED)
    asked = scheduler.pop_tag(_ASKED)
    pending = scheduler.pop_tag(_ALARM, None)
    if pending is not None and pending == due:
        pending = None
    scheduler.schedule(asked if pending is None or asked < pending else pending, tag=_ALARM)


def schedule_Alarm_duration(capability, at):
    _alarm(capability, at)


def schedule_Alarm_datetime(capability, at):
    _alarm(capability, at)


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


def get_GlobalState_str(store, key):
    """CAP-6: the value under `key`; "an absent key fails the node"."""
    try:
        return store[key].value
    except KeyError:
        raise HglError(f"the global store holds nothing under {key!r}") from None


def has_GlobalState_str(store, key):
    """CAP-6: "whether the key holds a value"."""
    return key in store


def put_GlobalState_str_V(store, key, value):
    """CAP-6: "puts one there". It is kept as it is (`Held`). The store
    keeps the value, and is given a copy (`copied`): a place it was
    read from goes on changing, and that does not reach the store."""
    store[key] = Held(copied(value))


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


def _changed(holder, steps, at, change):
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
    if kind == "f":
        inner = getattr(holder, key)
        changed = _changed(inner, steps, at + 1, change)
        return holder if changed is inner else set_field(holder, key, changed)
    if kind == "k":
        if key not in holder:
            raise HglError(f"the dictionary has no key {key!r}", code="value.missing_key")
    else:
        if not 0 <= key < len(holder):
            raise HglError(f"the index {key} is outside a sequence of {len(holder)}")
        if type(holder) is tuple:
            inner = holder[key]
            changed = _changed(inner, steps, at + 1, change)
            return holder if changed is inner else holder[:key] + (changed,) + holder[key + 1 :]
    inner = holder[key]
    changed = _changed(inner, steps, at + 1, change)
    if changed is not inner:
        holder[key] = changed
    return holder


def _read_in(holder, steps):
    """What `steps` leads to inside `holder`, read and not changed. A
    step that fails, an index outside a list or a key a dictionary has
    not, fails here, as it does where a place is read."""
    for kind, key in steps:
        if kind == "f":
            holder = getattr(holder, key)
        elif kind == "k":
            if key not in holder:
                raise HglError(f"the dictionary has no key {key!r}", code="value.missing_key")
            holder = holder[key]
        else:
            if not 0 <= key < len(holder):
                raise HglError(f"the index {key} is outside a sequence of {len(holder)}")
            holder = holder[key]
    return holder


def set_at(root, steps, value, /):
    """`place = value` where the place is inside the value a `var`, a
    `state` or a `cache` holds, more than one step in: `book.bids[price]
    = size`, `rows[key][0] = 2` (EXPR-3). Each collection on the way is
    changed where it is (`_changed`).

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
        if kind == "f":
            return set_field(holder, key, new)
        if kind == "k":
            holder[key] = new
            return holder
        if not 0 <= key < len(holder):
            raise HglError(f"the index {key} is outside a sequence of {len(holder)}")
        if type(holder) is tuple:
            return holder[:key] + (new,) + holder[key + 1 :]
        holder[key] = new
        return holder

    return _changed(root, steps[:-1], 0, assign)


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

    changed = _changed(root, steps, 0, change)
    return (result[0], changed)


def set_item(sequence, index, value, /):
    """`place[index] = value` where the place is an element of the list
    a `var`, a `state` or a `cache` holds itself (EXPR-3). The list is
    changed where it is, and keeps `value`. "An index outside a list
    fails where it is evaluated", and the list is then as it was."""
    if not 0 <= index < len(sequence):
        raise HglError(f"the index {index} is outside a sequence of {len(sequence)}")
    sequence[index] = value
    return sequence


def set_entry(dictionary, key, value, /):
    """`place[key] = value` where the place is an entry of the
    dictionary a `var`, a `state` or a `cache` holds itself (EXPR-3).
    "A key the dictionary has not is added." The dictionary is changed
    where it is."""
    dictionary[key] = value
    return dictionary


def fn_append(values, item):
    """LIST-1: `append(values, item)` "gives the place `values` the list
    it held with `item` after its last entry".

    `values` is a `var` parameter (the language's FN-15), so this gives
    back the list the place is to hold, after a result it has none of.
    The list grows where it is (LIST-3), and keeps `item`: the compiler
    gives a copy of one that something else holds.
    """
    values.append(item)
    return (None, values)


def fn_extend(values, items):
    """LIST-2: "each entry of `items` after its last entry, in the
    order `items` has them". The list grows as `fn_append` grows it,
    and keeps each entry."""
    values.extend(items)
    return (None, values)


def get_dict_K__T__K(store, key):
    """Core's `get<K, T>(store: dict<K, T>, key: K) -> T?`: the value a
    dictionary has for a key, or null where it has none (ERR-6). What
    it gives is still in the dictionary: it is read there, and what
    keeps it takes a copy."""
    return store.get(key)


def with_item(sequence, index, value, /):
    """`place[index] = value` for a tuple, and for a list that is no
    place's own (EXPR-3): the sequence with one element another value.
    "An index outside a list fails where it is evaluated." `sequence`
    itself is as it was, for whatever else holds it."""
    if not 0 <= index < len(sequence):
        raise HglError(f"the index {index} is outside a sequence of {len(sequence)}")
    return sequence[:index] + type(sequence)((value,)) + sequence[index + 1 :]


def with_entry(dictionary, key, value, /):
    """`place[key] = value` for a dictionary that is no place's own
    (EXPR-3): the dictionary with the key given the value. "A key the
    dictionary has not is added." `dictionary` itself is as it was."""
    return {**dictionary, key: value}


def unmatched(selected):
    """A `switch` with no `default`, where no case has the selector's
    value (CTRL-6: "An unmatched selector uses default or fails")."""
    raise HglError(f"no case of the `switch` has the value {selected!r}")


def _i64(value):
    if not _I64_MIN <= value <= _I64_MAX:
        raise HglError("an i64 result is outside the signed 64-bit range", code="numeric.out_of_range")
    return value


def add__i64_i64(lhs, rhs):
    return _i64(lhs + rhs)


def add__f64_f64(lhs, rhs):
    return lhs + rhs


def add__i64_f64(lhs, rhs):
    return float(lhs) + rhs


def add__str_str(lhs, rhs):
    return lhs + rhs


def sub__i64_i64(lhs, rhs):
    return _i64(lhs - rhs)


def sub__f64_f64(lhs, rhs):
    return lhs - rhs


def sub__i64_f64(lhs, rhs):
    return float(lhs) - rhs


def sub__f64_i64(lhs, rhs):
    return lhs - float(rhs)


def mul__i64_i64(lhs, rhs):
    return _i64(lhs * rhs)


def mul__f64_f64(lhs, rhs):
    return lhs * rhs


def mul__i64_f64(lhs, rhs):
    return float(lhs) * rhs


def _zero_divisor(lhs, rhs, zero):
    """What a division by the zero `rhs` gives, when it gives a number.

    NUM-3: under `ieee`, "a finite nonzero numerator divided by signed
    zero produces signed infinity; zero divided by zero produces NaN.
    The infinity sign combines both operand signs." Under `nan` every
    zero divisor gives NaN, and under `fail` it is an error.
    """
    if zero == "fail":
        raise HglError("a division by zero", code="numeric.division_by_zero")
    if zero == "nan" or lhs == 0 or lhs != lhs:
        return _NAN
    if zero != "ieee":
        raise HglError(f"the zero policy `{zero}` is not provided")
    sign = _math.copysign(1.0, lhs) * _math.copysign(1.0, rhs)
    return _math.copysign(_INFINITY, sign)


def _div(lhs, rhs, zero):
    """`/` of numbers, which gives an `f64` (NUM-1)."""
    if rhs == 0:
        return _zero_divisor(lhs, rhs, zero)
    return lhs / rhs


def _floordiv(lhs, rhs, zero):
    """`//` with an `f64` operand.

    NUM-12: "Under `.ieee` a floor division or a modulo with an `f64`
    operand is what IEEE arithmetic makes of the zero divisor": the
    quotient is an infinity or NaN, and so is its floor.
    """
    if rhs == 0:
        return _zero_divisor(lhs, rhs, zero)
    # NUM-1: "Where an f64 operand is an infinity or NaN and the divisor
    # is not zero, `//` is the floor of the IEEE quotient". Python's own
    # `//` is the floor of the exact quotient: NaN for an infinite
    # numerator, and -1.0 for `-1.0 // inf`.
    if not (_math.isfinite(lhs) and _math.isfinite(rhs)):
        quotient = lhs / rhs
        # An infinity and NaN are their own floor.
        if not _math.isfinite(quotient):
            return quotient
        # A finite quotient here is a zero, a finite numerator over an
        # infinity, and its floor keeps its sign.
        return _math.copysign(float(_math.floor(quotient)), quotient)
    return float(lhs // rhs)


def _mod(lhs, rhs, zero):
    """`%` with an `f64` operand: NaN for a zero divisor under `ieee`."""
    if rhs == 0:
        if zero == "fail":
            raise HglError("a division by zero", code="numeric.division_by_zero")
        if zero not in ("ieee", "nan"):
            raise HglError(f"the zero policy `{zero}` is not provided")
        return _NAN
    # NUM-1: there `%` is "what IEEE `fmod` gives: a remainder by an
    # infinity is the numerator as it is, whatever its sign". Of an
    # infinite numerator, and with a NaN operand, it is NaN, where
    # Python's `fmod` raises and its `%` has the divisor's sign.
    if not (_math.isfinite(lhs) and _math.isfinite(rhs)):
        if lhs != lhs or rhs != rhs or _math.isinf(lhs):
            return _NAN
        return float(lhs)
    return float(lhs % rhs)


def _whole(zero):
    """A division of two `i64` gives an `i64`, which holds no NaN or
    infinity: NUM-12 admits `fail` there, and `error`."""
    if zero != "fail":
        raise HglError(f"the zero policy `{zero}` is not provided")
    raise HglError("a division by zero", code="numeric.division_by_zero")


def _floordiv_i64(lhs, rhs, zero):
    # NUM-1: "Integer floor division rounds toward negative infinity",
    # as Python's does.
    if rhs == 0:
        _whole(zero)
    return _i64(lhs // rhs)


def _mod_i64(lhs, rhs, zero):
    # NUM-1: "modulo has the divisor's sign when nonzero", as Python's
    # does.
    if rhs == 0:
        _whole(zero)
    return lhs % rhs


def _divide(ordinary, lhs, rhs, zero):
    """A division under its zero policy.

    NUM-9: under `error`, "either signed zero divisor produces
    `err(...)` carrying error code `numeric.division_by_zero`; every
    nonzero divisor produces `ok(value)` from the ordinary division."
    """
    if zero != "error":
        return ordinary(lhs, rhs, zero)
    if rhs == 0:
        return Err(Error("a division by zero", code="numeric.division_by_zero"))
    # The divisor is not zero, so no policy is consulted.
    return Ok(ordinary(lhs, rhs, "fail"))


def div__i64_i64(lhs, rhs, zero):
    return _divide(_div, lhs, rhs, zero)


def div__f64_f64(lhs, rhs, zero):
    return _divide(_div, lhs, rhs, zero)


def div__i64_f64(lhs, rhs, zero):
    return _divide(_div, lhs, rhs, zero)


def div__f64_i64(lhs, rhs, zero):
    return _divide(_div, lhs, rhs, zero)


def floordiv__i64_i64(lhs, rhs, zero):
    return _divide(_floordiv_i64, lhs, rhs, zero)


def floordiv__f64_f64(lhs, rhs, zero):
    return _divide(_floordiv, lhs, rhs, zero)


def floordiv__i64_f64(lhs, rhs, zero):
    return _divide(_floordiv, lhs, rhs, zero)


def floordiv__f64_i64(lhs, rhs, zero):
    return _divide(_floordiv, lhs, rhs, zero)


def mod__i64_i64(lhs, rhs, zero):
    return _divide(_mod_i64, lhs, rhs, zero)


def mod__f64_f64(lhs, rhs, zero):
    return _divide(_mod, lhs, rhs, zero)


def mod__i64_f64(lhs, rhs, zero):
    return _divide(_mod, lhs, rhs, zero)


def mod__f64_i64(lhs, rhs, zero):
    return _divide(_mod, lhs, rhs, zero)


def _microseconds(value):
    """The whole microseconds of a duration, which is a `timedelta`."""
    return value // _MICROSECOND


def _duration(microseconds):
    """The duration of `microseconds`.

    TYPE-2: "Instants/durations use signed 64-bit microseconds". A
    `timedelta` holds more, so the range is checked here, on the count.
    OPS-10: a duration result "outside its type's range is an HGL error
    with code `numeric.out_of_range`, as an `i64` result is".
    """
    if not _I64_MIN <= microseconds <= _I64_MAX:
        raise HglError(
            "a duration result is outside the signed 64-bit range of microseconds",
            code="numeric.out_of_range",
        )
    return _datetime.timedelta(microseconds=microseconds)


def add__duration_duration(lhs, rhs):
    return _duration(_microseconds(lhs) + _microseconds(rhs))


def sub__duration_duration(lhs, rhs):
    return _duration(_microseconds(lhs) - _microseconds(rhs))


def mul__duration_i64(lhs, rhs):
    return _duration(_microseconds(lhs) * rhs)


def mul__duration_f64(lhs, rhs):
    """OPS-10: "the exact result rounded to the nearest duration the type
    holds, a tie to the even one", and a duration "multiplied ... by a NaN
    or an infinity" is out of range. The product is exact as a fraction,
    and Python rounds one half to even."""
    if not _math.isfinite(rhs):
        raise HglError(
            "a duration multiplied by an infinity or a NaN", code="numeric.out_of_range"
        )
    return _duration(round(_Fraction(_microseconds(lhs)) * _Fraction(rhs)))


def neg__duration(value):
    return _duration(-_microseconds(value))


def _div_duration(lhs, rhs, zero):
    """A duration divided by a number (OPS-1).

    OPS-10: "the exact result rounded to the nearest duration the type
    holds, a tie to the even one", and a duration "divided by a NaN or an
    infinity" is out of range. The quotient is exact as a fraction, and
    Python rounds one half to even. NUM-12: every division takes `zero`,
    and these candidates admit `fail`, and `error` through `_divide`.
    """
    if rhs == 0:
        _whole(zero)
    if isinstance(rhs, float) and not _math.isfinite(rhs):
        raise HglError(
            "a duration divided by an infinity or a NaN", code="numeric.out_of_range"
        )
    return _duration(round(_Fraction(_microseconds(lhs)) / _Fraction(rhs)))


def div__duration_i64(lhs, rhs, zero):
    return _divide(_div_duration, lhs, rhs, zero)


def div__duration_f64(lhs, rhs, zero):
    return _divide(_div_duration, lhs, rhs, zero)


def div__duration_duration(lhs, rhs, zero):
    """OPS-1: a duration by a duration is an `f64`, the quotient of
    their counts, under the zero policies a quotient of numbers has."""
    return _divide(_div, _microseconds(lhs), _microseconds(rhs), zero)


def floordiv__duration_duration(lhs, rhs, zero):
    """OPS-1: `//` of two durations is an `i64`, the floor of the
    quotient of their counts (NUM-1)."""
    return _divide(_floordiv_i64, _microseconds(lhs), _microseconds(rhs), zero)


# ---- Conversions ----
#
# OPS-4: "`convert<T>(value)` converts a value to the type T." Core's
# candidates of it take the same type and give different ones, so each
# function here is named with what it gives too: `convert_i64_to_f64`
# is `native impl const fn convert(value: i64) -> f64`.
#
# OPS-6 says what each yields.


def convert_i64_to_f64(value):
    # "The nearest `f64`": Python rounds an `int` to the nearest
    # `float`, a tie to the even one.
    return float(value)


def convert_i64_to_bool(value):
    """OPS-6: "False for zero, true otherwise"."""
    return value != 0


def convert_f64_to_bool(value):
    """OPS-6: "False for zero, true otherwise; NaN is true". A NaN is
    unequal to zero, as to everything."""
    return value != 0


def convert_bool_to_i64(value):
    """OPS-6: "0 or 1"."""
    return 1 if value else 0


def convert_bool_to_f64(value):
    return 1.0 if value else 0.0


def convert_bool_to_str(value):
    """OPS-6: "The text of the literal that denotes the value"."""
    return "true" if value else "false"


def convert_str_to_bytes(value):
    """OPS-6: UTF-8."""
    return value.encode("utf-8")


def convert_date_to_datetime(value):
    """OPS-6: "The start of that date, in UTC". An instant here is a
    `datetime` in UTC that names no zone."""
    return _datetime.datetime(value.year, value.month, value.day)


def convert_datetime_to_date(value):
    """OPS-6: "The date of the instant, in UTC"."""
    return value.date()


def _unconverted(message, invalid):
    """A conversion that failed, under its policy (OPS-8).

    Under `"error"` "a failure is an Err carrying error code
    `convert.invalid`", and under `"fail"` "an escaping HGL error":
    the same failure, raised.
    """
    if invalid == "error":
        return Err(Error(message, code="convert.invalid"))
    if invalid == "fail":
        raise HglError(message, code="convert.invalid")
    raise HglError(f"the invalid policy `{invalid}` is not provided")


def _converted(value, invalid):
    """A conversion that can fail and did not: "the conversion then
    returns `result<T, error>`" under `"error"`, and T under `"fail"`
    (OPS-7, OPS-8)."""
    return Ok(value) if invalid == "error" else value


def _rounded(value, rounding):
    """The integer `rounding` selects for a finite `value` (OPS-8).

    What is left of `value` past its whole part is exact: a `float`
    with a fraction is below 2**52, where the difference of two
    `float`s of one sign is held exactly. So a tie is seen as one.
    """
    if rounding == "floor":
        return _math.floor(value)
    if rounding == "ceiling":
        return _math.ceil(value)
    # Toward zero.
    whole = _math.trunc(value)
    if rounding == "truncate":
        return whole
    if rounding == "half_even":
        # Python's `round` of a `float` is to the nearest integer, "a
        # tie to the even integer".
        return round(value)
    left = _magnitude(value - whole)
    # The integer next to `whole` on the side away from zero.
    away = whole + (1 if value > 0 else -1)
    if rounding == "half_up":
        # "a tie away from zero"
        return away if left >= 0.5 else whole
    if rounding == "half_down":
        # "a tie toward zero"
        return away if left > 0.5 else whole
    raise HglError(f"the rounding `{rounding}` is not provided")


def convert_f64_Rounding_i64__to_i64(value, rounding, nan, invalid):
    """An `f64` as an `i64`: "The integer its rounding selects".

    OPS-7: it fails for "an `f64` that is NaN, an infinity, or outside
    `i64` once rounded". OPS-8: `nan` is "the value a NaN converts to.
    Null leaves a NaN to `invalid`". `rounding` is a member of core's
    `Rounding`, which is named as the language names it.
    """
    if value != value:
        if nan is not None:
            return _converted(nan, invalid)
        return _unconverted("a NaN is no `i64`", invalid)
    if value in (_INFINITY, -_INFINITY):
        return _unconverted("an infinity is no `i64`", invalid)
    whole = _rounded(value, rounding.name)
    if not _I64_MIN <= whole <= _I64_MAX:
        return _unconverted("the number is outside `i64` once rounded", invalid)
    return _converted(whole, invalid)


def convert_str_to_bool(value, invalid):
    """OPS-6: "The value of the literal the text is". OPS-7: it fails
    for "text that is no literal of the type asked for"."""
    if value == "true":
        return _converted(True, invalid)
    if value == "false":
        return _converted(False, invalid)
    return _unconverted("the text is no `bool` literal", invalid)


def convert_i64_to_str(value):
    """OPS-6: "Its decimal digits, after a `-` where it is below zero"."""
    return str(value)


def convert_f64_to_str(value):
    """OPS-6: "The shortest decimal text that reads back as the same
    `f64`, always with a `.` or an exponent, after a `-` where its sign
    is negative ... It has an exponent where the number is 1e16 or
    more, or below 1e-4 and not zero, in size ... A NaN is `nan`, and
    the infinities are `inf` and `-inf`".

    Python's own text of a `float` is that shortest text, with an
    exponent in the same two ranges. It writes the exponent with a
    sign and two digits, `1e+21` and `1.5e-07`, where the language's
    literal is `1e21` and `1.5e-7`.
    """
    if value != value:
        return "nan"
    if value in (_INFINITY, -_INFINITY):
        return "inf" if value > 0 else "-inf"
    text = repr(value)
    if "e" not in text:
        return text
    digits, exponent = text.split("e")
    return f"{digits}e{int(exponent)}"


# The language's literals of a number (its LEX-22, LEX-23): a run of
# digits with one `_` between two at most, `0x` and hexadecimal digits,
# or `0b` and binary ones; and a float's, digits with a fraction, an
# exponent, or both. The digits are ASCII's.
_DIGITS = "[0-9](?:_?[0-9])*"
_INTEGER_TEXT = _re.compile(f"0x[0-9A-Fa-f](?:_?[0-9A-Fa-f])*|0b[01](?:_?[01])*|{_DIGITS}")
_EXPONENT = f"[eE][+-]?{_DIGITS}"
_FLOAT_TEXT = _re.compile(f"{_DIGITS}(?:\\.{_DIGITS}(?:{_EXPONENT})?|{_EXPONENT})")


# A number beyond every `i64` and every `f64`, which a decimal of more
# digits than this has is taken to be.
_BEYOND_DIGITS = 400
_BEYOND = 10**_BEYOND_DIGITS


def _integer_written(text):
    """The integer the integer literal `text` writes, or None where the
    text is no such literal.

    Python reads no decimal text of more than a few thousand digits.
    Zeros before a number change nothing and are left out, and a number
    of more digits than any `i64` or `f64` has is given as one beyond
    them all: what converts it fails as it does for any number too
    large.
    """
    if _INTEGER_TEXT.fullmatch(text) is None:
        return None
    digits = text.replace("_", "")
    if digits.startswith("0x"):
        return int(digits[2:], 16)
    if digits.startswith("0b"):
        return int(digits[2:], 2)
    digits = digits.lstrip("0") or "0"
    if len(digits) > _BEYOND_DIGITS:
        return _BEYOND
    return int(digits, 10)


def convert_str_to_i64(value, invalid):
    """OPS-6: "The number the text writes: an integer literal, after
    one optional `-` ... A number outside `i64` fails". The text is all
    of it: "nothing stands before or after it"."""
    negative = value.startswith("-")
    written = _integer_written(value[1:] if negative else value)
    if written is None:
        return _unconverted("the text is no integer", invalid)
    number = -written if negative else written
    if not _I64_MIN <= number <= _I64_MAX:
        return _unconverted("the number the text writes is outside `i64`", invalid)
    return _converted(number, invalid)


def convert_str_to_f64(value, invalid):
    """OPS-6: "The `f64` nearest the number the text writes, a tie to
    the even one: a float or an integer literal, after one optional
    `-` ... One that rounds to an infinity fails ... `"nan"` is a NaN,
    and `"inf"` and `"-inf"` are the infinities"."""
    if value == "nan":
        return _converted(_NAN, invalid)
    if value == "inf":
        return _converted(_INFINITY, invalid)
    if value == "-inf":
        return _converted(-_INFINITY, invalid)
    negative = value.startswith("-")
    text = value[1:] if negative else value
    if _FLOAT_TEXT.fullmatch(text) is not None:
        # Python reads a decimal as the nearest `float`, a tie to the
        # even one, and one too large as an infinity.
        number = float(text.replace("_", ""))
    else:
        written = _integer_written(text)
        if written is None:
            return _unconverted("the text is no number", invalid)
        try:
            number = float(written)
        except OverflowError:
            number = _INFINITY
    if number == _INFINITY:
        return _unconverted("the number the text writes rounds to an infinity", invalid)
    return _converted(-number if negative else number, invalid)


def convert_bytes_to_str(value, invalid):
    """OPS-6: UTF-8. OPS-7: it fails for "bytes that are not
    well-formed UTF-8"."""
    try:
        text = value.decode("utf-8")
    except UnicodeDecodeError:
        return _unconverted("the bytes are not well-formed UTF-8", invalid)
    return _converted(text, invalid)


def _moved(instant, duration):
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


def instant(written, minutes):
    """The instant of a date and a time written `minutes` east of UTC.

    LEX-11: a `datetime` literal writes its offset, and the instant is
    that far from what its date and time say. An instant is a `datetime`
    in UTC that names no zone, as hgraph has them.
    """
    return _moved(written, -_datetime.timedelta(minutes=minutes))


def add__datetime_duration(lhs, rhs):
    return _moved(lhs, rhs)


def sub__datetime_duration(lhs, rhs):
    return _moved(lhs, -rhs)


def sub__datetime_datetime(lhs, rhs):
    return _duration(_microseconds(lhs - rhs))


def add__civil_datetime_duration(lhs, rhs):
    return _moved(lhs, rhs)


def sub__civil_datetime_duration(lhs, rhs):
    return _moved(lhs, -rhs)


def sub__civil_datetime_civil_datetime(lhs, rhs):
    return _duration(_microseconds(lhs - rhs))


def add__date_time(lhs, rhs):
    """OPS-1: a `date` and a `time` are the `civil_datetime` of that time
    on that date."""
    return _datetime.datetime.combine(lhs, rhs)


def sub__date_date(lhs, rhs):
    return _duration(_microseconds(lhs - rhs))


def add__date_duration(lhs, rhs):
    """OPS-10: a `date` with a `duration` added or taken away "is the
    `civil_datetime` that far from the start of the date, whatever the
    duration"."""
    return _moved(_datetime.datetime.combine(lhs, _datetime.time()), rhs)


def sub__date_duration(lhs, rhs):
    return _moved(_datetime.datetime.combine(lhs, _datetime.time()), -rhs)


class Tenor:
    """A `tenor`, a rule for moving a date (the language's TYPE-17): "a
    count of months and a count of days, or it is a count of business
    days".

    `business` is `None` for a tenor of months and days. Two are equal
    "when they are of the same form and their counts are equal".
    """

    __slots__ = ("months", "days", "business")

    def __init__(self, months=0, days=0, business=None):
        self.months = months
        self.days = days
        self.business = business

    def _counts(self):
        return (self.months, self.days, self.business)

    def __eq__(self, other):
        return isinstance(other, Tenor) and self._counts() == other._counts()

    def __hash__(self):
        return hash(self._counts())

    def __repr__(self):
        return convert_tenor_to_str(self)


def _tenor_count(value):
    """TEN-2: "A count outside the i64 range is an HGL error with code
    `numeric.out_of_range`"."""
    if not _I64_MIN <= value <= _I64_MAX:
        raise HglError("a count of a tenor is outside the signed 64-bit range", code="numeric.out_of_range")
    return value


def _same_form(lhs, rhs):
    """TEN-2: "The operands of a sum or a difference are of one form: a
    tenor of business days with one of months and days is an HGL error
    with code `tenor.mixed_units`"."""
    if (lhs.business is None) != (rhs.business is None):
        raise HglError(
            "a tenor of business days and one of months and days have no sum",
            code="tenor.mixed_units",
        )


def add__tenor_tenor(lhs, rhs):
    """TEN-2: "A sum, a difference, a negation, and a scaling apply to
    each count"."""
    _same_form(lhs, rhs)
    if lhs.business is not None:
        return Tenor(business=_tenor_count(lhs.business + rhs.business))
    return Tenor(_tenor_count(lhs.months + rhs.months), _tenor_count(lhs.days + rhs.days))


def sub__tenor_tenor(lhs, rhs):
    _same_form(lhs, rhs)
    if lhs.business is not None:
        return Tenor(business=_tenor_count(lhs.business - rhs.business))
    return Tenor(_tenor_count(lhs.months - rhs.months), _tenor_count(lhs.days - rhs.days))


def neg__tenor(value):
    if value.business is not None:
        return Tenor(business=_tenor_count(-value.business))
    return Tenor(_tenor_count(-value.months), _tenor_count(-value.days))


def mul__tenor_i64(lhs, rhs):
    if lhs.business is not None:
        return Tenor(business=_tenor_count(lhs.business * rhs))
    return Tenor(_tenor_count(lhs.months * rhs), _tenor_count(lhs.days * rhs))


def compare__tenor_tenor(lhs, rhs):
    """TEN-3: "The order is count by count ... `<=>` gives `equivalent`
    for two equal tenors; `less` or `greater` where each count that
    differs differs that way; and `unordered` otherwise, two tenors of
    different forms included"."""
    if (lhs.business is None) != (rhs.business is None):
        return "unordered"
    if lhs.business is not None:
        pairs = ((lhs.business, rhs.business),)
    else:
        pairs = ((lhs.months, rhs.months), (lhs.days, rhs.days))
    less = any(left < right for left, right in pairs)
    greater = any(left > right for left, right in pairs)
    if less and greater:
        return "unordered"
    return "less" if less else "greater" if greater else "equivalent"


def lt__tenor_tenor(lhs, rhs):
    """TEN-3: "`<`, `<=`, `>`, and `>=` are false for an unordered
    pair"."""
    return compare__tenor_tenor(lhs, rhs) == "less"


def le__tenor_tenor(lhs, rhs):
    return compare__tenor_tenor(lhs, rhs) in ("less", "equivalent")


def gt__tenor_tenor(lhs, rhs):
    return compare__tenor_tenor(lhs, rhs) == "greater"


def ge__tenor_tenor(lhs, rhs):
    return compare__tenor_tenor(lhs, rhs) in ("greater", "equivalent")


def convert_tenor_to_str(value):
    """TEN-9: a tenor's text "uses the largest units: its months as `Y`
    and `M`, its days as `W` and `D`, or its business days as `B`. A unit
    whose count is zero is not written; a tenor of no months and no days
    is `0D`. A count below zero has `-` before its units"."""

    def units(count, larger, each, smaller):
        if count == 0:
            return ""
        sign, count = ("-", -count) if count < 0 else ("", count)
        high, low = divmod(count, each)
        return sign + (f"{high}{larger}" if high else "") + (f"{low}{smaller}" if low else "")

    if value.business is not None:
        return f"{value.business}B"
    return units(value.months, "Y", 12, "M") + units(value.days, "W", 7, "D") or "0D"


def fn_business_days(value):
    """The count of a tenor of business days, and null for one of months
    and days (TEN-1)."""
    return value.business


def fn_moved(day, by):
    """TEN-4: "A date moved by a tenor of months and days takes the
    months first: the same day of the month that many months on, or the
    last day of that month where it has no such day. Then it takes the
    days ... A result outside the `date` range is an HGL error with code
    `numeric.out_of_range`"."""
    year, month = divmod(day.year * 12 + (day.month - 1) + by.months, 12)
    month += 1
    try:
        last = _calendar.monthrange(year, month)[1]
        return _datetime.date(year, month, min(day.day, last)) + _datetime.timedelta(days=by.days)
    except (OverflowError, ValueError):
        raise HglError(
            "a date result is outside the years 1 to 9999", code="numeric.out_of_range"
        ) from None


def fn_weekend(day):
    """TEN-5: true for a Saturday and for a Sunday."""
    return day.weekday() >= 5


def fn_same_month(lhs, rhs):
    """TEN-8: true where the two dates are in one month of one year."""
    return (lhs.year, lhs.month) == (rhs.year, rhs.month)


def neg__i64(value):
    return _i64(-value)


def neg__f64(value):
    return -value


def eq__T_T(lhs, rhs):
    return _equal(lhs, rhs)


def ne__T_T(lhs, rhs):
    return not _equal(lhs, rhs)


def lt__T_T(lhs, rhs):
    return lhs < rhs


def le__T_T(lhs, rhs):
    return lhs <= rhs


def gt__T_T(lhs, rhs):
    return lhs > rhs


def ge__T_T(lhs, rhs):
    return lhs >= rhs


def _compare(lhs, rhs):
    """`lhs <=> rhs`: the name of the member of core's `CmpResult` the
    comparison comes to (OPS-3). NUM-6: "A NaN operand yields
    `unordered`; equivalent numeric values, including opposite signed
    zeros, yield `equivalent`." Only a number is a NaN, so "only a
    numeric comparison yields `unordered`". Python compares an `i64`
    with an `f64` exactly.

    A native candidate that gives an enum gives its member's name, as
    one that takes an enum is given it: the generated module reads the
    member from the enum it declares.
    """
    if lhs != lhs or rhs != rhs:
        return "unordered"
    if lhs < rhs:
        return "less"
    if lhs > rhs:
        return "greater"
    return "equivalent"


def compare__T_T(lhs, rhs):
    return _compare(lhs, rhs)


def compare__i64_f64(lhs, rhs):
    return _compare(lhs, rhs)


def compare__f64_i64(lhs, rhs):
    return _compare(lhs, rhs)


def and__bool_bool(lhs, rhs):
    return lhs and rhs


def or__bool_bool(lhs, rhs):
    return lhs or rhs


def not__bool(value):
    return not value


def _close(lhs, rhs, abs, rel):
    """`lhs == rhs` under the tolerances `abs` and `rel` (NUM-4, NUM-5).

    With no tolerance it is exact. Python compares an `int` with a
    `float` by their exact values, as NUM-2 has a mixed comparison do,
    and its `==` already has NaN equal to nothing, an infinity equal to
    itself alone, and the two zeros equal.

    NUM-4: "For finite operands, closeness is the symmetric condition"

        abs(a - b) <= max(abs_tolerance, rel_tolerance * max(abs(a), abs(b)))

    NUM-5: "The finite test must not admit an incorrect result through
    overflow of intermediate subtraction or scaling." The difference of
    `1e308` and `-1e308` is no `f64`, and nor is 1.9 times `1e308`: the
    test is made in exact fractions, which every finite number is.
    """
    # NUM-4: "Tolerances must be finite and nonnegative. ... An invalid
    # tolerance is an HGL error".
    for name, tolerance in (("abs", abs), ("rel", rel)):
        if not (_math.isfinite(tolerance) and tolerance >= 0):
            raise HglError(f"the tolerance `{name}` is not finite and nonnegative")
    if abs == 0 and rel == 0:
        return lhs == rhs
    # NUM-5: "NaN is neither equal nor close to any value, including
    # itself. Infinities are equal/close only to the same signed
    # infinity ... Apply these cases before the finite closeness rule."
    if lhs != lhs or rhs != rhs:
        return False
    if _math.isinf(lhs) or _math.isinf(rhs):
        return lhs == rhs
    left, right = _Fraction(lhs), _Fraction(rhs)
    allowed = max(_Fraction(abs), _Fraction(rel) * max(_magnitude(left), _magnitude(right)))
    return _magnitude(left - right) <= allowed


def eq__i64_i64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return _close(lhs, rhs, abs, rel)


def eq__f64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return _close(lhs, rhs, abs, rel)


def eq__i64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return _close(lhs, rhs, abs, rel)


# NUM-4: "Configured `!=` is the complement under the same options."
def ne__i64_i64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return not _close(lhs, rhs, abs, rel)


def ne__f64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return not _close(lhs, rhs, abs, rel)


def ne__i64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return not _close(lhs, rhs, abs, rel)


def lt__i64_f64(lhs, rhs):
    return lhs < rhs


def lt__f64_i64(lhs, rhs):
    return lhs < rhs


def le__i64_f64(lhs, rhs):
    return lhs <= rhs


def le__f64_i64(lhs, rhs):
    return lhs <= rhs


def gt__i64_f64(lhs, rhs):
    return lhs > rhs


def gt__f64_i64(lhs, rhs):
    return lhs > rhs


def ge__i64_f64(lhs, rhs):
    return lhs >= rhs


def ge__f64_i64(lhs, rhs):
    return lhs >= rhs
