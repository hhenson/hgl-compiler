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
    (Category::Parse, "date_time.malformed"),
    (Category::Parse, "date_time.invalid_field"),
    (Category::Parse, "date_time.invalid_zone"),
    (Category::Type, "rolling.size_kind"),
    (Category::Type, "rolling.size_bounds"),
    (Category::Type, "yield.time_type"),
    (Category::Type, "test.raises_code"),
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
        assert!(contains(Category::Type, "rolling.size_kind"));
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
