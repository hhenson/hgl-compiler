// Core's RED-5 ordered reduction of a growing list.
#pragma once

#include <hgraph/types/generated.h>

#include <array>
#include <tuple>
#include <utility>
#include <vector>

namespace hglrt {
namespace ordered_reduce_detail {

template <class Accumulator, class Member>
struct MemberPresent {
    static void eval(
        hgraph::In<"accumulator", Accumulator>,
        hgraph::In<"member", Member, hgraph::InputValidity::Unchecked> member,
        hgraph::Out<hgraph::TS<hgraph::Bool>> out
    ) {
        const auto present = member.valid();
        if (!out.valid() || present != out.value().checked_as<hgraph::Bool>()) {
            out.set(present);
        }
    }
};

template <class Accumulator, class Member>
struct PassAccumulator {
    static hgraph::Port<Accumulator> compose(
        hgraph::Wiring&, hgraph::Port<Accumulator> accumulator, hgraph::Port<Member>
    ) {
        return accumulator;
    }
};

template <class Combiner, class Accumulator, class Member, class... Captures>
struct GuardedCombiner {
    static hgraph::Port<Accumulator> compose(
        hgraph::Wiring& wiring, hgraph::Port<Accumulator> accumulator, hgraph::Port<Member> member,
        hgraph::Scalar<"captures", Captures>... captures
    ) {
        // RED-2: the active member can arrive or be withdrawn. An invalid
        // member selects the accumulator itself and never calls the combiner.
        const auto present = hgraph::wire<MemberPresent<Accumulator, Member>>(wiring, accumulator, member);
        const std::array inputs{accumulator.erased(), member.erased()};
        std::vector<hgraph::SwitchBranchGraph> branches;
        // FN-14: the present branch registers any connections the callback
        // captures. The switch carries those captures into the fold step.
        branches.push_back({hgraph::Value{hgraph::Bool{true}},
                            hgraph::compile_subgraph<Combiner>(wiring, captures.value()...)});
        hgraph::SwitchBranchGraph absent{
            {}, hgraph::compile_subgraph<PassAccumulator<Accumulator, Member>>(wiring)};
        return {wiring, hgraph::compiled::switch_(wiring, present.erased(), inputs, std::move(branches),
                                                 std::move(absent))};
    }
};

} // namespace ordered_reduce_detail

/// RED-5: fold the valid positions of a growing list from a live seed.
template <class Combiner, class Accumulator, class Member, class... Captures>
hgraph::Port<Accumulator> ordered_list_reduce(
    hgraph::Wiring& wiring, hgraph::Port<hgraph::TSL<Member>> items, hgraph::Port<Accumulator> zero,
    const Captures&... captures
) {
    using Step = ordered_reduce_detail::GuardedCombiner<Combiner, Accumulator, Member, Captures...>;
    auto result = hgraph::compiled::reduce_(
        wiring, items.erased(), hgraph::compile_subgraph<Step>(wiring, captures...),
        zero.erased(), hgraph::compiled::ReduceOrder::Ordered
    );
    return {wiring, std::move(result)};
}

} // namespace hglrt
