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
                     hgraph::State<hgraph::Int> prior_validity,
                     hgraph::Out<Keys> result) {
        const hgraph::TSSInputView& left = left_input;
        const hgraph::TSSInputView& right = right_input;
        const hgraph::TSSOutputView& out = result;
        const bool left_valid = left.valid();
        const bool right_valid = right.valid();
        const hgraph::Int validity = (left_valid ? 1 : 0) | (right_valid ? 2 : 0);
        const auto included = [&](const hgraph::ValueView& key) {
            const bool in_left = left_valid && left.contains(key);
            const bool in_right = right_valid && right.contains(key);
            return Outer ? in_left || in_right : in_left && in_right;
        };
        const auto now = out.evaluation_time();
        const bool initial = !out.valid();
        auto output = out.begin_mutation(now);
        const auto refresh = [&](const hgraph::ValueView& key) {
            const bool present = output.contains(key);
            if (included(key)) {
                if (!present) {
                    static_cast<void>(output.add(key));
                }
            } else if (present) {
                static_cast<void>(output.remove(key));
            }
        };

        // A validity change or sampled binding can replace all input keys.
        if (initial || validity != prior_validity.get() || left.base().delta_is_sampled_rebind()
            || right.base().delta_is_sampled_rebind()) {
            prior_validity.set(validity);
            // Snapshot removals before mutating the set being traversed.
            std::vector<hgraph::Value> removed;
            for (const auto& key : out.values()) {
                if (!included(key)) {
                    removed.emplace_back(key);
                }
            }
            for (const auto& key : removed) {
                static_cast<void>(output.remove(key.view()));
            }
            if (left_valid) {
                for (const auto& key : left.values()) {
                    refresh(key);
                }
            }
            if constexpr (Outer) {
                if (right_valid) {
                    for (const auto& key : right.values()) {
                        refresh(key);
                    }
                }
            }
        } else {
            // The source's sparse cursors visit only this cycle's membership
            // changes, including removals that still retain their keys.
            const auto apply_changes = [&](const hgraph::TSSInputView& input) {
                if (!input.valid() || !input.modified()) {
                    return;
                }
                const auto data = input.base().bound_output().data_view().as_set();
                if (!data.modified(now)) {
                    return;
                }
                for (auto slot = data.next_added_slot(); slot != hgraph::TS_DATA_NO_CHILD_ID;
                     slot = data.next_added_slot(slot)) {
                    refresh(data.at_slot(slot));
                }
                for (auto slot = data.next_removed_slot(); slot != hgraph::TS_DATA_NO_CHILD_ID;
                     slot = data.next_removed_slot(slot)) {
                    refresh(data.at_slot(slot));
                }
            };
            apply_changes(left);
            apply_changes(right);
        }
        if (initial) {
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
