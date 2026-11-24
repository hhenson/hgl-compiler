//! Reads Core's format strings at compile time (FMT-2 to FMT-5,
//! FMT-7 and FMT-9). The formatter consumes the resulting places and
//! typed specifiers.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use hgl_diagnostics::Location;

use crate::hir::{Argument, BinaryOperator, Callee, Expr, ExprKind, PackKey, Value};
use crate::{
    Argument as GenericArgument, ConstantValue, DeclarationKind, Detail, Program, Role, Scalar,
    Signature, Type,
};

/// A literal piece or a place in a Core format string.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// Characters to copy, with a doubled brace already reduced to one.
    Text(&'a str),
    /// A value to render.
    Place(Place<'a>),
}

/// A place and the decoded string offsets used for its diagnostics.
#[derive(Debug, PartialEq, Eq)]
pub struct Place<'a> {
    /// Empty for the next positional argument, otherwise its given name.
    pub name: &'a str,
    /// A leading `*`, where the place asks for a tick prefix (FMT-9).
    pub marked: bool,
    /// The specifier after the colon, if given.
    pub specifier: &'a str,
    /// The full place including its braces.
    pub range: Range<usize>,
    /// The specifier's decoded start, or the closing brace.
    pub specifier_start: usize,
}

/// Split a format string, identifying the first brace that is neither
/// doubled nor part of a place (FMT-2, FMT-3).
pub fn places(mut text: &str) -> Result<Vec<Piece<'_>>, Range<usize>> {
    let mut pieces = Vec::new();
    let mut offset = 0;
    while let Some(brace) = text.find(['{', '}']) {
        let written = &text.as_bytes()[brace..];
        let at = offset + brace;
        if written.get(1) == written.first() {
            pieces.push(Piece::Text(&text[..brace + 1]));
            text = &text[brace + 2..];
            offset += brace + 2;
            continue;
        }
        if written[0] == b'}' {
            return Err(at..at + 1);
        }
        let Some(after) = text[brace + 1..].find(['{', '}']) else {
            return Err(at..at + 1);
        };
        let close = brace + 1 + after;
        if text.as_bytes()[close] != b'}' {
            return Err(at..at + 1);
        }
        let contents = &text[brace + 1..close];
        let (name, specifier) = contents.split_once(':').unwrap_or((contents, ""));
        let marked = name.starts_with('*');
        let name = name.strip_prefix('*').unwrap_or(name);
        if !name.is_empty() && !identifier(name) {
            return Err(at..at + 1);
        }
        if brace != 0 {
            pieces.push(Piece::Text(&text[..brace]));
        }
        let end = offset + close + 1;
        pieces.push(Piece::Place(Place {
            name,
            marked,
            specifier,
            range: at..end,
            specifier_start: end - 1 - specifier.len(),
        }));
        offset = end;
        text = &text[close + 1..];
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

fn identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A chrono specifier, read before the generated program runs (FMT-4).
#[derive(Debug, PartialEq, Eq)]
pub struct Chrono<'a> {
    /// The original fill, alignment and width, applied to the whole field.
    pub padding: &'a str,
    /// The literal text and directives in source order. Empty uses FMT-5.
    pub parts: Vec<ChronoPart<'a>>,
}

/// One component of a chrono specifier (FMT-4).
#[derive(Debug, PartialEq, Eq)]
pub enum ChronoPart<'a> {
    /// Text between directives.
    Text(&'a str),
    /// A standard directive or a square-bracketed duration total.
    Directive {
        /// The directive letter, or `%` for a literal percent.
        letter: char,
        /// The whole duration in this unit.
        total: bool,
        /// Fractional second digits. Zero writes whole seconds.
        precision: u8,
    },
}

/// Validate a fmt 12.1 scalar specifier, with FMT-4's locale restriction.
pub fn scalar_specifier(scalar: Scalar, text: &str) -> bool {
    let rest = alignment(text);
    let signed = rest.starts_with(['+', '-', ' ']);
    let rest = if signed { &rest[1..] } else { rest };
    let alternate = rest.starts_with('#');
    let rest = rest.strip_prefix('#').unwrap_or(rest);
    let zero = rest.starts_with('0');
    let rest = rest.strip_prefix('0').unwrap_or(rest);
    if rest.starts_with('0') {
        return false;
    }
    let Some(rest) = digits(rest) else {
        return false;
    };
    let precision = rest.starts_with('.');
    let rest = if let Some(after) = rest.strip_prefix('.') {
        let Some(remaining) = digits(after).filter(|remaining| remaining.len() != after.len())
        else {
            return false;
        };
        remaining
    } else {
        rest
    };
    match scalar {
        // Hold fmt's byte-oriented `c` pending the FMT-4 decision (#434).
        Scalar::I64 => !precision && matches!(rest, "" | "d" | "x" | "X" | "o" | "b" | "B"),
        Scalar::F64 => matches!(rest, "" | "a" | "A" | "e" | "E" | "f" | "F" | "g" | "G"),
        Scalar::Bool => {
            !signed && !precision && matches!(rest, "" | "s" | "d" | "x" | "X" | "o" | "b" | "B")
        }
        Scalar::Str => !(signed || alternate || zero) && matches!(rest, "" | "s" | "?"),
        _ => false,
    }
}

/// Read the chrono directives admitted by the value's type (FMT-4).
pub fn chrono_specifier(scalar: Scalar, text: &str) -> Option<Chrono<'_>> {
    let aligned = alignment(text);
    if aligned.starts_with('0') {
        return None;
    }
    let mut rest = digits(aligned)?;
    let padding = &text[..text.len() - rest.len()];
    if !rest.is_empty() && !rest.starts_with('%') {
        return None;
    }
    let time = matches!(scalar, Scalar::Time | Scalar::Datetime | Scalar::Duration);
    let date = matches!(scalar, Scalar::Date | Scalar::Datetime);
    let duration = scalar == Scalar::Duration;
    let mut parts = Vec::new();
    while let Some(percent) = rest.find('%') {
        if percent != 0 {
            parts.push(ChronoPart::Text(&rest[..percent]));
        }
        rest = &rest[percent + 1..];
        let (letter, total) = if let Some(bracketed) = rest.strip_prefix('[') {
            let (unit, after) = bracketed.split_once(']')?;
            if !duration || !matches!(unit, "H" | "M" | "S") {
                return None;
            }
            rest = after;
            (unit.as_bytes()[0] as char, true)
        } else {
            let letter = rest.chars().next()?;
            rest = &rest[letter.len_utf8()..];
            let admitted = match letter {
                'Y' | 'y' | 'm' | 'b' | 'B' | 'd' | 'e' | 'a' | 'A' | 'F' | 'D' => date,
                'j' => date || duration,
                'H' | 'I' | 'p' | 'M' | 'S' | 'R' | 'T' => time,
                '%' => true,
                _ => false,
            };
            if !admitted {
                return None;
            }
            (letter, false)
        };
        let mut precision = 0;
        if matches!(letter, 'S' | 'T')
            && let Some((&b'.', after)) = rest.as_bytes().split_first()
            && let Some(&digit) = after.first().filter(|digit| digit.is_ascii_digit())
        {
            if !matches!(digit, b'0' | b'3' | b'6' | b'9')
                || after.get(1).is_some_and(u8::is_ascii_digit)
            {
                return None;
            }
            precision = digit - b'0';
            rest = &rest[2..];
        }
        parts.push(ChronoPart::Directive {
            letter,
            total,
            precision,
        });
    }
    if !rest.is_empty() {
        parts.push(ChronoPart::Text(rest));
    }
    Some(Chrono { padding, parts })
}

fn alignment(text: &str) -> &str {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return text;
    };
    if chars
        .next()
        .is_some_and(|next| matches!(next, '<' | '^' | '>'))
    {
        &text[first.len_utf8() + 1..]
    } else if matches!(first, '<' | '^' | '>') {
        &text[1..]
    } else {
        text
    }
}

/// Widths and precisions have the pinned library's positive int range.
fn digits(text: &str) -> Option<&str> {
    let count = text.bytes().take_while(u8::is_ascii_digit).count();
    if count != 0 {
        text[..count].parse::<i32>().ok()?;
    }
    Some(&text[count..])
}

/// The signature of Core's native value candidate, when it is selected.
pub fn value_signature<'a>(program: &'a Program, callee: &Callee) -> Option<&'a Signature> {
    let (module, declaration) = callee.declaration()?;
    let owner = &program.modules[module];
    let declared = &owner.declarations[declaration];
    if Some(module) != program.core()
        || declared.name != "format"
        || !matches!(
            declared.kind,
            DeclarationKind::NativeCandidate(Role::Value, _)
        )
    {
        return None;
    }
    match &owner.details[declaration] {
        Detail::Callable(signature) => Some(signature),
        _ => None,
    }
}

/// An argument supplied to the requested declaration slot, excluding packs.
pub fn argument<'a>(
    signature: &Signature,
    arguments: &'a [Argument],
    name: &str,
) -> Option<&'a Argument> {
    let position = signature
        .parameters
        .iter()
        .position(|parameter| parameter.name == name)?;
    arguments
        .iter()
        .find(|argument| argument.parameter == Some(position) && argument.pack.is_none())
}

/// The index of the written argument that fills one place (FMT-2).
pub fn argument_index(arguments: &[Argument], name: &str, position: usize) -> Option<usize> {
    arguments.iter().position(|argument| match &argument.pack {
        Some(PackKey::Position(index)) => name.is_empty() && *index == position,
        Some(PackKey::Name(named)) => name == named,
        None => false,
    })
}

/// The payload type of Core's `Marked<T>` (FMT-7).
pub fn marked_payload<'a>(program: &Program, ty: &'a Type) -> Option<&'a Type> {
    let Type::Named {
        module,
        declaration,
        arguments,
        ..
    } = ty
    else {
        return None;
    };
    if Some(*module) != program.core() {
        return None;
    }
    let declared = &program.modules[*module].declarations[*declaration];
    match arguments.as_slice() {
        [GenericArgument::Type(ty)]
            if declared.kind == DeclarationKind::Struct && declared.name == "Marked" =>
        {
            Some(ty)
        }
        _ => None,
    }
}

/// Why the compiler has yet to read a format pattern.
#[derive(Clone, Copy)]
pub(crate) enum PatternFault {
    /// An expression outside the compile-time reader's supported forms.
    Unread,
    /// The pattern would exceed the reader's budget.
    Limit,
}

pub(crate) type PatternText = Result<Rc<str>, PatternFault>;

pub(crate) const PATTERN_DECLARATIONS: usize = 64;
const PATTERN_BYTES: usize = 64 * 1024;

pub(crate) fn append_pattern(pattern: &mut String, text: &str) -> Result<(), PatternFault> {
    if text.len() > PATTERN_BYTES - pattern.len() {
        return Err(PatternFault::Limit);
    }
    pattern.push_str(text);
    Ok(())
}

/// One read per declaration in this fold. Copies of a named result
/// share the same text, and a recursive reference finds an unread value.
#[derive(Default)]
pub(crate) struct PatternMemo(HashMap<(usize, usize), PatternText>);

impl PatternMemo {
    pub(crate) fn declaration(
        &mut self,
        key: (usize, usize),
        remaining: usize,
        read: impl FnOnce(&mut Self, usize) -> PatternText,
    ) -> PatternText {
        if let Some(result) = self.0.get(&key) {
            return result.clone();
        }
        let next = remaining.checked_sub(1).ok_or(PatternFault::Limit)?;
        self.0.insert(key, Err(PatternFault::Unread));
        let result = read(self, next);
        self.0.insert(key, result.clone());
        result
    }
}

/// A pattern known in the typed program, including its bound generic
/// values and additions of constant strings (FMT-1).
pub(crate) fn constant_pattern_in(
    program: &Program,
    value: &Expr,
    instance: &[(Location, GenericArgument)],
) -> PatternText {
    use PatternFault::Unread;

    fn read(
        program: &Program,
        value: &Expr,
        instance: &[(Location, GenericArgument)],
        memo: &mut PatternMemo,
        remaining: usize,
    ) -> PatternText {
        let mut text = String::new();
        let mut pending = vec![value];
        // Binary operands do not extend the chain of declarations.
        while let Some(value) = pending.pop() {
            match &value.kind {
                ExprKind::Literal(Value::Str(value)) => append_pattern(&mut text, value)?,
                ExprKind::GenericValue { at, .. } => {
                    let Some((_, GenericArgument::Constant(given))) =
                        instance.iter().find(|(parameter, _)| parameter == at)
                    else {
                        return Err(Unread);
                    };
                    let ConstantValue::Str(value) = &given.value else {
                        return Err(Unread);
                    };
                    append_pattern(&mut text, value)?;
                }
                ExprKind::Constant {
                    module,
                    declaration,
                } => {
                    let value =
                        memo.declaration((*module, *declaration), remaining, |memo, next| {
                            let defined = program.modules[*module]
                                .constants
                                .get(*declaration)
                                .and_then(Option::as_ref)
                                .ok_or(Unread)?;
                            read(program, &defined.value, instance, memo, next)
                        })?;
                    append_pattern(&mut text, &value)?;
                }
                _ => {
                    let Some((BinaryOperator::Add, left, right)) = value.kind.binary() else {
                        return Err(Unread);
                    };
                    pending.extend([right, left]);
                }
            }
        }
        Ok(Rc::from(text))
    }

    read(
        program,
        value,
        instance,
        &mut PatternMemo::default(),
        PATTERN_DECLARATIONS,
    )
}

/// True where FMT-5 needs a selected `convert<str>` call.
pub fn needs_text_conversion(ty: &Type) -> bool {
    !matches!(
        ty,
        Type::Scalar(
            Scalar::I64
                | Scalar::F64
                | Scalar::Str
                | Scalar::Bool
                | Scalar::Date
                | Scalar::Time
                | Scalar::Datetime
                | Scalar::Duration
        )
    )
}

/// The collection's component types for a selected Core text conversion.
pub fn collection_members<'a>(
    program: &Program,
    callee: &Callee,
    ty: &'a Type,
) -> Option<Vec<&'a Type>> {
    let (module, declaration) = callee.declaration()?;
    let owner = &program.modules[module];
    if Some(module) != program.core()
        || owner.declarations[declaration].name != "convert"
        || !matches!(
            owner.declarations[declaration].kind,
            DeclarationKind::NativeCandidate(Role::Value, _)
        )
        || !matches!(&owner.details[declaration], Detail::Callable(signature)
            if signature.result == Some(Type::Scalar(Scalar::Str)))
    {
        return None;
    }
    match ty {
        Type::List { element, .. } | Type::Set(element) => Some(vec![element]),
        Type::Dict { key, value } => Some(vec![key, value]),
        _ => None,
    }
}

/// Whether a pattern or text conversion needs an instance's arguments.
pub(crate) fn has_dependent_formatting(program: &Program, body: &crate::hir::Body) -> bool {
    let mut dependent = false;
    crate::values::walk_body_calls(body, &mut |call| {
        if dependent {
            return;
        }
        let ExprKind::Call {
            callee, arguments, ..
        } = &call.kind
        else {
            return;
        };
        if let Some(signature) = value_signature(program, callee) {
            dependent = arguments.iter().any(|argument| {
                argument.pack.is_some() && crate::declared::names_parameter(&argument.value.ty)
            });
            if let Some(pattern) = argument(signature, arguments, "fmt_str") {
                pattern.value.walk(&mut |value| {
                    dependent |= matches!(value.kind, ExprKind::GenericValue { .. });
                });
            }
        } else if callee
            .declaration()
            .is_some_and(|declaration| program.needs_const_inputs(declaration))
        {
            // A generic caller can supply a concrete ordinary const value
            // only after substituting its own generic arguments.
            dependent = true;
        } else if let [argument] = arguments.as_slice()
            && let Some(members) = collection_members(program, callee, &argument.value.ty)
        {
            dependent = members.into_iter().any(crate::declared::names_parameter);
        }
    });
    dependent
}
