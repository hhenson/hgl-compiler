"""Core's native value functions, for the hgraph target.

Written by the hgl compiler beside the modules it generates. Each function
is one native candidate of core's `hgl/native/scalar_operators.hgl`, named
after the candidate and the types of its parameters: `add__i64_i64` is
`native impl const fn add_(lhs: i64, rhs: i64) -> i64`.

Core's numeric contract has `i64` arithmetic neither wrap nor promote: a
result outside the signed 64-bit range is an error.

A division takes `zero`, the name of a member of core's `ZeroPolicy`,
which says what a zero divisor gives (NUM-3, NUM-12): `"ieee"` what IEEE
arithmetic does, `"nan"` a NaN, and `"fail"` an error with the code
`numeric.division_by_zero`. Under `"error"` a division gives a `result`
(NUM-9): an `Err` for a zero divisor, and an `Ok` of the quotient
otherwise.

A `result` is an `Ok` or an `Err`, and an `error` an `Error`: the values
the language's `ok`, `err` and `error` make (ERR-2, ERR-4).
"""

import math as _math

_I64_MIN = -(2**63)
_I64_MAX = 2**63 - 1
_NAN = float("nan")
_INFINITY = float("inf")


class HglError(Exception):
    """A failure of an HGL operation while a program runs.

    `code` is the catalogued code of the failure, where it has one.
    """

    def __init__(self, message, code=None):
        super().__init__(message if code is None else f"{code}: {message}")
        self.code = code


class Ok:
    """A success of a `result`, which owns its value (ERR-2)."""

    __slots__ = ("value",)

    def __init__(self, value):
        self.value = value

    # ERR-2: "Equality ... compares tag then selected payload. This
    # adds no ordering/hash": with `__eq__` and no `__hash__`, Python
    # makes the class unhashable.
    def __eq__(self, other):
        return type(other) is Ok and self.value == other.value

    def __repr__(self):
        return f"ok({self.value!r})"


class Err:
    """A failure of a `result`, which owns its error (ERR-2)."""

    __slots__ = ("error",)

    def __init__(self, error):
        self.error = error

    def __eq__(self, other):
        return type(other) is Err and self.error == other.error

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


def _i64(value):
    if not _I64_MIN <= value <= _I64_MAX:
        raise HglError("an i64 result is outside the signed 64-bit range")
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
    # NUM-2: floating arithmetic "preserves IEEE special values". An
    # infinite numerator has an infinite quotient, or NaN over an
    # infinity, and its floor is itself. Python's `//` would give NaN
    # for every one. What a finite numerator over an infinity gives is
    # not said by core yet (hhenson/hgl_core#35), and is Python's.
    if _math.isinf(lhs):
        return lhs / rhs
    return float(lhs // rhs)


def _mod(lhs, rhs, zero):
    """`%` with an `f64` operand: NaN for a zero divisor under `ieee`."""
    if rhs == 0:
        if zero == "fail":
            raise HglError("a division by zero", code="numeric.division_by_zero")
        if zero not in ("ieee", "nan"):
            raise HglError(f"the zero policy `{zero}` is not provided")
        return _NAN
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


def neg__i64(value):
    return _i64(-value)


def neg__f64(value):
    return -value


def eq__T_T(lhs, rhs):
    return lhs == rhs


def ne__T_T(lhs, rhs):
    return lhs != rhs


def lt__T_T(lhs, rhs):
    return lhs < rhs


def le__T_T(lhs, rhs):
    return lhs <= rhs


def gt__T_T(lhs, rhs):
    return lhs > rhs


def ge__T_T(lhs, rhs):
    return lhs >= rhs


def _exact(abs, rel):
    if abs != 0.0 or rel != 0.0:
        raise HglError("comparison within a tolerance is not provided yet")


def eq__i64_i64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    _exact(abs, rel)
    return lhs == rhs


def eq__f64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    _exact(abs, rel)
    return lhs == rhs


def eq__i64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    _exact(abs, rel)
    return lhs == rhs


def ne__i64_i64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    _exact(abs, rel)
    return lhs != rhs


def ne__f64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    _exact(abs, rel)
    return lhs != rhs


def ne__i64_f64_f64_f64(lhs, rhs, abs=0.0, rel=0.0):
    _exact(abs, rel)
    return lhs != rhs


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
