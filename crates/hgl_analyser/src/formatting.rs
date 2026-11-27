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
        // FMT-4 rejects fmt's byte-oriented `c` for i64.
        Scalar::I64 => !precision && matches!(rest, "" | "d" | "x" | "X" | "o" | "b" | "B"),
        Scalar::F64 => matches!(rest, "" | "a" | "A" | "e" | "E" | "f" | "F" | "g" | "G"),
        Scalar::Bool => {
            !(signed || zero || precision)
                && matches!(rest, "" | "s" | "d" | "x" | "X" | "o" | "b" | "B")
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

/// A checked plain text field measured in Unicode characters (FMT-4).
pub struct TextLayout<'a> {
    /// The single character used for each position of padding.
    pub fill: &'a str,
    /// The field's '<', '^' or '>' alignment.
    pub align: char,
    /// The minimum number of characters in the whole field.
    pub width: u32,
    /// The maximum number of characters taken from the text.
    pub precision: Option<u32>,
    /// An explicit sign from a numeric specifier.
    pub sign: Option<char>,
    /// Whether a numeric field pads after its sign with zeros.
    pub zero: bool,
}

/// A plain string or the default float text, with its checked layout.
/// Explicit numeric presentations and the fmt debug presentation
/// continue through the library's own formatter.
pub fn text_layout(scalar: Scalar, specifier: &str) -> Option<TextLayout<'_>> {
    let (fill, align, mut rest) = split_alignment(specifier);
    let mut sign = None;
    let mut zero = false;
    if scalar == Scalar::F64 {
        if let Some(flag @ ('+' | '-' | ' ')) = rest.chars().next() {
            sign = Some(flag);
            rest = &rest[1..];
        }
        if let Some(after_zero) = rest.strip_prefix('0') {
            zero = align.is_none();
            rest = after_zero;
        }
    }
    let after_width = digits(rest)?;
    let width = rest[..rest.len() - after_width.len()].parse().unwrap_or(0);
    let precision = match scalar {
        Scalar::Str => {
            let (precision, presentation) = if let Some(after) = after_width.strip_prefix('.') {
                let presentation = digits(after)?;
                let precision = after[..after.len() - presentation.len()].parse().ok()?;
                (Some(precision), presentation)
            } else {
                (None, after_width)
            };
            if !matches!(presentation, "" | "s") || (width == 0 && precision.is_none()) {
                return None;
            }
            precision
        }
        Scalar::F64 if after_width.is_empty() => None,
        _ => return None,
    };
    Some(TextLayout {
        fill,
        align: align.unwrap_or(if scalar == Scalar::F64 { '>' } else { '<' }),
        width,
        precision,
        sign,
        zero,
    })
}

fn alignment(text: &str) -> &str {
    split_alignment(text).2
}

fn split_alignment(text: &str) -> (&str, Option<char>, &str) {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return (" ", None, text);
    };
    if let Some(next @ ('<' | '^' | '>')) = chars.next() {
        let fill = first.len_utf8();
        (&text[..fill], Some(next), &text[fill + 1..])
    } else if matches!(first, '<' | '^' | '>') {
        (" ", Some(first), &text[1..])
    } else {
        (" ", None, text)
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

/// A selected Core declaration backed by the value formatter (FMT-1, FMT-9).
pub struct Renderer<'a> {
    /// Signature used to locate the pattern, value packs and options.
    pub signature: &'a Signature,
    /// The private graph entry takes the graph node’s injected Clock (CAP-3).
    clock_slot: Option<usize>,
}

impl Renderer<'_> {
    /// Index of the clock supplied by HGL, independent of named argument order.
    pub fn clock_argument(&self, arguments: &[Argument]) -> Option<usize> {
        let slot = self.clock_slot?;
        arguments
            .iter()
            .position(|argument| argument.parameter == Some(slot) && argument.pack.is_none())
    }
}

/// Recognize the public value candidate and the private graph entry by their
/// resolved Core declaration. Both use the same format checker and renderer.
pub fn renderer<'a>(program: &'a Program, callee: &Callee) -> Option<Renderer<'a>> {
    let (module, declaration) = callee.declaration()?;
    if Some(module) != program.core() {
        return None;
    }
    let owner = &program.modules[module];
    let declared = &owner.declarations[declaration];
    let Detail::Callable(signature) = &owner.details[declaration] else {
        return None;
    };
    let clock_slot = match (declared.name.as_str(), declared.kind) {
        ("format", DeclarationKind::NativeCandidate(Role::Value, _)) => None,
        ("_format", DeclarationKind::NativeFunction(Role::Value))
            if !declared.exported
                && !declared.test_only
                && owner.files[declared.file].part.as_deref() == Some("formatting") =>
        {
            let clock = program.capability_type("clock")?;
            Some(
                signature
                    .parameters
                    .iter()
                    .position(|p| p.pack.is_none() && p.ty == clock)?,
            )
        }
        _ => return None,
    };
    Some(Renderer {
        signature,
        clock_slot,
    })
}

/// The signature of Core's public native value candidate, when selected.
pub fn value_signature<'a>(program: &'a Program, callee: &Callee) -> Option<&'a Signature> {
    let renderer = renderer(program, callee)?;
    renderer.clock_slot.is_none().then_some(renderer.signature)
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
    match structural_pattern_in(program, value, instance) {
        Err(PatternFault::Unread) => module_helper_pattern_in(program, value, instance),
        known => known,
    }
}

/// The established typed folds, before a source fold or helper extension.
/// Keeping this separate preserves their existing limit outcomes (FMT-3).
pub(crate) fn structural_pattern_in(
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
                ExprKind::Field { .. } | ExprKind::Index { .. } => {
                    use crate::const_inputs::ConstInputFault::{ReaderLimit, TextLimit};

                    let projection = crate::const_inputs::projection_value(
                        program, value, instance,
                    )
                    .map_err(|fault| match fault {
                        ReaderLimit | TextLimit => PatternFault::Limit,
                        _ => Unread,
                    })?;
                    let ExprKind::Literal(Value::Str(value)) = projection.kind else {
                        return Err(Unread);
                    };
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

/// FMT-3, UNIT-13: extend an unread module pattern through the bounded
/// typed helper reader, after the caller's established folds have failed.
pub(crate) fn module_helper_pattern_in(
    program: &Program,
    value: &Expr,
    instance: &[(Location, GenericArgument)],
) -> PatternText {
    use crate::const_inputs::ConstInputFault::{ReaderLimit, TextLimit};
    use PatternFault::Unread;
    let value =
        crate::const_inputs::module_helper_pattern(program, value, instance).map_err(|fault| {
            match fault {
                ReaderLimit | TextLimit => PatternFault::Limit,
                _ => Unread,
            }
        })?;
    let ExprKind::Literal(Value::Str(text)) = value.kind else {
        return Err(Unread);
    };
    Ok(Rc::from(text))
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

/// The pattern of a Core format call whose value candidate is selected
/// during specialization. Use the public signature while the call is open.
pub(crate) fn deferred_pattern<'a>(
    program: &Program,
    call: &'a crate::values::Selecting,
) -> Option<&'a Expr> {
    let operator = call.operator;
    if call.graph || Some(operator.module) != program.core() {
        return None;
    }
    let core = &program.modules[operator.module];
    if core.declarations[operator.declaration].name != "format" {
        return None;
    }
    let Detail::Callable(signature) = core.details.get(operator.declaration)? else {
        return None;
    };
    let slot = signature
        .parameters
        .iter()
        .position(|parameter| parameter.name == "fmt_str")?;
    call.argument_for(signature, slot)
}

/// Whether a pattern or text conversion needs an instance's arguments.
pub(crate) fn has_dependent_formatting(
    program: &Program,
    (module, declaration): (usize, usize),
    body: &crate::hir::Body,
) -> bool {
    if program.modules[module].selecting[declaration]
        .iter()
        .any(|call| deferred_pattern(program, call).is_some())
    {
        return true;
    }
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
        if let Some(renderer) = renderer(program, callee) {
            dependent = arguments.iter().any(|argument| {
                argument.pack.is_some() && crate::declared::names_parameter(&argument.value.ty)
            });
            if let Some(pattern) = argument(renderer.signature, arguments, "fmt_str") {
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
        }
    });
    dependent
}
