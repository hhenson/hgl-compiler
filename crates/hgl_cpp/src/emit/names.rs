//! Writes every C++ name the generated code gives: modules' namespaces,
//! declarations, tests, parameters and locals, and the names of the
//! runtime library's natives. Part of the transpiler, the last stage.
//!
//! An HGL name becomes a C++ one through [`identifier`] alone, which
//! gives no two HGL names one C++ name, never a name the C++ standard
//! keeps, and the same name on every run. A macro of a platform's
//! headers cannot take one: each file `#undef`s the plain names it
//! writes ([`program_names`]). The target's own names, the namespaces `m_` and `t_`,
//! have a form [`identifier`] gives no HGL name.

use std::collections::BTreeSet;

use hgl_analyser::{DeclarationKind, Detail, Program, Signature, Type};

/// The words the C++ standard keeps: its keywords and alternative
/// tokens, the identifiers it gives a meaning in some places (`final`,
/// `module`, `pre`), its attribute tokens, and the lowercase macros its
/// own library defines (`errno`, `assert`), in the order of the
/// alphabet. No plain identifier is one of them: a file may not
/// `#undef` any of them ([macro.names]). The macros one platform's
/// headers define besides are not here: each file `#undef`s the plain
/// names it writes after its last `#include` (`assemble`), and units
/// are compiled as strict C++23, in which the compiler predefines none
/// (`linux`) and keeps no GNU keyword (`typeof`).
const RESERVED: &[&str] = &[
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "assert",
    "assume",
    "auto",
    "bitand",
    "bitor",
    "bool",
    "break",
    "carries_dependency",
    "case",
    "catch",
    "char",
    "char16_t",
    "char32_t",
    "char8_t",
    "class",
    "co_await",
    "co_return",
    "co_yield",
    "compl",
    "concept",
    "const",
    "const_cast",
    "consteval",
    "constexpr",
    "constinit",
    "continue",
    "contract_assert",
    "decltype",
    "default",
    "delete",
    "deprecated",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "errno",
    "explicit",
    "export",
    "extern",
    "fallthrough",
    "false",
    "final",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "import",
    "indeterminate",
    "inline",
    "int",
    "likely",
    "long",
    "math_errhandling",
    "maybe_unused",
    "module",
    "mutable",
    "namespace",
    "new",
    "no_unique_address",
    "nodiscard",
    "noexcept",
    "noreturn",
    "not",
    "not_eq",
    "nullptr",
    "offsetof",
    "operator",
    "or",
    "or_eq",
    "override",
    "post",
    "pre",
    "private",
    "protected",
    "public",
    "register",
    "reinterpret_cast",
    "requires",
    "return",
    "setjmp",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "stderr",
    "stdin",
    "stdout",
    "struct",
    "switch",
    "template",
    "this",
    "thread_local",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unlikely",
    "unsigned",
    "using",
    "va_arg",
    "va_copy",
    "va_end",
    "va_start",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "while",
    "xor",
    "xor_eq",
];

/// An HGL name (LEX-1: an ASCII letter or `_`, then letters, digits and
/// `_`) as a C++ identifier.
///
/// A name is written as it is where it is *plain*: it starts with a
/// letter, ends with a letter or a digit, has no `__`, holds a
/// lowercase letter, and is none of [`RESERVED`]. Every other name is
/// written with each `_` as `_1`, and `_` after it: `_kept` is
/// `_1kept_`, `EOF` is `EOF_`, `int` is `int_`, `add_` is `add_1_`.
///
/// No two names give one identifier: a plain name never ends with `_`
/// and every other one does, and `_1` gives back the `_` it stands for.
/// No identifier has `__` in it, which C++ reserves; one that starts
/// with `_` is written only inside the target's namespaces, where C++
/// reserves no such name. A name with no lowercase letter is no plain
/// one, since a macro's name is written so (`EOF`, `NULL`). The
/// target's own names, `m_` and `t_`, have the form `identifier` gives
/// `m` and `t` where they were not plain; they are, so no name gives
/// either.
pub(crate) fn identifier(written: &str) -> String {
    let plain = written.starts_with(|first: char| first.is_ascii_alphabetic())
        && !written.ends_with('_')
        && !written.contains("__")
        && written
            .chars()
            .any(|character| character.is_ascii_lowercase())
        && !RESERVED.contains(&written);
    if plain {
        written.to_string()
    } else {
        format!("{}_", written.replace('_', "_1"))
    }
}

/// The C++ namespace module `path` is written in, qualified from the
/// global one: `demo.calc` is `::hgl::demo::calc::m_`. Each part of the
/// path is an [`identifier`], and the module's own declarations are in
/// `m_`, apart from the modules under it: a declaration `calc` of
/// module `demo` is `::hgl::demo::m_::calc`, and module `demo.calc` is
/// `::hgl::demo::calc::m_`.
pub(crate) fn module_namespace(path: &str) -> String {
    let parts: Vec<String> = path.split('.').map(identifier).collect();
    format!("::hgl::{}::m_", parts.join("::"))
}

/// The C++ namespace the named tests of module `path` are written in,
/// qualified from the global one: `::hgl::demo::calc::m_::t_`. A test
/// and a declaration may have one name (TST-1 keeps the two apart).
pub(crate) fn test_namespace(path: &str) -> String {
    format!("{}::t_", module_namespace(path))
}

/// A type as part of a native function's name: `i64` as it is, and any
/// other character than a letter or a digit as `_`. The runtime
/// library's natives are named so, as the hgraph target names its
/// provider's.
fn mangled(ty: &Type) -> String {
    ty.to_string()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// The name the runtime library gives a native candidate of core
/// (NATIVE-7): the candidate's name, then the types of its parameters,
/// each after a `_`. `add_(i64, i64)` is `add__i64_i64`.
pub(super) fn native_candidate(name: &str, signature: &Signature) -> String {
    let types: Vec<String> = signature
        .parameters
        .iter()
        .map(|parameter| mangled(&parameter.ty))
        .collect();
    format!("{name}_{}", types.join("_"))
}

/// The name the runtime library gives a native function of core
/// (NATIVE-7): `fn_` and its name, `fn_log`.
pub(super) fn native_function(name: &str) -> String {
    format!("fn_{name}")
}

/// A native as the generated code calls it, qualified from the global
/// namespace: `::hglrt::add__i64_i64`.
pub(super) fn native(name: &str) -> String {
    format!("::{}::{name}", crate::runtime::NAMESPACE)
}

/// A name of the runtime library's test host, qualified from the global
/// namespace: `::hglrt::testing::check`.
pub(crate) fn testing(name: &str) -> String {
    format!("::{}::{name}", crate::runtime::TESTING)
}

/// Where a function of the program is defined: the namespace it is in,
/// under its module's (`module_namespace`), and its name there. A value
/// function is named `identifier` gives its name, `total`. An HGL
/// candidate of an operator is in `c_`, where the candidates of one
/// operator are C++ overloads of its name, `c_::add_1_` (GEN-4). An
/// instance of a generic declaration (GEN-2) is named by its suffix in a
/// namespace of its declaration's own under `i_`: `i_::spread::of_f64`,
/// `c_::i_::add_1_::of_Weekdays`. `c_` and `i_` have the form the target's
/// own names have, which no HGL name gives.
pub(super) fn placed(
    program: &Program,
    module: usize,
    declaration: usize,
    instance: Option<&str>,
) -> (Option<String>, String) {
    let declared = &program.modules[module].declarations[declaration];
    let candidate = matches!(declared.kind, DeclarationKind::Implementation(..));
    let name = identifier(&declared.name);
    match (candidate, instance) {
        (false, None) => (None, name),
        (true, None) => (Some("c_".to_string()), name),
        (false, Some(suffix)) => (Some(format!("i_::{name}")), instance_name(suffix)),
        (true, Some(suffix)) => (Some(format!("c_::i_::{name}")), instance_name(suffix)),
    }
}

/// The name of the instance whose suffix is `suffix` (`hgl_lower`'s:
/// its generic arguments, each character that is no letter or digit as
/// `_`): `of_` and the suffix, as [`identifier`] writes it, `of_f64`,
/// `of_3`. A suffix may start with a digit, which no C++ name does.
fn instance_name(suffix: &str) -> String {
    identifier(&format!("of_{suffix}"))
}

/// A function of the program, or an instance of one, as a call names
/// it, qualified from the global namespace:
/// `::hgl::demo::calc::m_::total`, `::hgl::demo::m_::i_::spread::f64`
/// ([`placed`]). A constant is named as a function of its value is.
pub(super) fn function(
    program: &Program,
    module: usize,
    declaration: usize,
    instance: Option<&str>,
) -> String {
    let (namespace, name) = placed(program, module, declaration, instance);
    let module = module_namespace(&program.modules[module].path);
    match namespace {
        Some(namespace) => format!("{module}::{namespace}::{name}"),
        None => format!("{module}::{name}"),
    }
}

/// A name the generated code makes for what it holds while a statement
/// runs, `word` and its number in the piece: `argument_00_`,
/// `propagated_01_`. Its number is written after a `0`, a form no name
/// [`identifier`] gives has: a name it escapes has `_1` for each `_` but
/// the last, and a plain one does not end with `_`.
pub(super) fn temporary(word: &str, number: usize) -> String {
    format!("{word}_0{number}_")
}

/// A string as a C++ narrow string literal, for text the target writes
/// itself: a test's identity.
pub(crate) fn string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            printable if printable.is_ascii_graphic() || printable == ' ' => out.push(printable),
            // Any other character as its UTF-8 bytes, each in octal,
            // which no character after it can extend.
            other => {
                let mut bytes = [0; 4];
                for byte in other.encode_utf8(&mut bytes).bytes() {
                    out.push_str(&format!("\\{byte:03o}"));
                }
            }
        }
    }
    out.push('"');
    out
}

/// Every name the program gives that [`identifier`] writes as it is: the
/// parts of each module's path, and each declaration, test, enum member,
/// parameter and local. A generated file `#undef`s each of them it writes,
/// after its last `#include`, so that no macro a platform's header
/// defines takes one (`assemble`); a macro already expanded in those
/// headers is not changed by it.
pub(crate) fn program_names(program: &Program) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for module in &program.modules {
        names.extend(module.path.split('.').map(str::to_string));
        names.extend(
            module
                .declarations
                .iter()
                .map(|declared| declared.name.clone()),
        );
        names.extend(module.tests.iter().map(|test| test.name.clone()));
        for detail in &module.details {
            match detail {
                Detail::Enum(read) => {
                    names.extend(read.members.iter().map(|member| member.name.clone()));
                }
                Detail::Callable(signature) => {
                    names.extend(
                        signature
                            .parameters
                            .iter()
                            .map(|parameter| parameter.name.clone()),
                    );
                }
                _ => {}
            }
        }
        for body in module.bodies.iter().chain(&module.test_bodies).flatten() {
            names.extend(body.locals.iter().map(|local| local.name.clone()));
        }
    }
    names.retain(|name| identifier(name) == *name);
    names
}

#[cfg(test)]
mod tests {
    use super::{identifier, module_namespace, string};

    /// A plain name is written as it is.
    #[test]
    fn a_plain_name_is_kept() {
        for name in ["total", "count_publications", "x1", "camelCase", "m", "t"] {
            assert_eq!(identifier(name), name);
        }
    }

    /// Every other name ends with `_`, has each `_` as `_1`, and has no
    /// `__`.
    #[test]
    fn a_name_that_is_not_plain_is_escaped() {
        assert_eq!(identifier("_kept"), "_1kept_");
        assert_eq!(identifier("add_"), "add_1_");
        assert_eq!(identifier("a__b"), "a_1_1b_");
        assert_eq!(identifier("int"), "int_");
        assert_eq!(identifier("errno"), "errno_");
        assert_eq!(identifier("likely"), "likely_");
        assert_eq!(identifier("EOF"), "EOF_");
        assert_eq!(identifier("N"), "N_");
        assert_eq!(identifier("MAX_SIZE"), "MAX_1SIZE_");
        for name in ["_kept", "add_", "a__b", "int", "EOF", "_", "__", "_1_"] {
            assert!(!identifier(name).contains("__"), "{name}");
        }
    }

    /// A macro of one platform's headers or a GNU keyword is a plain name:
    /// a generated file `#undef`s it, and units are strict C++23.
    #[test]
    fn a_platform_word_is_a_plain_name() {
        for name in [
            "linux",
            "typeof",
            "issubnormal",
            "htobe16",
            "min",
            "interface",
        ] {
            assert_eq!(identifier(name), name);
        }
    }

    /// No two names give one identifier, the target's own `m_` and `t_`
    /// among them.
    #[test]
    fn no_two_names_give_one_identifier() {
        let names = [
            "a", "a_", "a_1", "a_1_", "a__", "_a", "a__1", "m", "m_", "t", "t_", "int", "int_",
            "N", "N_", "_1", "__1", "_", "x_y", "x_1y", "x__y",
        ];
        let mut given: Vec<String> = names.iter().map(|name| identifier(name)).collect();
        given.push("m_".to_string());
        given.push("t_".to_string());
        let mut unique = given.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), given.len(), "{given:?}");
    }

    /// A module's namespace is under `hgl`, with its own declarations in
    /// `m_`.
    #[test]
    fn a_module_is_a_namespace_of_its_own() {
        assert_eq!(module_namespace("demo.calc"), "::hgl::demo::calc::m_");
        assert_eq!(module_namespace("a._b"), "::hgl::a::_1b_::m_");
        assert_eq!(module_namespace("a_.b"), "::hgl::a_1_::b::m_");
    }

    #[test]
    fn a_string_is_a_literal() {
        assert_eq!(string("demo.calc::sums"), "\"demo.calc::sums\"");
        assert_eq!(string("a\"b\\c\né"), "\"a\\\"b\\\\c\\n\\303\\251\"");
    }
}
