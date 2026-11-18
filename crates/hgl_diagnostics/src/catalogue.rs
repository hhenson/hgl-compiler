//! The specification's diagnostic catalogue, as data.
//!
//! `errors-and-diagnostics.md` lists every source rejection that has a
//! stable code: its category, its code, and what it means. A code in that
//! list is *catalogued*. Only a catalogued category and code may be named
//! by an `expect-error` annotation (TST-12), so the test runner looks an
//! annotation up here.
//!
//! The list is written out below and not read from the specification when
//! the compiler runs. `tests/catalogue.rs` compares it with the pinned
//! chapter, so raising the pin shows which rows moved.

use crate::Category;

/// Every source row of the catalogue: its category and its code, in the
/// specification's order.
///
/// The catalogue's execution codes, such as `value.missing_key`, name
/// failures of a running program. They are a different list and are not
/// here ("The source and execution catalogs are disjoint").
pub const SOURCE_CODES: &[(Category, &str)] = &[
    (Category::Parse, "syntax.expected_token"),
    (Category::Parse, "syntax.invalid_encoding"),
    (Category::Parse, "syntax.unexpected_character"),
    (Category::Parse, "string.unclosed"),
    (Category::Parse, "string.unknown_escape"),
    (Category::Parse, "string.invalid_escape"),
    (Category::Parse, "string.malformed_text_block"),
    (Category::Parse, "comment.unclosed"),
    (Category::Parse, "number.malformed"),
    (Category::Parse, "number.integer_range"),
    (Category::Parse, "number.float_range"),
    (Category::Parse, "duration.invalid"),
    (Category::Parse, "tenor.invalid"),
    (Category::Parse, "date_time.malformed"),
    (Category::Parse, "date_time.invalid_field"),
    (Category::Parse, "date_time.invalid_zone"),
    (Category::Parse, "doc.unattached"),
    (Category::Parse, "doc.repeated"),
    (Category::Module, "module.duplicate_unnamed"),
    (Category::Module, "module.duplicate_part"),
    (Category::Module, "import.misplaced"),
    (Category::Module, "import.unknown_module"),
    (Category::Name, "import.unknown_name"),
    (Category::Module, "import.private"),
    (Category::Module, "import.cycle"),
    (Category::Module, "application.duplicate_path"),
    (Category::Module, "application.import"),
    (Category::Module, "application.export"),
    (Category::Module, "application.run_count"),
    (Category::Phase, "run.position"),
    (Category::Type, "run.entry"),
    (Category::Name, "name.duplicate"),
    (Category::Name, "name.used_before_declaration"),
    (Category::Name, "constant.cycle"),
    (Category::Name, "name.test_only"),
    (Category::Name, "name.captured"),
    (Category::Name, "default.formal_argument"),
    (Category::Name, "alias.unqualified"),
    (Category::Name, "alias.unknown"),
    (Category::Name, "test.duplicate_name"),
    (Category::Module, "test.export"),
    (Category::Operator, "test.implementation"),
    (Category::Operator, "impl.unknown_operator"),
    (Category::Operator, "impl.option_kind"),
    (Category::Name, "native.missing_declaration"),
    (Category::Phase, "native.const_hook"),
    (Category::Phase, "native.hook_without_when"),
    (Category::Phase, "native.duplicate_hook"),
    (Category::Injectable, "inject.unknown_capability"),
    (Category::Injectable, "inject.out_without_result"),
    (Category::Injectable, "capability.as_value"),
    (Category::Injectable, "inject.alarm_and_scheduler"),
    (Category::Injectable, "inject.no_scheduler"),
    (Category::Injectable, "store.reserved_key"),
    (Category::Phase, "node.nested"),
    (Category::Phase, "node.in_value_function"),
    (Category::Phase, "node.declaration_order"),
    (Category::Phase, "node.duplicate_hook"),
    (Category::Phase, "node.statement"),
    (Category::Phase, "source.statement"),
    (Category::Phase, "source.when"),
    (Category::Phase, "source.input"),
    (Category::Phase, "state.input_read"),
    (Category::Phase, "out.lifecycle"),
    (Category::Phase, "input.lifecycle"),
    (Category::Phase, "payload.unguarded"),
    (Category::Phase, "endpoint.argument"),
    (Category::Phase, "connection.as_constant"),
    (Category::Type, "generic.invalid_application"),
    (Category::Type, "pack.default"),
    (Category::Type, "pack.const"),
    (Category::Type, "pack.order"),
    (Category::Type, "pack.undeclared"),
    (Category::Type, "pack.cardinality"),
    (Category::Type, "type.not_a_type"),
    (Category::Type, "type.not_ordinary"),
    (Category::Type, "signal.position"),
    (Category::Type, "endpoint.position"),
    (Category::Type, "nullable.key"),
    (Category::Type, "generic.argument_count"),
    (Category::Type, "list.size"),
    (Category::Type, "generic.kind_conflict"),
    (Category::Type, "generic.kind_unresolved"),
    (Category::Type, "generic.argument_kind"),
    (Category::Type, "generic.unresolved"),
    (Category::Type, "generic.argument_constant"),
    (Category::Constraint, "requires.unmet"),
    (Category::Type, "struct.parent"),
    (Category::Type, "struct.duplicate_parent"),
    (Category::Type, "struct.inheritance_cycle"),
    (Category::Type, "field.conflict"),
    (Category::Type, "field.not_inherited"),
    (Category::Type, "field.duplicate"),
    (Category::Type, "field.recursive"),
    (Category::Type, "field.null_default"),
    (Category::Type, "enum.value"),
    (Category::Type, "enum.duplicate_value"),
    (Category::Type, "enum.overflow"),
    (Category::Type, "enum.duplicate_member"),
    (Category::Type, "type.mismatch"),
    (Category::Type, "null.context_type"),
    (Category::Type, "sequence.context_type"),
    (Category::Type, "enum.context_type"),
    (Category::Name, "binding.unassigned"),
    (Category::Name, "binding.immutable"),
    (Category::Type, "binding.category"),
    (Category::Type, "argument.order"),
    (Category::Type, "argument.unexpected"),
    (Category::Name, "argument.unknown"),
    (Category::Name, "argument.duplicate"),
    (Category::Type, "argument.missing"),
    (Category::Type, "argument.constant"),
    (Category::Type, "argument.place"),
    (Category::FunctionKind, "parameter.var"),
    (Category::FunctionKind, "call.role"),
    (Category::Type, "return.missing"),
    (Category::Type, "switch.duplicate_label"),
    (Category::Phase, "while.condition"),
    (Category::Phase, "loop.carried"),
    (Category::Type, "struct.abstract"),
    (Category::Type, "call.not_callable"),
    (Category::Name, "member.unknown"),
    (Category::Operator, "operator.no_candidate"),
    (Category::Operator, "operator.ambiguous"),
    (Category::Type, "default.order"),
    (Category::Type, "tuple.index"),
    (Category::Type, "rolling_window.size_kind"),
    (Category::Type, "rolling_window.size_bounds"),
    (Category::Type, "yield.time_type"),
    (Category::Type, "test.raises_code"),
    (Category::Type, "eval.target"),
    (Category::Type, "eval.no_result"),
    (Category::Type, "trace.placeholder"),
    (Category::Type, "trace.style"),
    (Category::Type, "trace.key_order"),
    (Category::Type, "trace.escape"),
    (Category::Type, "nullable.required_presence"),
    (Category::Type, "lookup.default_type"),
    (Category::Type, "result.context_type"),
    (Category::Type, "result.required_variant"),
    (Category::Type, "result.propagation_type"),
    (Category::Phase, "result.propagation_phase"),
    (Category::Phase, "capture.phase"),
    (Category::Type, "capture.output_field"),
    (Category::Type, "value.constant_capability"),
    (Category::Shape, "delta.unsupported_shape"),
    (Category::Type, "delta.type_mismatch"),
    (Category::Name, "delta.argument_name"),
    (Category::Name, "delta.duplicate_argument"),
    (Category::Type, "delta.entry_constant"),
    (Category::Type, "delta.entry_type"),
    (Category::Type, "delta.duplicate_entry"),
    (Category::Type, "delta.index_bounds"),
    (Category::Type, "delta.overlap"),
    (Category::Phase, "test.statement_phase"),
];

/// True when the catalogue has a row with this category and this code.
pub fn contains(category: Category, code: &str) -> bool {
    SOURCE_CODES
        .iter()
        .any(|&(listed_category, listed_code)| listed_category == category && listed_code == code)
}

/// Every execution code of the catalogue, in the specification's
/// order: a failure of a running program that has a stable code. An
/// `assert raises("code")` names one (TST-10). "The source and
/// execution catalogs are disjoint."
pub const EXECUTION_CODES: &[&str] = &[
    "value.missing_key",
    "value.unset_read",
    "value.capability",
    "value.byte_range",
    "numeric.division_by_zero",
    "numeric.out_of_range",
    "tenor.mixed_units",
    "yield.negative_duration",
    "yield.non_increasing_time",
    "eval.input_delta_profile",
    "run.end_and_length",
];

/// True when the catalogue lists `code` as an execution code.
pub fn is_execution(code: &str) -> bool {
    EXECUTION_CODES.contains(&code)
}

/// Every warning of the catalogue, in the specification's order. A
/// warning has a code and no category, and does not reject the source
/// (ERR-1); no `expect-error` annotation names one (ERR-10).
pub const WARNING_CODES: &[&str] = &["node.no_activation", "statement.no_effect"];

/// True when the catalogue lists `code` as a warning.
pub fn is_warning(code: &str) -> bool {
    WARNING_CODES.contains(&code)
}

/// The category the catalogue lists `code` under, or `None` when it
/// lists no such code.
pub fn category_of(code: &str) -> Option<Category> {
    SOURCE_CODES
        .iter()
        .find(|&&(_, listed)| listed == code)
        .map(|&(category, _)| category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_found_by_category_and_code() {
        assert!(contains(Category::Parse, "syntax.expected_token"));
        assert!(contains(Category::Type, "rolling_window.size_kind"));
        // The category is part of the row.
        assert!(!contains(Category::Name, "syntax.expected_token"));
        assert!(!contains(Category::Parse, "syntax.nesting_limit"));
        // An execution code is not a source code.
        assert!(!contains(Category::Type, "value.missing_key"));
        assert_eq!(category_of("capture.phase"), Some(Category::Phase));
        assert_eq!(category_of("value.missing_key"), None);
    }

    #[test]
    fn no_code_is_listed_twice() {
        for (index, (_, code)) in SOURCE_CODES.iter().enumerate() {
            assert!(
                SOURCE_CODES[..index]
                    .iter()
                    .all(|(_, earlier)| earlier != code),
                "`{code}` is listed twice"
            );
        }
    }
}
