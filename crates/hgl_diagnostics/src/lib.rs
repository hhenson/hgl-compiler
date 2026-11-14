//! Diagnostics: what a compiler stage reports when source is wrong.
//!
//! Stages never print and never stop at the first problem. Each one is handed
//! a [`Diagnostics`] sink, appends a [`Diagnostic`] for every problem it finds,
//! and carries on so that one run reports as much as it can. The caller (the
//! `hgl` command, a test runner, an editor) decides what to do with the list;
//! [`render`] turns it into the text a person reads.
//!
//! The record follows the specification's diagnostic contract
//! (`errors-and-diagnostics.md`, rule ERR-1 and the diagnostic catalog): a
//! source rejection has a [`Category`], a stable [`Code`], and a primary
//! [`Location`]. Those three are what a `# expect-error(category, "code")`
//! annotation matches (TST-13); the message is for people and is not a match
//! key.

pub mod catalogue;
pub mod render;

use hgl_source::{FileId, Span};

/// Whether a diagnostic rejects the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The source is rejected.
    Error,
    /// The source stays valid (ERR-1: "A warning does not reject valid
    /// source").
    Warning,
}

/// The specification's source-rejection categories (diagnostic catalog).
///
/// The catalog also names `build`, which is infrastructure failure rather
/// than a property of the source; it is deliberately absent here because it
/// "cannot satisfy a source-rejection expectation".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// Malformed source text or syntax.
    Parse,
    /// A name does not resolve, or resolves ambiguously.
    Name,
    /// A value has the wrong type.
    Type,
    /// A temporal shape is not admitted.
    Shape,
    /// A declared constraint is not met.
    Constraint,
    /// A callable is used as the wrong kind of function.
    FunctionKind,
    /// An operation appears in a phase that does not admit it.
    Phase,
    /// An injectable is missing or misused.
    Injectable,
    /// Operator declaration or selection fails.
    Operator,
    /// Module, part, import or export structure is wrong.
    Module,
}

impl Category {
    /// Every category, in the specification's order.
    pub const ALL: [Category; 10] = [
        Category::Parse,
        Category::Name,
        Category::Type,
        Category::Shape,
        Category::Constraint,
        Category::FunctionKind,
        Category::Phase,
        Category::Injectable,
        Category::Operator,
        Category::Module,
    ];

    /// The category the specification spells `name`, for example
    /// `function-kind`; `None` when it spells none so.
    pub fn from_name(name: &str) -> Option<Category> {
        Category::ALL
            .into_iter()
            .find(|category| category.as_str() == name)
    }

    /// The category as the specification spells it, for example
    /// `function-kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Parse => "parse",
            Category::Name => "name",
            Category::Type => "type",
            Category::Shape => "shape",
            Category::Constraint => "constraint",
            Category::FunctionKind => "function-kind",
            Category::Phase => "phase",
            Category::Injectable => "injectable",
            Category::Operator => "operator",
            Category::Module => "module",
        }
    }
}

/// The stable identifier of a diagnosed condition.
///
/// The two variants keep the specification's codes apart from this
/// compiler's own. Only a catalogued code may satisfy an `expect-error`
/// annotation (TST-12: "Category/code must be catalogued").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// A code from the specification's diagnostic catalog, for example
    /// `syntax.expected_token`.
    Catalogued(&'static str),
    /// A code local to this compiler, for a condition the specification
    /// rejects but has not catalogued. It is stable for this compiler's own
    /// tests and may be replaced when the specification assigns a code.
    Local(&'static str),
}

impl Code {
    /// The code text, whichever kind it is.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Catalogued(text) | Code::Local(text) => text,
        }
    }

    /// True for a specification code.
    pub fn is_catalogued(self) -> bool {
        matches!(self, Code::Catalogued(_))
    }
}

/// A place in the source: which file, and which bytes of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Location {
    /// The file.
    pub file: FileId,
    /// The bytes within it.
    pub span: Span,
}

impl Location {
    /// Creates a location.
    pub fn new(file: FileId, span: Span) -> Location {
        Location { file, span }
    }
}

/// Another place a diagnostic points at, with what to say about it: the
/// first declaration, when the problem is a second one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    /// The place.
    pub location: Location,
    /// What it is, for example `first declared here`.
    pub message: String,
}

/// One reported problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Whether the source is rejected.
    pub severity: Severity,
    /// The specification category.
    pub category: Category,
    /// The stable code.
    pub code: Code,
    /// Where the problem is. Its first character is the position an
    /// `expect-error` annotation is matched against (TST-13).
    pub primary: Location,
    /// The explanation shown to a person.
    pub message: String,
    /// The specification rule that requires the rejection, for example
    /// `LEX-2`. Shown as a note so a reader can look the rule up.
    pub rule: Option<&'static str>,
    /// Extra lines of help shown under the message.
    pub notes: Vec<String>,
    /// Other places worth showing with the problem. They help a reader
    /// and are not what an `expect-error` annotation matches (TST-13).
    pub related: Vec<Related>,
}

impl Diagnostic {
    /// Creates an error. Further detail is added with [`Diagnostic::with_rule`]
    /// and [`Diagnostic::with_note`], each of which returns the diagnostic so
    /// calls can be chained.
    pub fn error(
        category: Category,
        code: Code,
        primary: Location,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            category,
            code,
            primary,
            message: message.into(),
            rule: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Records the specification rule behind the diagnostic.
    pub fn with_rule(mut self, rule: &'static str) -> Diagnostic {
        self.rule = Some(rule);
        self
    }

    /// Adds a line of help.
    pub fn with_note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    /// Adds another place to show, with what to say about it.
    pub fn with_related(mut self, location: Location, message: impl Into<String>) -> Diagnostic {
        self.related.push(Related {
            location,
            message: message.into(),
        });
        self
    }
}

/// The sink compiler stages report into: an ordered list of diagnostics.
///
/// A stage receives `&mut Diagnostics` (a mutable reference, like a non-const
/// `Diagnostics&` in C++) and calls [`Diagnostics::emit`].
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Creates an empty sink.
    pub fn new() -> Diagnostics {
        Diagnostics::default()
    }

    /// Appends a diagnostic.
    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    /// True when at least one diagnostic is an error, so the source is
    /// rejected.
    pub fn has_errors(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.severity == Severity::Error)
    }

    /// True when nothing has been reported.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of diagnostics reported.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The diagnostics in the order they were reported.
    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }

    /// Reorders the diagnostics by file, then by position in the file, so
    /// output reads top to bottom whatever order the stages ran in. The sort
    /// is stable: diagnostics at the same position keep their reported order.
    pub fn sort_by_location(&mut self) {
        self.items
            .sort_by_key(|item| (item.primary.file, item.primary.span.start));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hgl_source::SourceMap;

    fn sample(file: FileId, start: u32, severity: Severity) -> Diagnostic {
        let mut diagnostic = Diagnostic::error(
            Category::Parse,
            Code::Catalogued("syntax.expected_token"),
            Location::new(file, Span::new(start, start + 1)),
            "sample",
        );
        diagnostic.severity = severity;
        diagnostic
    }

    #[test]
    fn categories_use_the_specification_spelling() {
        assert_eq!(Category::FunctionKind.as_str(), "function-kind");
        assert_eq!(Category::Parse.as_str(), "parse");
    }

    #[test]
    fn local_codes_are_not_catalogued() {
        assert!(Code::Catalogued("syntax.expected_token").is_catalogued());
        assert!(!Code::Local("example.local_code").is_catalogued());
        assert_eq!(
            Code::Local("example.local_code").as_str(),
            "example.local_code"
        );
    }

    #[test]
    fn only_errors_reject_the_source() {
        let mut sources = SourceMap::new();
        let file = sources.add("t.hgl", "abc").unwrap();
        let mut diagnostics = Diagnostics::new();
        assert!(diagnostics.is_empty());
        diagnostics.emit(sample(file, 0, Severity::Warning));
        assert!(!diagnostics.has_errors());
        diagnostics.emit(sample(file, 1, Severity::Error));
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn sorting_orders_by_file_then_position() {
        let mut sources = SourceMap::new();
        let first = sources.add("a.hgl", "abc").unwrap();
        let second = sources.add("b.hgl", "abc").unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.emit(sample(second, 0, Severity::Error));
        diagnostics.emit(sample(first, 2, Severity::Error));
        diagnostics.emit(sample(first, 1, Severity::Error));
        diagnostics.sort_by_location();
        let order: Vec<(usize, u32)> = diagnostics
            .as_slice()
            .iter()
            .map(|item| (item.primary.file.index(), item.primary.span.start))
            .collect();
        assert_eq!(order, [(0, 1), (0, 2), (1, 0)]);
    }
}
