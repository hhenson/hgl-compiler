// A temporal error boundary, `capture(callable, args...)` (the language's
// ERR-7 and ERR-8), and the temporal values of what HGL's errors are: an
// `error` and a `result<T, error>` (ERR-2, ERR-4), with the name, the text
// and the hash the library gives each (`value_form`, values.h), by which a
// trace of either is compared and written, and a replay of one is
// configuration hgraph can intern.
//
// ERR-7: "In composition, `capture(callable, args...)` creates a temporal
// error boundary ... read through two connections: `.value` retains the
// normal output type; outputless calls expose only `.error`, a connection
// of `error`". The compiler writes the call a capture covers as a graph of
// its own, `G`, whose `compose` takes the call's connections as ports and
// its other arguments as scalars, and calls the callable with them. hgraph's
// `try_except_` wires that graph as the child of one node, with canonical
// errors: the child's output is forwarded to the field `value` of the
// node's output, and a failure of the child's evaluation is published, as
// hgraph's `CanonicalError`, to the field `error`, or to the whole output
// of a capture of a call with no result. What hgraph does is what ERR-7 and
// ERR-8 ask:
//
//   - "Each evaluation failure ticks its canonical error; before failure
//     the stream is invalid, and later success leaves its retained error
//     valid but unmodified": the error is published in the cycle of the
//     failure, and only then.
//   - "Capture ends the failed pass without rollback or immediate retry.
//     The child remains live for future ordinary work ... Earlier writes
//     remain visible": the child's evaluation stops where it failed, and
//     what its nodes changed stays changed.
//   - "Start/stop failures escape": a failure of a child's `start` or
//     `stop` is not caught, and fails the run.
//   - ERR-4: "On first graph failure, attach original node identity,
//     phase, and evaluation time; propagation/capture preserve them": the
//     error keeps the context it has, and is given the failing node's
//     where it has none.
//
// `try_except_` is declared by `<hgraph/types/subgraph_wiring.h>`, which
// brings hgraph's full wiring front end (`graph_wiring.h`) with it: the
// front end for generated code (`generated.h`) has no `try_except_`, as it
// has `compiled::switch_` and `compiled::map_` (hgraph issue 1871, which
// asks for it there; this include goes when it is). Nothing of hgraph's
// standard library comes with it.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/traces.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>
// hgraph issue 1871: `try_except_` is not in the front end for generated
// code; until it is, its header and the full front end with it.
#include <hgraph/types/subgraph_wiring.h>

#include <cstddef>
#include <expected>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

/// What a capture of a call whose output is of the hgraph schema `S` gives:
/// `.value`, the call's own output, and `.error`, a connection of `error`
/// (ERR-7), as hgraph's `try_except_` names them.
template <class S>
using captured = hgraph::UnNamedTSB<hgraph::Field<"value", S>, hgraph::Field<"error", hgraph::TS<error>>>;

/// How every capture asks hgraph to capture: with canonical errors, so that
/// what the boundary publishes is HGL's `error` (ERR-4), and not hgraph's
/// `NodeError`.
inline constexpr hgraph::ErrorCaptureOptions capture_options{.canonical_errors = true};

/// `capture(callable, args...)` (ERR-7) of the graph `G` the compiler
/// writes for the call: `args` are what `G::compose` takes, the call's
/// connections as ports and its other arguments as values, in the order
/// written. It gives a port of `captured<S>` where `G` gives a port of `S`,
/// and a port of `error` where `G` gives nothing: "outputless calls expose
/// only `.error`".
template <class G, class... Args>
auto capture(hgraph::Wiring& wiring, const Args&... args) {
    using output = typename hgraph::StaticGraphSignature<G>::output_type;
    const hgraph::Port<void> boundary = hgraph::try_except_<G>(wiring, capture_options, args...);
    if constexpr (std::is_void_v<output>) {
        return boundary.template as<hgraph::TS<error>>();
    } else {
        return boundary.template as<captured<typename output::schema>>();
    }
}

namespace detail {
/// The port of the field `index` of the capture `boundary`, which is a node's
/// output: the node, and the field's position after the output's own path.
template <class Field, class S>
hgraph::Port<Field> captured_field(const hgraph::Port<captured<S>>& boundary, std::size_t index) {
    std::vector<std::size_t> path = boundary.path();
    path.push_back(index);
    return hgraph::Port<Field>{boundary.checked_wiring(), boundary.node(), std::move(path)};
}
} // namespace detail

/// `seen.value` of the capture `boundary` (ERR-7: "`.value` retains the
/// normal output type"): what the call publishes, as it publishes it. "A
/// result the callable returns as data, an Err included, is its `.value`
/// and no failure."
template <class S>
hgraph::Port<S> captured_value(const hgraph::Port<captured<S>>& boundary) {
    return detail::captured_field<S>(boundary, 0);
}

/// `seen.error` of the capture `boundary` (ERR-7): a connection of `error`,
/// which ticks with each failure of an evaluation the boundary covers.
template <class S>
hgraph::Port<hgraph::TS<error>> captured_error(const hgraph::Port<captured<S>>& boundary) {
    return detail::captured_field<hgraph::TS<error>>(boundary, 1);
}

/// `seen.error` of a capture of a call with no result, whose boundary is the
/// connection of `error` itself.
inline hgraph::Port<hgraph::TS<error>> captured_error(const hgraph::Port<hgraph::TS<error>>& boundary) {
    return boundary;
}

/// An `error`'s name and text (ERR-4), as the library names a type and
/// writes a value (`value_form`, values.h): `error(message: "m", code:
/// "c")`, its code where it has one. Its context is no part of its text, as
/// it is none of its equality ("Equality excludes provenance").
template <>
struct value_form<error> {
    static std::string name() { return "error"; }
    /// The hash of its code and its message, which its equality compares.
    static std::size_t hash(const error& value) {
        return detail::combined(hash_of(value.code), hash_of(value.message));
    }
    static std::string text(const error& value) {
        std::string text = "error(message: " + testing::text_of(value.message);
        if (value.code.has_value()) {
            text += ", code: " + testing::text_of(*value.code);
        }
        return text + ")";
    }
};

/// A result's name and text (ERR-2): `result<i64, error>`; `ok(2)`,
/// `err(error(message: "m"))`.
template <class V, class E>
struct value_form<std::expected<V, E>> {
    static std::string name() { return "result<" + type_name<V>() + ", " + type_name<E>() + ">"; }
    /// The hash of its tag and its payload, which its equality compares.
    static std::size_t hash(const std::expected<V, E>& value) {
        return value.has_value() ? detail::combined(1, hash_of(*value))
                                 : detail::combined(2, hash_of(value.error()));
    }
    static std::string text(const std::expected<V, E>& value) {
        if (value.has_value()) {
            return "ok(" + testing::text_of(*value) + ")";
        }
        return "err(" + testing::text_of(value.error()) + ")";
    }
};

} // namespace hglrt

/// A result (ERR-2: "Temporal outcomes publish atomically, including
/// repeats") is registered with hgraph as an atomic, under `hgl.` and its
/// HGL type's name, the first time hgraph asks for its schema: hgraph holds
/// it whole and compares it with its own equality, which compares the tag,
/// then the payload. An `error` is hgraph's `CanonicalError`, which hgraph
/// registers itself.
template <class V, class E>
struct hgraph::scalar_descriptor<std::expected<V, E>> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        static const std::string name = "hgl." + hglrt::type_name<std::expected<V, E>>();
        return TypeRegistry::instance().register_scalar<std::expected<V, E>>(name);
    }
};
