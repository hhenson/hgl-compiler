"""Core's native value functions, for the hgraph target.

Written by the hgl compiler beside the modules it generates. Each function
is one native candidate of core's `hgl/native/scalar_operators.hgl`, named
after the candidate and the types of its parameters: `add__i64_i64` is
`native impl const fn add_(lhs: i64, rhs: i64) -> i64`.

Core's numeric contract has `i64` arithmetic neither wrap nor promote: a
result outside the signed 64-bit range is an error.
"""

_I64_MIN = -(2**63)
_I64_MAX = 2**63 - 1


class HglError(Exception):
    """A failure of an HGL operation while a program runs."""


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
