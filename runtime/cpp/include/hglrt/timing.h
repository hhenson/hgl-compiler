// What the generated code of a node with a scheduler or an alarm, and of a
// source with declarations or hooks, needs beside capabilities.h and
// sources.h: whether an input ticked in a node's evaluation, which tells a
// wake by the node's inputs from one by its timer (the language's TIME-3 and
// TIME-9), and a capability lent to a source's body for one resumption
// (SOURCE-1, TIME-8).
#pragma once

// The node keeps what its source's body reaches in a `std::unique_ptr`,
// where it does not move.
#include <memory>
#include <optional>
#include <stdexcept>
#include <utility>

namespace hglrt {

// ---- What woke a node (TIME-3, TIME-9) --------------------------------------

/// Whether the temporal input `input` ticked in the current evaluation: it
/// published, or it lost its value (END-4: "The transition to invalid
/// notifies active subscribers"), as hgraph's view says, whose `modified` is
/// of a publication alone. An evaluation of a node with an alarm in which no
/// input ticked is the alarm's (TIME-9: `scheduled()` "holds where the node
/// is evaluated and no temporal input has ticked"); in a node with a
/// scheduler or an alarm, a handler that a tick wakes runs only where one did
/// (TIME-3: "Such a wake alone runs no other handler").
template <class E>
bool ticked(const E& input) {
    return input.modified() || input.invalidated();
}

// ---- What a source's body reaches (SOURCE-1, TIME-8) ------------------------

template <class T>
class Lending;

/// A capability of type `T` (`Clock`, `Logger`, `GlobalState`) that a
/// source's body reads where it injects one (SOURCE-1: a source "injects
/// `clock`, `logger`, and `global_state`"). The body is a coroutine that
/// lives from one evaluation of its node to the next, and a capability "is
/// not retained" (TIME-8): its node lends it one for each resumption, made
/// from the view of that evaluation, with a `Lending`, and takes it back
/// after. The body holds a reference to this, which the node keeps where it
/// does not move.
template <class T>
class Lent {
  public:
    /// The capability lent for this resumption. The body runs only while
    /// its node lends it, so one not lent is a fault of the generated code,
    /// and no HGL failure (ERR-1).
    [[nodiscard]] const T& get() const {
        if (!held_.has_value()) {
            throw std::logic_error("hglrt: a source's body read a capability not lent to it");
        }
        return *held_;
    }

  private:
    friend class Lending<T>;
    std::optional<T> held_{};
};

/// Lends `value` to the body through `slot` for as long as this lives, one
/// resumption of the body: it is taken back when this ends, the resumption
/// failing included.
template <class T>
class [[nodiscard]] Lending {
  public:
    Lending(Lent<T>& slot, T value) : slot_(slot) { slot_.held_.emplace(std::move(value)); }
    ~Lending() { slot_.held_.reset(); }

    Lending(const Lending&) = delete;
    Lending& operator=(const Lending&) = delete;
    Lending(Lending&&) = delete;
    Lending& operator=(Lending&&) = delete;

  private:
    Lent<T>& slot_;
};

} // namespace hglrt
