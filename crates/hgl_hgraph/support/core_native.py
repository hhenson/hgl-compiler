"""Core's native value functions, for the hgraph target.

Written by the hgl compiler beside the modules it generates. Each function
is one native candidate of core's `hgl/native/`, named after the candidate
and the types of its parameters: `add__i64_i64` is
`native impl const fn add_(lhs: i64, rhs: i64) -> i64`. The transpiler
reads the names this file defines, and only this file. What the
compiler names by hand is in `_core_support.py`, which this imports: a
generated module reaches both as `_core.name`. A conversion is
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

A `tenor`, a `timezone`, a `zoned_datetime` and a `zoned_time` are frozen
dataclasses here, which hgraph holds by their fields, so that each is a
temporal value and a nullable one (TYPE-2, TYPE-14, TYPE-17). A zone
literal is made by a function that checks the zone (LEX-12).

A division takes `zero`, the name of a member of core's `ZeroPolicy`,
which says what a zero divisor gives (NUM-3, NUM-12): `"ieee"` what IEEE
arithmetic does, `"nan"` a NaN, and `"fail"` an error with the code
`numeric.division_by_zero`. Under `"error"` a division gives a `result`
(NUM-9): an `Err` for a zero divisor, and an `Ok` of the quotient
otherwise.
"""

import calendar as _calendar
import dataclasses as _dataclasses
import datetime as _datetime
import functools as _functools
import math as _math
import re as _re
import zoneinfo as _zoneinfo
from fractions import Fraction as _Fraction

# What the compiler names by hand, which a generated module reaches as
# `_core.name` too.
from _hgl._core_support import *  # noqa: F403

# The private helpers the natives use, and those the harness reads as
# `_core`'s.
from _hgl._core_support import (
    _I64_MAX,
    _I64_MIN,
    _INFINITY,
    _MICROSECOND,
    _NAN,
    _alarm,
    _causes,
    _equal,
    _is_set_delta,
    _magnitude,
    _raise,
    _removes,
    _shifted,
    _tag,
    _wake_delay,
)

# ==== Core's natives, each named after its candidate ====


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


def schedule_Scheduler_duration_str__bool(capability, at, tag, on_wall_clock, _starting=None):
    """CAP-4: "With a `duration`, a wake that long after the current
    evaluation time". hgraph counts a `timedelta` from then, and "a wake
    asked for under a tag replaces the pending wake of that tag".

    A wake at a time no cycle can still run at, the current evaluation
    time or before (core's CRUN-3: each cycle's time "is later than the
    one before"), is kept by no scheduler of hgraph's: it is not
    delivered, as an alarm's is not (`_alarm`).

    `_starting` is given where the compiler did not see the duration:
    it is checked then (`_wake_delay`, TIME-8), once every argument of
    the call is evaluated, true where the call is written in `start`."""
    if _starting is not None:
        _wake_delay(at, _starting)
    capability.schedule(at, tag=_tag(tag), on_wall_clock=on_wall_clock)


# CAP-4: "with a `datetime`, a wake then".
schedule_Scheduler_datetime_str__bool = schedule_Scheduler_duration_str__bool


def fn_un_schedule(scheduler, tag):
    """CAP-4: "`un_schedule(scheduler)` and `un_schedule(scheduler, tag:
    name)` cancel a pending wake": the earliest, or the one asked for
    under `name`."""
    if tag is None:
        scheduler.un_schedule()
    else:
        scheduler.un_schedule(_tag(tag))


def fn_reset(scheduler):
    """CAP-4: "`reset(scheduler)` every one"."""
    scheduler.reset()


def schedule_Alarm_duration(capability, at, _starting=None):
    """CAP-5: "`schedule(alarm, at)` asks for a wake" (`_alarm`). What may
    be asked of it is what may be asked of the scheduler: a duration is
    checked as the scheduler's is (`schedule_Scheduler_duration_str__bool`)."""
    if _starting is not None:
        _wake_delay(at, _starting)
    _alarm(capability, at)


schedule_Alarm_datetime = schedule_Alarm_duration


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


def _i64(value, what="an i64 result"):
    """`value`, where it is in the range of an `i64`. NUM-2: "an `i64`
    result outside the signed 64-bit range is an HGL error with code
    `numeric.out_of_range`"; `what` says what it is."""
    if not _I64_MIN <= value <= _I64_MAX:
        raise HglError(f"{what} is outside the signed 64-bit range", code="numeric.out_of_range")
    return value


def add__i64_i64(lhs, rhs):
    return _i64(lhs + rhs)


def add__f64_f64(lhs, rhs):
    return lhs + rhs


def add__i64_f64(lhs, rhs):
    return float(lhs) + rhs


def add__str_str(lhs, rhs):
    return lhs + rhs


def mul__str_i64(lhs, rhs):
    """OPS-1: a `str` times an `i64` is a `str`, core's string
    repetition: the text `rhs` times, none for zero. The contract says
    nothing of a count below zero, which Python would take as zero, so
    that is not written."""
    if rhs < 0:
        raise NotImplementedError(
            "the hgraph target does not write a string times a count below zero yet"
        )
    return lhs * rhs


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


def _by_zero():
    """The error of a zero divisor: NUM-3, "The `fail` policy makes a zero
    divisor an HGL error with code `numeric.division_by_zero`", and NUM-9,
    under `error` "`err(...)` carrying error code
    `numeric.division_by_zero`"."""
    return Error("a division by zero", code="numeric.division_by_zero")


def _zero_divisor(lhs, rhs, zero):
    """What a division by the zero `rhs` gives, when it gives a number.

    NUM-3: under `ieee`, "a finite nonzero numerator divided by signed
    zero produces signed infinity; zero divided by zero produces NaN.
    The infinity sign combines both operand signs." Under `nan` every
    zero divisor gives NaN, and under `fail` it is an error. The
    candidates of core that come here require one of the three:
    `error` is `_divide`'s.
    """
    if zero == "fail":
        _raise(_by_zero())
    if zero == "nan" or lhs == 0 or lhs != lhs:
        return _NAN
    # `ieee`.
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
    """`%` with an `f64` operand: NaN for a zero divisor under `ieee`
    and `nan`, the other two its candidates require (`error` is
    `_divide`'s)."""
    if rhs == 0:
        if zero == "fail":
            _raise(_by_zero())
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


def _floordiv_i64(lhs, rhs, zero):
    # NUM-1: "Integer floor division rounds toward negative infinity",
    # as Python's does. A division of two `i64` gives an `i64`, which
    # holds no NaN or infinity: NUM-12 admits `fail` there, and `error`,
    # which is `_divide`'s.
    if rhs == 0:
        _raise(_by_zero())
    return _i64(lhs // rhs)


def _mod_i64(lhs, rhs, zero):
    # NUM-1: "modulo has the divisor's sign when nonzero", as Python's
    # does. `fail`, as `_floordiv_i64`'s.
    if rhs == 0:
        _raise(_by_zero())
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
        return Err(_by_zero())
    # The divisor is not zero, so no policy is consulted.
    return Ok(ordinary(lhs, rhs, "fail"))


def div__i64_i64(lhs, rhs, zero):
    return _divide(_div, lhs, rhs, zero)


div__f64_f64 = div__i64_i64


div__i64_f64 = div__i64_i64


div__f64_i64 = div__i64_i64


def floordiv__i64_i64(lhs, rhs, zero):
    return _divide(_floordiv_i64, lhs, rhs, zero)


def floordiv__f64_f64(lhs, rhs, zero):
    return _divide(_floordiv, lhs, rhs, zero)


floordiv__i64_f64 = floordiv__f64_f64


floordiv__f64_i64 = floordiv__f64_f64


def mod__i64_i64(lhs, rhs, zero):
    return _divide(_mod_i64, lhs, rhs, zero)


def mod__f64_f64(lhs, rhs, zero):
    return _divide(_mod, lhs, rhs, zero)


mod__i64_f64 = mod__f64_f64


mod__f64_i64 = mod__f64_f64


def neg__i64(value):
    return _i64(-value)


def neg__f64(value):
    return -value


eq__T_T = _equal


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


compare__T_T = _compare


compare__i64_f64 = compare__T_T


compare__f64_i64 = compare__T_T


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


eq__f64_f64_f64_f64 = eq__i64_i64_f64_f64


eq__i64_f64_f64_f64 = eq__i64_i64_f64_f64


# NUM-4: "Configured `!=` is the complement under the same options."
def ne__i64_i64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    return not _close(lhs, rhs, abs, rel)


ne__f64_f64_f64_f64 = ne__i64_i64_f64_f64


ne__i64_f64_f64_f64 = ne__i64_i64_f64_f64


lt__i64_f64 = lt__T_T


lt__f64_i64 = lt__T_T


le__i64_f64 = le__T_T


le__f64_i64 = le__T_T


gt__i64_f64 = gt__T_T


gt__f64_i64 = gt__T_T


ge__i64_f64 = ge__T_T


ge__f64_i64 = ge__T_T


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
        _raise(_by_zero())
    if isinstance(rhs, float) and not _math.isfinite(rhs):
        raise HglError(
            "a duration divided by an infinity or a NaN", code="numeric.out_of_range"
        )
    return _duration(round(_Fraction(_microseconds(lhs)) / _Fraction(rhs)))


def div__duration_i64(lhs, rhs, zero):
    return _divide(_div_duration, lhs, rhs, zero)


div__duration_f64 = div__duration_i64


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


# OPS-6: "False for zero, true otherwise; NaN is true". A NaN is
# unequal to zero, as to everything.
convert_f64_to_bool = convert_i64_to_bool


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
    # `fail`, the other of core's `InvalidPolicy`.
    raise HglError(message, code="convert.invalid")


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
    # `half_down`, the last of core's `Rounding`: "a tie toward zero".
    return away if left > 0.5 else whole


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


def instant(written, minutes):
    """The instant of a date and a time written `minutes` east of UTC.

    LEX-11: a `datetime` literal writes its offset, and the instant is
    that far from what its date and time say. An instant is a `datetime`
    in UTC that names no zone, as hgraph has them.
    """
    return _shifted(written, -_datetime.timedelta(minutes=minutes))


def add__datetime_duration(lhs, rhs):
    return _shifted(lhs, rhs)


def sub__datetime_duration(lhs, rhs):
    return _shifted(lhs, -rhs)


def sub__datetime_datetime(lhs, rhs):
    return _duration(_microseconds(lhs - rhs))


add__civil_datetime_duration = add__datetime_duration


sub__civil_datetime_duration = sub__datetime_duration


sub__civil_datetime_civil_datetime = sub__datetime_datetime


def add__date_time(lhs, rhs):
    """OPS-1: a `date` and a `time` are the `civil_datetime` of that time
    on that date."""
    return _datetime.datetime.combine(lhs, rhs)


sub__date_date = sub__datetime_datetime


def add__date_duration(lhs, rhs):
    """OPS-10: a `date` with a `duration` added or taken away "is the
    `civil_datetime` that far from the start of the date, whatever the
    duration"."""
    return _shifted(_datetime.datetime.combine(lhs, _datetime.time()), rhs)


def sub__date_duration(lhs, rhs):
    return _shifted(_datetime.datetime.combine(lhs, _datetime.time()), -rhs)


# TYPE-2: "named `timezone`, resolved `zoned_datetime`, and
# date-independent `zoned_time`". Each is a frozen dataclass, a value
# hgraph holds by its fields, so that it is a temporal value and a
# nullable one (TYPE-14). A zone is kept by its name: "zone aliases are
# distinct names", so two names are two zones whatever each links to,
# and the dataclass compares the names.


@_dataclasses.dataclass(frozen=True, repr=False)
class Timezone:
    """A `timezone`: a zone of the run provider's catalogue, by name."""

    name: str

    def __repr__(self):
        return f"@[{self.name}]"


@_dataclasses.dataclass(frozen=True, repr=False)
class ZonedDatetime:
    """A `zoned_datetime`: an instant, a `datetime` in UTC that names no
    zone as hgraph has them, resolved in the zone named `zone`.

    Two are equal where both the instants and the zones are: "Same-instant
    zoned values need not be equal" (TYPE-2), and two zones are two names.
    The offset is the zone's at the instant, so it is no field.
    """

    instant: _datetime.datetime
    zone: str

    def __repr__(self):
        try:
            local = self.instant.replace(tzinfo=_datetime.UTC).astimezone(_zone_info(self.zone))
        except (HglError, OverflowError):
            return f"@{self.instant.isoformat()}Z[{self.zone}]"
        return f"@{local.isoformat()}[{self.zone}]"


@_dataclasses.dataclass(frozen=True, repr=False)
class ZonedTime:
    """A `zoned_time`: a time of day in the zone named `zone`, on no date,
    so with no offset."""

    time: _datetime.time
    zone: str

    def __repr__(self):
        return f"@{self.time.isoformat()}[{self.zone}]"


@_functools.cache
def _catalogue():
    """The names of the run provider's zone catalogue: those of the IANA
    database Python's `zoneinfo` reads, aliases included. Read once."""
    return frozenset(_zoneinfo.available_timezones())


def _zone_info(name):
    """The zone `name` names. LEX-12: "The selected run provider checks
    exact catalog membership": a name the catalogue has not is a
    failure. The catalogue is the run's, so "Provider checks do not
    depend on the compiler's local time-zone database". No code is
    catalogued for the failure."""
    if name not in _catalogue():
        raise HglError(f"the zone `{name}` is not in the run provider's zone catalogue")
    return _zoneinfo.ZoneInfo(name)


@_functools.cache
def timezone(name):
    """A `timezone` literal, `@[Europe/London]`, checked (LEX-12)."""
    _zone_info(name)
    return Timezone(name)


@_functools.cache
def zoned_time(written, name):
    """A `zoned_time` literal, `@09:30[Europe/London]`: the time `written`
    in the zone `name`, checked to be in the catalogue (LEX-12). It is
    on no date, so it has no offset to check."""
    _zone_info(name)
    return ZonedTime(written, name)


@_functools.cache
def zoned_datetime(written, minutes, name):
    """A `zoned_datetime` literal, `@2026-09-03T09:30+01:00[Europe/London]`:
    the date and time `written`, `minutes` east of UTC, in the zone `name`.

    LEX-11: "A zoned datetime requires an explicit offset", and LEX-12:
    the provider checks "offset consistency". The written offset is
    consistent where it is the zone's own at the instant it gives, and
    then that instant in the zone is the date and time written. A time
    a change of offset repeats is one instant at each of its two
    offsets, and a time it skips is at none.
    """
    zone = _zone_info(name)
    at = instant(written, minutes)
    try:
        offset = at.replace(tzinfo=_datetime.UTC).astimezone(zone).utcoffset()
    except OverflowError:
        # The instant in the zone is outside Python's years, so not the
        # date and time written: the zone's offset is another.
        offset = None
    if offset != _datetime.timedelta(minutes=minutes):
        raise HglError(
            f"the offset of `{written.isoformat()}` is not the offset of the zone `{name}` at that time"
        )
    return ZonedDatetime(at, name)


def add__zoned_datetime_duration(lhs, rhs):
    """OPS-1: a `zoned_datetime` and a `duration` give "that instant's
    type". A duration is elapsed time, so the instant moves by it and
    the zone is kept; the offset is the zone's at the new instant."""
    return ZonedDatetime(_shifted(lhs.instant, rhs), lhs.zone)


def sub__zoned_datetime_duration(lhs, rhs):
    return ZonedDatetime(_shifted(lhs.instant, -rhs), lhs.zone)


def eq__zoned_datetime_zoned_datetime_bool(lhs, rhs, utc=False):
    """OPS-2: "Two `zoned_datetime` values are equal when their instants
    and their zone names both are; compared as UTC, they are equal when
    their instants are". The dataclass compares both fields; `utc`
    compares the instants alone, each a `datetime` in UTC."""
    if utc:
        return lhs.instant == rhs.instant
    return lhs == rhs


def ne__zoned_datetime_zoned_datetime_bool(lhs, rhs, utc=False):
    return not eq__zoned_datetime_zoned_datetime_bool(lhs, rhs, utc)


@_dataclasses.dataclass(frozen=True, repr=False)
class Tenor:
    """A `tenor`, a rule for moving a date (the language's TYPE-17): "a
    count of months and a count of days, or it is a count of business
    days".

    `business` is `None` for a tenor of months and days. Two are equal
    "when they are of the same form and their counts are equal": the
    dataclass compares the three counts.

    A frozen dataclass is what hgraph holds as a value of fields, so a
    tenor is a temporal value, `TS[Tenor]`, and a nullable one,
    `TS[Tenor | None]`, too (TYPE-14). hgraph holds a class of no
    fields as an object, which may not be unset.
    """

    months: int = 0
    days: int = 0
    business: int | None = None

    def __repr__(self):
        return convert_tenor_to_str(self)


def _tenor_count(value):
    """TEN-2: "A count outside the i64 range is an HGL error with code
    `numeric.out_of_range`"."""
    return _i64(value, "a count of a tenor")


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
