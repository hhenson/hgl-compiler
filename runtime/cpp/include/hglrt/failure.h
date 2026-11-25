// HGL's execution failures, as hgraph carries them.
//
// ERR-1 keeps four outcomes apart: a source rejection (the compiler's), an
// execution failure, ordinary result data, and a test or infrastructure
// failure. An execution failure is thrown as hgraph's `ExecutionFailure`,
// carrying a `CanonicalError`: the code the catalogue gives it, or none, and
// its message. hgraph adds the node, the phase and the evaluation time where
// the failure leaves a node (ERR-4: "On first graph failure, attach original
// node identity, phase, and evaluation time"). The failure this library
// throws, `Failure`, also holds where in the source it was raised: the call
// of the native or the checked read that raised it, as `std::source_location`
// gives it at that call (the HGL file and line where the generated code
// places them with `#line`).
//
// An `error` is the value ERR-4 describes, hgraph's `CanonicalError`, and a
// `result<T, error>` is a `std::expected` of it (ERR-2), which hgraph's
// `NodeResult<T>` is too.
#pragma once

#include <hgraph/runtime/canonical_error.h>

#include <exception>
#include <expected>
#include <optional>
#include <source_location>
#include <string>
#include <string_view>
#include <utility>

namespace hglrt {

/// `error` (ERR-4): a message, a code that may be null, and the context a
/// graph failure gave it. Its equality is the code's and the message's:
/// "Equality excludes provenance".
using error = hgraph::CanonicalError;

/// `result<T, error>` (ERR-2): a success holding a `T`, or a failure holding
/// an `error`.
template <class T>
using result = std::expected<T, error>;

/// The execution codes of the catalogue this library raises (the language's
/// errors chapter, "Execution code").
namespace codes {
/// A read needs a value that is not there: a payload, a child that is not
/// valid, a whole that is not all valid (TIME-1, ERR-14).
inline constexpr std::string_view unset_read = "value.unset_read";
/// A nullable value held null at a place requiring its payload (ERR-12).
inline constexpr std::string_view null_read = "value.null";
/// An index outside an ordinary list, below zero or at or past its size,
/// where a read or an assignment evaluates it (EXPR-3, EXPR-7).
inline constexpr std::string_view index_out_of_range = "value.index_out_of_range";
/// A strict dictionary lookup or a read of an absent global store key (ERR-6, CAP-6).
inline constexpr std::string_view missing_key = "value.missing_key";
/// A result outside its type's range (NUM-2, OPS-10), and text repeated a
/// count below zero (OPS-1).
inline constexpr std::string_view out_of_range = "numeric.out_of_range";
/// A zero divisor under `.fail`, or `?` on the `err` of one under `.error`
/// (NUM-3, NUM-9).
inline constexpr std::string_view division_by_zero = "numeric.division_by_zero";
/// A tolerance for `==` or `!=` is negative or non-finite (NUM-4).
inline constexpr std::string_view invalid_tolerance = "numeric.invalid_tolerance";
/// A conversion that cannot be made (OPS-7, OPS-8).
inline constexpr std::string_view invalid_conversion = "convert.invalid";
/// A sum or a difference of a tenor of business days and one of months and
/// days (TEN-2).
inline constexpr std::string_view mixed_units = "tenor.mixed_units";
/// A source's `yield` after a negative duration (SOURCE-2).
inline constexpr std::string_view negative_duration = "yield.negative_duration";
/// A source's `yield` at a time that is not after the one before (SOURCE-3).
inline constexpr std::string_view non_increasing_time = "yield.non_increasing_time";
/// A wake asked of a scheduler or an alarm with a duration that is not more
/// than zero, or one below zero in `start` (TIME-8, CAP-4, CAP-5).
inline constexpr std::string_view non_positive_duration = "schedule.non_positive_duration";
/// A run's configuration with both an `end` and a `length` (CRUN-2).
inline constexpr std::string_view end_and_length = "run.end_and_length";
/// A run starts too early or ends too late (CRUN-3).
inline constexpr std::string_view run_bounds = "run.bounds";
} // namespace codes

/// The value `error(message: message, code: code)` (ERR-4): an error with no
/// context.
error make_error(std::string message, std::optional<std::string> code = std::nullopt);

/// An HGL execution failure (ERR-1): hgraph's `ExecutionFailure`, carrying
/// the error it fails with, and the place in the source it was raised at.
class Failure : public hgraph::ExecutionFailure {
  public:
    /// The failure with `failure`, raised at `where`. (In the class, `error`
    /// names hgraph's accessor of the error; the type is named in full.)
    Failure(hglrt::error failure, std::source_location where);

    /// Where it was raised: the call of what raised it.
    [[nodiscard]] const std::source_location& where() const noexcept { return where_; }

  private:
    std::source_location where_;
};

/// Fails with the catalogued execution code `code` and `message` (ERR-1).
[[noreturn]] void fail(std::string_view code, std::string message,
                       std::source_location where = std::source_location::current());

/// Fails with `message` and no code: a failure the catalogue has no code for,
/// which still fails and satisfies no coded expectation (the errors chapter:
/// "Unlisted failures still fail; they cannot satisfy a coded expectation").
[[noreturn]] void fail_uncoded(std::string message,
                               std::source_location where = std::source_location::current());

/// Fails with `failure` as it is, its context kept (ERR-5). A retained
/// error keeps its first provenance (ERR-8).
[[noreturn]] void raise(error failure, std::source_location where = std::source_location::current());

/// The error returned from the enclosing fallible value function (ERR-5).
/// A function catches only its own typed propagation, leaving execution
/// failures and errors from the runtime to their surrounding phase.
template <class E>
struct return_error {
    E value;
};

/// Read a result as its success value, returning the failure from the
/// enclosing fallible value function (ERR-5 and ERR-15).
template <class T, class E>
T propagate(std::expected<T, E> held) {
    if (!held) {
        throw return_error<E>{std::move(held).error()};
    }
    return *std::move(held);
}

/// Read a result as its success value, raising its original error if
/// the result is a failure (ERR-5 and ERR-15).
template <class T>
T require_result(result<T> held, std::source_location where = std::source_location::current()) {
    if (!held) {
        raise(std::move(held).error(), where);
    }
    return *std::move(held);
}

/// Take the nullable form of a result with a non-nullable success type
/// (ERR-15). An already nullable success takes the result's value form.
template <class T>
std::optional<T> result_or_null(result<T> held) {
    if (!held) {
        return std::nullopt;
    }
    return *std::move(held);
}

/// Read a nullable value or raise `value.null` in the current phase (ERR-12).
template <class T>
T require_non_null(std::optional<T> held,
                   std::source_location where = std::source_location::current()) {
    if (!held) {
        fail(codes::null_read, "a null value was read", where);
    }
    return *std::move(held);
}

/// Read a nullable value where a null returns a failure from a fallible
/// value function (ERR-5 and ERR-12).
template <class T>
T propagate_non_null(std::optional<T> held) {
    if (!held) {
        throw return_error<error>{make_error("a null value was read", std::string{codes::null_read})};
    }
    return *std::move(held);
}

/// The HGL error that `failure` carries, where it is an HGL failure or
/// nests one, as hgraph rethrows a node's failure inside an error of its
/// own: the outermost such error, which has the context hgraph gave it.
/// None for any other failure.
[[nodiscard]] std::optional<error> carried_error(const std::exception& failure);

/// Where the HGL failure that `failure` carries or nests was raised, where
/// it was raised by this library and the place survived to here. hgraph
/// gives a failure of a node's `start` or `stop` with the failure nested,
/// and one of its evaluation as a new `ExecutionFailure` with no place, so
/// that one has none.
[[nodiscard]] std::optional<std::source_location> raised_at(const std::exception& failure);

/// `failure` as a person reads it: an HGL failure's code and message, the
/// node, phase and evaluation time hgraph gave it, and the HGL file and line
/// it was raised at, where the generated code set them (`#line`): a place
/// in this library or in the generated code of a test's body is not said;
/// any other failure's own text.
[[nodiscard]] std::string describe(const std::exception& failure);

} // namespace hglrt
