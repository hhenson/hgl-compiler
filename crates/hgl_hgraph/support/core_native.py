"""Core's native value functions, for the hgraph target.

Written by the hgl compiler beside the modules it generates. Each function
is one native candidate of core's `hgl/native/scalar_operators.hgl`, named
after the candidate and the types of its parameters: `add__i64_i64` is
`native impl const fn add_(lhs: i64, rhs: i64) -> i64`.

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

A struct is a class `struct` makes: a value that is never changed, so
that one held in two places is as good as a copy in each (TYPE-5).
"""

import calendar as _calendar
import dataclasses as _dataclasses
import datetime as _datetime
import functools as _functools
import math as _math
from fractions import Fraction as _Fraction

from hgraph import Null as _Null

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


class HglError(Exception):
    """A failure of an HGL operation while a program runs.

    `code` is the catalogued code of the failure, where it has one.
    """

    def __init__(self, message, code=None):
        super().__init__(message if code is None else f"{code}: {message}")
        self.code = code


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


class Ok:
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


class Err:
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
    messages and their codes are.
    """

    __slots__ = ("message", "code")

    def __init__(self, message, code=None):
        self.message = message
        self.code = code

    def __eq__(self, other):
        return type(other) is Error and (self.message, self.code) == (other.message, other.code)

    def __repr__(self):
        return f"error(message: {self.message!r}, code: {self.code!r})"


class Propagated(Exception):
    """Leaves a value function with the failure a `?` in it met.

    The function catches it and returns `failure` (ERR-5).
    """

    def __init__(self, failure):
        super().__init__("a failure propagated by `?`")
        self.failure = failure


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
    raise HglError(result.error.message, code=result.error.code)


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


def struct(name, fields):
    """A struct of the program, as a class (TYPE-3: "Structs are
    nominal, with public ordered fields and no methods").

    `fields` is every field the struct has, its parents' first, each by
    its Python name. A value is made with each field by name, and is
    never changed after: "Retained results own their data; later
    mutation of a source cannot change an earlier retained value"
    (TYPE-5).

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
        # (`shared`).
        namespace={"__eq__": equal, "__hash__": hashed, "_hgl_fields": tuple(fields)},
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
    """`place.field = new` (EXPR-3): the struct `value` with one field
    another value. `value` itself is as it was, for whatever else holds
    it; the compiler assigns what this gives to the place.

    `value` is taken by position only: a field may be named `value`.
    The new struct keeps the field's value, which is marked as shared.
    """
    return _dataclasses.replace(value, **{name: shared(new) for name, new in field.items()})


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
    keeps the value, so a list a place grows is marked as shared
    (`shared`): the place's later growth does not reach the store."""
    store[key] = Held(shared(value))


class Grown(list):
    """A list that a place grows where it is: what a `var`, a `state`
    or a `cache` holds once `append` or `extend` has grown it.

    LIST-3: "a value taken from the place before the call is as it
    was". A `let`, a parameter and whatever else is given a place's
    list is given the list itself, with no copy, and the list is then
    marked as shared (`shared`). While `owned` holds nothing else has
    the list, and it grows where it is; once it does not, the place
    takes a copy before it grows, and what was given the list keeps it
    as it was.
    """

    # One made in any other way than by `_own` is not a place's own.
    owned = False


class GrownDict(dict):
    """A dictionary that a place changes where it is: what a `var`, a
    `state` or a `cache` holds once an entry of it has been assigned.
    It is marked and copied as a `Grown` list is."""

    # One made in any other way than by `_own_dict` is not a place's own.
    owned = False


# The types of the values that hold no other: `shared` has nothing to
# look for in one.
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


def shared(value):
    """`value`, marked as held by more than the place it was read from.

    A list or a dictionary a place changes is marked itself. A struct, a
    tuple and a success are small and are never changed, so they carry
    no mark: the collections directly inside one are marked in its
    place. What is inside a marked collection is not looked at: a change
    reaches it only through that collection, which is copied first.
    """
    kind = type(value)
    if kind in _PLAIN:
        return value
    if kind is Grown or kind is GrownDict:
        value.owned = False
    elif kind is tuple:
        for part in value:
            shared(part)
    elif kind is Ok:
        shared(value.value)
    else:
        fields = getattr(kind, "_hgl_fields", None)
        if fields is not None:
            for field in fields:
                shared(getattr(value, field))
    return value


def _shared_parts(values):
    """Marks each of `values`, the entries of a collection that is now
    held by two: a copy holds what the list it was copied from holds."""
    if not values:
        return
    # One pass that Python makes itself tells whether any entry may
    # hold a collection.
    if set(map(type, values)) <= _PLAIN:
        return
    for part in values:
        shared(part)


def _own(values):
    """The list a place grows: `values` itself where the place alone
    holds it, and a copy otherwise."""
    if type(values) is Grown and values.owned:
        return values
    grown = Grown(values)
    grown.owned = True
    _shared_parts(grown)
    return grown


def _own_dict(values):
    """The dictionary a place changes: `values` itself where the place
    alone holds it, and a copy otherwise."""
    if type(values) is GrownDict and values.owned:
        return values
    grown = GrownDict(values)
    grown.owned = True
    _shared_parts(grown.values())
    return grown


def each(values):
    """The entries of a list or a set a `for` runs over, each marked as
    shared: the loop's name holds an entry that the collection holds
    too."""
    for value in values:
        yield shared(value)


def each_entry(dictionary):
    """The keys and values of a dictionary a `for` runs over, each value
    marked as shared (`each`)."""
    for key, value in dictionary.items():
        yield key, shared(value)


def _changed(holder, steps, at, change):
    """What `holder` is to be once the value `steps[at:]` leads to inside
    it has been changed by `change`.

    `holder` is held by its place alone, or by a collection that is. A
    list and a dictionary on the way are changed where they are while
    their own mark says they are their holder's alone, and are copied
    once otherwise (`_own`); a struct and a tuple are small, and a new
    one is made where a part of it has another value. A step that fails,
    an index outside a list or a key a dictionary has not, fails before
    anything is changed.
    """
    if at == len(steps):
        return change(holder)
    kind, key = steps[at]
    if kind == "f":
        inner = getattr(holder, key)
        changed = _changed(inner, steps, at + 1, change)
        return holder if changed is inner else _dataclasses.replace(holder, **{key: changed})
    if kind == "k":
        if key not in holder:
            raise HglError(f"the dictionary has no key {key!r}", code="value.missing_key")
        holder = _own_dict(holder)
    else:
        if not 0 <= key < len(holder):
            raise HglError(f"the index {key} is outside a sequence of {len(holder)}")
        if type(holder) is tuple:
            inner = holder[key]
            changed = _changed(inner, steps, at + 1, change)
            return holder if changed is inner else holder[:key] + (changed,) + holder[key + 1 :]
        holder = _own(holder)
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
    changed where it is while it is its holder's alone (`_changed`).

    `steps` is the way in, each step its kind and what it names: `"f"`
    a struct's field, `"k"` a dictionary's key, and `"i"` a list's index
    or a tuple's position. The last step is the place assigned: "A key
    the dictionary has not is added."

    `value` gives the value when it is called. It is called once the
    way in has been read, so that a step that fails does so before the
    value is evaluated, as in any assignment; what it gives is kept, and
    is marked as shared. Only then is anything changed: the value may
    be, or hold, a collection on the way in, `tree.kids[0].kids[0] =
    tree`, which evaluating it marks as shared, and which is then
    copied and not changed.
    """
    kind, key = steps[-1]
    _read_in(root, steps[:-1])
    new = shared(value())

    def assign(holder):
        if kind == "f":
            return _dataclasses.replace(holder, **{key: new})
        if kind == "k":
            holder = _own_dict(holder)
            holder[key] = new
            return holder
        if not 0 <= key < len(holder):
            raise HglError(f"the index {key} is outside a sequence of {len(holder)}")
        if type(holder) is tuple:
            return holder[:key] + (new,) + holder[key + 1 :]
        holder = _own(holder)
        holder[key] = new
        return holder

    return _changed(root, steps[:-1], 0, assign)


def given_at(root, steps, function, place, arguments, /):
    """A call of `function`, a native function of core with one `var`
    parameter, named `place`, that is given a place inside the value
    `root`: `append(book.orders, order)` (the language's FN-15).

    The function is given the value `steps` leads to, which it changes
    where it is while that is its holder's alone. `arguments` gives its
    other arguments, by name, when it is called: once the way in has
    been read, as they are written after the place. Only then is
    anything changed: an argument may be, or hold, a collection on the
    way in, `append(tree.kids[0].kids, tree)`, which evaluating it
    marks as shared, and which is then copied and not changed. This
    gives the function's result and then what `root` is to be.
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
    changed where it is while the place alone holds it, and copied
    once otherwise (`Grown`); it keeps `value`, which is marked as
    shared. "An index outside a list fails where it is evaluated", and
    the list is then as it was."""
    if not 0 <= index < len(sequence):
        raise HglError(f"the index {index} is outside a sequence of {len(sequence)}")
    sequence = _own(sequence)
    sequence[index] = shared(value)
    return sequence


def set_entry(dictionary, key, value, /):
    """`place[key] = value` where the place is an entry of the
    dictionary a `var`, a `state` or a `cache` holds itself (EXPR-3).
    "A key the dictionary has not is added." The dictionary is changed
    where it is while the place alone holds it, and copied once
    otherwise (`GrownDict`)."""
    dictionary = _own_dict(dictionary)
    dictionary[key] = shared(value)
    return dictionary


def restored(values, length):
    """What a place holds once a call that was given its list has
    failed (the language's FN-15: a function that fails "assigns
    nothing"): the list as long as it was before the call.

    A list the place alone holds is cut back where it is. One that
    something took while the call ran stays as that took it, and the
    place holds a list of its first entries.
    """
    if len(values) <= length:
        return values
    if type(values) is Grown and values.owned:
        del values[length:]
        return values
    return values[:length]


def fn_append(values, item):
    """LIST-1: `append(values, item)` "gives the place `values` the list
    it held with `item` after its last entry".

    `values` is a `var` parameter (the language's FN-15), so this gives
    back the list the place is to hold, after a result it has none of.
    The list grows where it is while the place alone holds it (LIST-3,
    `Grown`), and the list keeps `item`, which is marked as shared.
    """
    values = _own(values)
    values.append(shared(item))
    return (None, values)


def fn_extend(values, items):
    """LIST-2: "each entry of `items` after its last entry, in the
    order `items` has them". The list grows as `fn_append` grows it,
    and keeps each entry, which `items` holds too."""
    values = _own(values)
    _shared_parts(items)
    values.extend(items)
    return (None, values)


def get_dict_K__T__K(store, key):
    """Core's `get<K, T>(store: dict<K, T>, key: K) -> T?`: the value a
    dictionary has for a key, or null where it has none (ERR-6). What
    it gives is still in the dictionary, and is marked as shared."""
    return shared(store.get(key))


def with_item(sequence, index, value, /):
    """`place[index] = value` for a list or a tuple (EXPR-3): the
    sequence with one element another value. "An index outside a list
    fails where it is evaluated." `sequence` itself is as it was, for
    whatever else holds it."""
    if not 0 <= index < len(sequence):
        raise HglError(f"the index {index} is outside a sequence of {len(sequence)}")
    # The new sequence holds what `sequence` holds, and the value.
    _shared_parts(sequence)
    return sequence[:index] + type(sequence)((shared(value),)) + sequence[index + 1 :]


def with_entry(dictionary, key, value, /):
    """`place[key] = value` for a dictionary (EXPR-3): the dictionary
    with the key given the value. "A key the dictionary has not is
    added." `dictionary` itself is as it was."""
    # The new dictionary holds what `dictionary` holds, and the value.
    _shared_parts(dictionary.values())
    return {**dictionary, key: shared(value)}


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
        return convert_tenor(self)


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


def convert_tenor(value):
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
