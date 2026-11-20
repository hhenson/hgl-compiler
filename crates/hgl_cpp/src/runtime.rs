//! The C++ runtime library the generated code links, `libhglrt`, as
//! this target carries it.
//!
//! The library is this repository's `runtime/cpp`, a CMake project. The
//! build script puts every file of it into the crate ([`FILES`]), so
//! that the build driver can write them out and build the library once
//! for each SDK, wherever `hgl` is installed. The emitter reads the
//! library's headers for one thing: which of core's native functions
//! the library defines ([`provides`]), so that a call of one it does not
//! is reported, as any other form the target does not write yet.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Every file of `runtime/cpp`, as its path under that directory, with
/// `/` between its parts, and its bytes, in the order of the paths.
/// Empty where `hgl` was built from a tree without the library.
pub static FILES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/runtime_files.rs"));

/// The header every generated unit includes first, under the library's
/// include directory: the fixed include set, which the build driver
/// precompiles.
pub const HEADER: &str = "hglrt/hglrt.h";

/// The C++ namespace the library defines its names in: core's natives
/// among them.
pub const NAMESPACE: &str = "hglrt";

/// The namespace of the library's test host (`hglrt/testing.h`): a
/// test's `check` and `statement`, each test's `Registration`, and the
/// `main` of an executable of tests.
pub const TESTING: &str = "hglrt::testing";

/// True where `hgl` carries the library's sources.
pub fn available() -> bool {
    !FILES.is_empty()
}

/// True where the library defines the function `name`: core's native
/// functions are named as `names::native` says (NATIVE-7).
///
/// What it defines is read from its headers, the files under
/// `include/`: each name that is followed by `(`. A name the library
/// declares is so; one that only a comment writes with a `(` after it
/// would be taken for one too, and its call would then fail to compile.
pub fn provides(name: &str) -> bool {
    // `LazyLock` works its value out the first time it is read, and
    // keeps it for every read after.
    static DECLARED: LazyLock<HashSet<String>> = LazyLock::new(|| {
        FILES
            .iter()
            .filter(|(path, _)| path.starts_with("include/"))
            .filter_map(|(_, bytes)| std::str::from_utf8(bytes).ok())
            .flat_map(called_names)
            .collect()
    });
    DECLARED.contains(name)
}

/// True where the library declares the enum `name`, `enum class name`,
/// in its own namespace: one of core's enums its natives take, which
/// has core's members in core's order (`ZeroPolicy`, `Rounding`). The
/// generated code names that one, and writes no enum of its own for it.
pub fn declares_enum(name: &str) -> bool {
    static DECLARED: LazyLock<HashSet<String>> = LazyLock::new(|| {
        FILES
            .iter()
            .filter(|(path, _)| path.starts_with("include/"))
            .filter_map(|(_, bytes)| std::str::from_utf8(bytes).ok())
            .flat_map(|text| {
                text.split("enum class ")
                    .skip(1)
                    .filter_map(|after| {
                        let name: String = after
                            .chars()
                            .take_while(|character| {
                                character.is_ascii_alphanumeric() || *character == '_'
                            })
                            .collect();
                        (!name.is_empty()).then_some(name)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    });
    DECLARED.contains(name)
}

/// Every name in `text` that a `(` follows, spaces between allowed.
fn called_names(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let starts = bytes[at].is_ascii_alphabetic() || bytes[at] == b'_';
        let follows = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        if !starts || follows {
            at += 1;
            continue;
        }
        let begin = at;
        while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
            at += 1;
        }
        let mut next = at;
        while next < bytes.len() && bytes[next].is_ascii_whitespace() {
            next += 1;
        }
        if bytes.get(next) == Some(&b'(') {
            names.push(text[begin..at].to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::called_names;

    #[test]
    fn a_declared_function_is_found_by_its_name() {
        let header = "std::int64_t add__i64_i64(std::int64_t lhs, std::int64_t rhs);\n\
                      bool eq__i64_i64_f64_f64 (std::int64_t, std::int64_t, double, double);\n\
                      int x = 3; // not_called";
        let found = called_names(header);
        assert!(found.contains(&"add__i64_i64".to_string()));
        assert!(found.contains(&"eq__i64_i64_f64_f64".to_string()));
        assert!(!found.iter().any(|name| name == "not_called" || name == "x"));
    }
}
