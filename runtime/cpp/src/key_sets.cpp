// Core's SET-1 dictionary key projection and MAP-5 lifecycle key joins.

#include <hglrt/key_sets.h>

#include <hgraph/types/subgraph_wiring.h>

#include <vector>

namespace hglrt::key_sets_detail {

hgraph::WiringPortRef dictionary_keys(const hgraph::WiringPortRef& dictionary) {
    return hgraph::subgraph_wiring_detail::tsd_key_set_ref(dictionary);
}

} // namespace hglrt::key_sets_detail

namespace hglrt {
namespace {

// MAP-5, SET-2: reconcile each key against the current input memberships.
// An invalid input contributes no keys, and an unchanged set stays silent.
template <bool Outer>
struct JoinSplitKeys {
    using Keys = hgraph::TSS<hgraph::ScalarVar<"K">>;
    static constexpr auto name = Outer ? "hgl.map_union_keys" : "hgl.map_intersect_keys";

    static void eval(hgraph::In<"left", Keys, hgraph::InputValidity::Unchecked> left_input,
                     hgraph::In<"right", Keys, hgraph::InputValidity::Unchecked> right_input,
                     hgraph::Out<Keys> result) {
        const hgraph::TSSInputView& left = left_input;
        const hgraph::TSSInputView& right = right_input;
        const hgraph::TSSOutputView& out = result;
        const auto included = [&](const hgraph::ValueView& key) {
            const bool in_left = left.valid() && left.contains(key);
            const bool in_right = right.valid() && right.contains(key);
            return Outer ? in_left || in_right : in_left && in_right;
        };
        // Snapshot removals before mutating the set being traversed.
        std::vector<hgraph::Value> removed;
        for (const auto& key : out.values()) {
            if (!included(key)) {
                removed.emplace_back(key);
            }
        }
        auto output = out.begin_mutation(out.evaluation_time());
        for (const auto& key : removed) {
            static_cast<void>(output.remove(key.view()));
        }
        const auto add_from = [&](const hgraph::TSSInputView& input) {
            if (input.valid()) {
                for (const auto& key : input.values()) {
                    if (!output.contains(key) && included(key)) {
                        static_cast<void>(output.add(key));
                    }
                }
            }
        };
        add_from(left);
        if constexpr (Outer) {
            add_from(right);
        }
        if (!out.valid()) {
            output.touch();
        }
    }
};

} // namespace

hgraph::WiringPortRef joined_map_keys(
    hgraph::Wiring& wiring,
    std::span<const hgraph::WiringPortRef> inputs,
    std::span<const std::size_t> split,
    MapKeyJoin join
) {
    const auto first = key_sets_detail::dictionary_keys(inputs[split.front()]);
    if (join == MapKeyJoin::Left) {
        return first;
    }
    hgraph::Port<void> keys{wiring, first};
    for (auto index : split.subspan(1)) {
        hgraph::Port<void> next{wiring, key_sets_detail::dictionary_keys(inputs[index])};
        keys = join == MapKeyJoin::Outer
            ? hgraph::wire<JoinSplitKeys<true>>(wiring, keys, next)
            : hgraph::wire<JoinSplitKeys<false>>(wiring, keys, next);
    }
    return keys.erased();
}

} // namespace hglrt
