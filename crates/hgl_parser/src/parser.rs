//! The parser's machinery: the cursor, the event list, error reports, and
//! taking a reading back.
//!
//! The grammar itself is in the [`crate::grammar`] modules, written as
//! methods on [`Parser`]. This module holds what they share.
//!
//! Three ideas carry most of it:
//!
//! * **Line breaks are asked for, not read.** The cursor steps over line
//!   breaks, and [`Parser::nl`] says whether one that counts stands before
//!   the cursor's token. LEX-5 lists where a line break is ignored: inside
//!   most delimiters (the [`Context`]), after a token that cannot end a
//!   line such as `,` or `=` ([`Parser::continue_line`]), and before a
//!   token that continues one such as a binary operator (the grammar looks
//!   for those with [`Parser::at`], which does not ask). Everywhere else a
//!   line break ends the line: [`Parser::eat`] and [`Parser::expect`] do
//!   not reach across it.
//! * **Context.** Whether a line break counts, whether `{` may start an
//!   expression, and whether `>` compares, depend on what encloses the
//!   cursor. [`Context`] holds those answers and each delimiter replaces it.
//! * **Probes.** Where one token cannot tell two readings apart, the parser
//!   tries a reading, notes whether it worked, and takes it back
//!   ([`Parser::choose`]). Nothing is lost by this: the grammar functions
//!   only append to lists, and taking back is cutting the lists short.

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Code, Diagnostic, Location};
use hgl_lexer::TokenKind;
use hgl_source::{FileId, Span};

use crate::codes;
use crate::event::Event;
use crate::input::Input;
use crate::kind::SyntaxKind;

/// What the enclosing construct lets the cursor's tokens mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Context {
    /// A line break can end what is being parsed (LEX-5). False inside
    /// parentheses, brackets, angle brackets and braces that hold a list.
    pub(crate) newlines: bool,
    /// `{` opens the body of the enclosing `when` or `while`, so it cannot
    /// start an expression here (CTRL-9).
    pub(crate) no_brace: bool,
    /// `>` closes the enclosing generic argument list, so it is not a
    /// comparison here (GEN-14).
    pub(crate) no_gt: bool,
    /// A comma here separates entries, so `a < b, c > (d)` could be two
    /// comparisons (GEN-14).
    pub(crate) comma_list: bool,
}

impl Context {
    /// Statements or members, one to a line.
    pub(crate) const BLOCK: Context = Context {
        newlines: true,
        no_brace: false,
        no_gt: false,
        comma_list: false,
    };
    /// Comma-separated expressions: parentheses, sequence brackets,
    /// arguments, operator configuration.
    pub(crate) const LIST: Context = Context {
        newlines: false,
        no_brace: false,
        no_gt: false,
        comma_list: true,
    };
    /// Delimiters whose commas, if any, do not separate expressions: index
    /// brackets, import sets, constraint sets, property lists, pack types.
    pub(crate) const PLAIN: Context = Context {
        newlines: false,
        no_brace: false,
        no_gt: false,
        comma_list: false,
    };
    /// Generic parameters or arguments.
    pub(crate) const ANGLE: Context = Context {
        newlines: false,
        no_brace: false,
        no_gt: true,
        comma_list: false,
    };

    /// The four answers packed into one number, for use in a lookup key.
    fn bits(self) -> u8 {
        u8::from(self.newlines)
            | u8::from(self.no_brace) << 1
            | u8::from(self.no_gt) << 2
            | u8::from(self.comma_list) << 3
    }
}

/// The answer of a probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The reading the probe is named for holds.
    Yes,
    /// It does not.
    No,
    /// It holds and so does the other reading. Only the constructor probe
    /// gives this answer (GEN-14).
    Both,
}

/// The questions a probe answers. Each names its "yes" reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Choice {
    /// `name<...>(...)` constructs a generic type, rather than comparing.
    Constructor,
    /// A generic argument is a type, rather than an expression.
    GenericArgument,
    /// A constraint operand is a type, rather than an expression.
    Operand,
    /// A constraint term is a relation such as `T in {i64, f64}`.
    Relation,
    /// `switch` starts a switch statement, rather than naming a value.
    Switch,
    /// `raises` after `assert` starts a raises assertion.
    Raises,
    /// A statement that starts with a name is an assignment.
    Assignment,
}

/// A place in the event list a node may later be started from. See
/// [`Event::Mark`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mark(usize);

/// One error, with the token position it was reported at.
pub(crate) struct Report {
    /// Index of the significant token the cursor stood on.
    pub(crate) at: usize,
    /// The error.
    pub(crate) diagnostic: Diagnostic,
}

/// Everything a probe may change, saved so it can be put back.
pub(crate) struct Snapshot {
    position: usize,
    events: usize,
    reports: usize,
    errors: usize,
    last_error_at: Option<usize>,
    context: Context,
    glued_assign: Option<usize>,
    joined: Option<usize>,
}

/// The shape of an angle-bracketed list that was just parsed.
pub(crate) struct AngleList {
    /// Number of entries.
    pub(crate) count: usize,
    /// Token position of each comma between entries.
    pub(crate) commas: Vec<usize>,
    /// True when the closing `>` was found.
    pub(crate) closed: bool,
    /// True when every entry was parsed as a type.
    pub(crate) all_types: bool,
}

/// The state of one parse.
///
/// `'a` is a lifetime: the parser borrows the tokens and the source text
/// for as long as it lives and copies neither.
pub(crate) struct Parser<'a> {
    input: &'a Input<'a>,
    file: FileId,
    /// Index of the significant token the cursor stands on.
    pub(crate) position: usize,
    events: Vec<Event>,
    reports: Vec<Report>,
    /// Number of errors found, counting those not reported because one was
    /// already reported at the same token. Grammar functions compare it
    /// before and after a piece to learn whether the piece was clean.
    pub(crate) errors: usize,
    /// Token position of the last error reported.
    last_error_at: Option<usize>,
    /// What the enclosing construct allows.
    pub(crate) context: Context,
    /// Set when `>=` has been taken as the `>` that closes a generic
    /// argument list (LEX-17): the cursor is then standing where the `=`
    /// would have been, at this position.
    glued_assign: Option<usize>,
    /// A token position whose line break, if it has one before it, does
    /// not count: the token before it cannot end a line (LEX-5).
    joined: Option<usize>,
    /// A token position at which a name is read as a plain name, not as
    /// the start of generic arguments. Set while the comparison reading of
    /// `name < ...` is tried (GEN-14).
    pub(crate) no_generic_at: Option<usize>,
    /// Answers already worked out, so that probes nested in probes do not
    /// repeat work. The key is the question, the token position and the
    /// context.
    decisions: HashMap<(Choice, usize, u8), Verdict>,
    /// Number of probes in progress: more than one when a probe meets a
    /// question of its own.
    probing: usize,
    /// Token positions at which an entry of a generic argument list has
    /// started, in a list that then never reached its `>`. Any list with
    /// an entry starting at one of these reads the same tokens to the same
    /// end, so a probe that comes to one need not read on.
    unclosed_from: HashSet<usize>,
    /// Current nesting depth.
    depth: usize,
    /// The depth past which nesting is reported, not read:
    /// [`crate::Limits::nesting`].
    max_depth: usize,
}

impl<'a> Parser<'a> {
    /// A parser at the start of `input` that reads nesting up to
    /// `max_depth` levels deep.
    pub(crate) fn new(input: &'a Input<'a>, file: FileId, max_depth: usize) -> Parser<'a> {
        Parser {
            input,
            file,
            position: 0,
            events: Vec::new(),
            reports: Vec::new(),
            errors: 0,
            last_error_at: None,
            context: Context::BLOCK,
            glued_assign: None,
            joined: None,
            no_generic_at: None,
            decisions: HashMap::new(),
            probing: 0,
            unclosed_from: HashSet::new(),
            depth: 0,
            max_depth,
        }
    }

    /// Ends the parse, giving the events and the errors found.
    pub(crate) fn finish_parse(self) -> (Vec<Event>, Vec<Diagnostic>) {
        let reports = self
            .reports
            .into_iter()
            .map(|report| report.diagnostic)
            .collect();
        (self.events, reports)
    }

    // ---- Cursor ----

    /// Kind of the token under the cursor; [`SyntaxKind::Eof`] at the end.
    pub(crate) fn current(&self) -> SyntaxKind {
        self.input.kind(self.position)
    }

    /// Kind of the token `n` places after the cursor.
    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        self.input.kind(self.position + n)
    }

    /// True when the token under the cursor is of `kind`.
    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// Text of the token `n` places after the cursor.
    pub(crate) fn text(&self, n: usize) -> &'a str {
        self.input.text(self.position + n)
    }

    /// True when the token under the cursor is the name `word`.
    ///
    /// This is how a contextual name such as `const` or `switch` is
    /// recognised: the lexer gives it as an identifier and its position
    /// gives it its meaning (LEX-9, LEX-10).
    pub(crate) fn at_word(&self, word: &str) -> bool {
        self.nth_word(0, word)
    }

    /// True when the token `n` places after the cursor is the name `word`.
    pub(crate) fn nth_word(&self, n: usize, word: &str) -> bool {
        self.nth(n) == SyntaxKind::Ident && self.text(n) == word
    }

    /// True when a line break that counts stands before the cursor's token
    /// (LEX-5). Never true where the context ignores line breaks.
    pub(crate) fn nl(&self) -> bool {
        self.nl_at(0)
    }

    /// As [`Parser::nl`], for the token `n` places after the cursor.
    pub(crate) fn nl_at(&self, n: usize) -> bool {
        let index = self.position + n;
        self.context.newlines && self.joined != Some(index) && self.input.line_break_before(index)
    }

    /// Says that the token just taken cannot end a line, so a line break
    /// after it is ignored. LEX-5 lists them: a comma, a binary or
    /// assignment operator, `=`, `=>`, `->`, and a colon before a type or
    /// after an argument's name.
    ///
    /// Also called before the first token of a statement, member or
    /// declaration: the line break before it has separated it from the one
    /// before and is not counted a second time.
    pub(crate) fn continue_line(&mut self) {
        self.joined = Some(self.position);
    }

    /// True when the cursor's token is on the line being read. Reports
    /// that `what` was expected, at the end of that line, when it is not.
    pub(crate) fn on_line(&mut self, what: &str) -> bool {
        if self.nl() {
            self.expected(what);
            return false;
        }
        true
    }

    /// True when the cursor's token is the first on its line, whatever the
    /// context. Recovery uses this to find where to resume.
    pub(crate) fn at_line_start(&self) -> bool {
        self.position == 0 || self.input.line_break_before(self.position)
    }

    /// Takes the cursor's token into the open node and moves on. Does
    /// nothing at the end of the input.
    pub(crate) fn bump(&mut self) {
        if !self.at(SyntaxKind::Eof) {
            self.events.push(Event::Token);
            self.position += 1;
        }
    }

    /// Takes the cursor's token when it is of `kind` and on the line being
    /// read. A token across a line break that counts is left alone: the
    /// places where a line may go on are few, and each asks with
    /// [`Parser::at`] instead.
    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        let found = self.at(kind) && !self.nl();
        if found {
            self.bump();
        }
        found
    }

    /// Takes the cursor's token when it is of `kind` and on the line being
    /// read; reports otherwise.
    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.expected(&describe(kind));
        false
    }

    /// Takes an identifier; reports that `what` was expected otherwise.
    pub(crate) fn expect_name(&mut self, what: &str) -> bool {
        if self.eat(SyntaxKind::Ident) {
            return true;
        }
        self.expected(what);
        false
    }

    /// Takes the name `word`, on the line being read; reports otherwise.
    pub(crate) fn expect_word(&mut self, word: &str) -> bool {
        if self.at_word(word) && !self.nl() {
            self.bump();
            return true;
        }
        self.expected(&format!("`{word}`"));
        false
    }

    /// True at a member name: an identifier, or `time` (LEX-9).
    pub(crate) fn at_member_name(&self) -> bool {
        self.nth_member_name(0)
    }

    /// As [`Parser::at_member_name`], `n` places after the cursor.
    pub(crate) fn nth_member_name(&self, n: usize) -> bool {
        matches!(self.nth(n), SyntaxKind::Ident | SyntaxKind::KwTime)
    }

    /// Takes a member name; reports otherwise.
    pub(crate) fn expect_member_name(&mut self) -> bool {
        if self.at_member_name() && !self.nl() {
            self.bump();
            return true;
        }
        self.expected("a member name");
        false
    }

    // ---- Nodes ----

    /// Opens a node of `kind` at the cursor.
    pub(crate) fn start(&mut self, kind: SyntaxKind) {
        self.events.push(Event::Start(kind));
    }

    /// Closes the innermost open node.
    pub(crate) fn finish(&mut self) {
        self.events.push(Event::Finish);
    }

    /// Remembers the cursor's place, so that what is parsed next can be
    /// wrapped in a node once its kind is known.
    pub(crate) fn mark(&mut self) -> Mark {
        self.events.push(Event::Mark);
        Mark(self.events.len() - 1)
    }

    /// Wraps everything parsed since `mark` in a node of `kind`.
    pub(crate) fn wrap(&mut self, mark: Mark, kind: SyntaxKind) {
        self.events.push(Event::StartAt(mark.0, kind));
        self.events.push(Event::Finish);
    }

    /// Takes the cursor's token as a node of `kind` holding just it.
    pub(crate) fn leaf(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.bump();
        self.finish();
    }

    // ---- Context and depth ----

    /// Runs `parse` with `context` in place of the current one.
    ///
    /// `impl FnOnce(&mut Self) -> R` is "any function or closure that can
    /// be called once with the parser and gives an `R`".
    pub(crate) fn nested<R>(&mut self, context: Context, parse: impl FnOnce(&mut Self) -> R) -> R {
        let outer = std::mem::replace(&mut self.context, context);
        let result = parse(self);
        self.context = outer;
        result
    }

    /// Goes one level deeper. False, with a report, at the nesting limit;
    /// the caller then parses nothing. A true answer is paired with
    /// [`Parser::leave`].
    pub(crate) fn enter(&mut self) -> bool {
        if self.depth >= self.max_depth {
            let span = self.input.span(self.position);
            let message = format!("nesting deeper than {} levels is not read", self.max_depth);
            self.report(span, Code::Local(codes::NESTING_LIMIT), message, None, None);
            return false;
        }
        self.depth += 1;
        true
    }

    /// Comes back up one level.
    pub(crate) fn leave(&mut self) {
        self.depth -= 1;
    }

    // ---- Errors ----

    /// Records an error at the cursor. A second error at the same token is
    /// counted and not recorded: one token, one report.
    fn report(
        &mut self,
        span: Span,
        code: Code,
        message: String,
        rule: Option<&'static str>,
        note: Option<&str>,
    ) {
        self.errors += 1;
        if self.last_error_at == Some(self.position) {
            return;
        }
        self.last_error_at = Some(self.position);
        self.push_report(span, self.position, code, message, rule, note);
    }

    fn push_report(
        &mut self,
        span: Span,
        at: usize,
        code: Code,
        message: String,
        rule: Option<&'static str>,
        note: Option<&str>,
    ) {
        let location = Location::new(self.file, span);
        let mut diagnostic = Diagnostic::error(Category::Parse, code, location, message);
        if let Some(rule) = rule {
            diagnostic = diagnostic.with_rule(rule);
        }
        if let Some(note) = note {
            diagnostic = diagnostic.with_note(note);
        }
        self.reports.push(Report { at, diagnostic });
    }

    /// Where an "expected" error points and what it says was found.
    ///
    /// At the end of a line or of the file the error points just after the
    /// last token read, which is where the missing piece belongs, not at
    /// whatever comes next.
    fn found(&self) -> (Span, String) {
        let after_previous = match self.position.checked_sub(1) {
            Some(index) => self.input.span(index).end,
            None => 0,
        };
        if self.at(SyntaxKind::Eof) {
            (Span::empty(after_previous), "end of file".to_string())
        } else if self.nl() {
            (Span::empty(after_previous), "end of line".to_string())
        } else {
            let text = self.text(0);
            let shown =
                if self.at(SyntaxKind::Str) && (text.len() > 40 || text.contains(['\n', '\r'])) {
                    "a string literal".to_string()
                } else {
                    format!("`{text}`")
                };
            (self.input.span(self.position), shown)
        }
    }

    /// Reports "expected `what`, found ...".
    pub(crate) fn expected(&mut self, what: &str) {
        self.expected_with(what, None, None);
    }

    /// As [`Parser::expected`], citing the rule that requires `what`.
    pub(crate) fn expected_rule(&mut self, what: &str, rule: &'static str) {
        self.expected_with(what, Some(rule), None);
    }

    /// As [`Parser::expected`], with an optional rule and note.
    pub(crate) fn expected_with(
        &mut self,
        what: &str,
        rule: Option<&'static str>,
        note: Option<&str>,
    ) {
        let (span, found) = self.found();
        let message = format!("expected {what}, found {found}");
        let code = Code::Catalogued(codes::SYNTAX_EXPECTED_TOKEN);
        self.report(span, code, message, rule, note);
    }

    /// Reports `message` against the cursor's token.
    pub(crate) fn error(&mut self, message: &str, rule: &'static str, note: Option<&str>) {
        let span = self.input.span(self.position);
        let code = Code::Catalogued(codes::SYNTAX_EXPECTED_TOKEN);
        self.report(span, code, message.to_string(), Some(rule), note);
    }

    /// Reports `message` against the token at position `at`, which the
    /// cursor has already passed. Always recorded.
    pub(crate) fn error_behind(&mut self, at: usize, message: &str, rule: &'static str) {
        self.errors += 1;
        let span = self.input.span(at);
        let code = Code::Catalogued(codes::SYNTAX_EXPECTED_TOKEN);
        self.push_report(span, at, code, message.to_string(), Some(rule), None);
    }

    /// Number of errors recorded so far. With [`Parser::errors_between`].
    pub(crate) fn report_count(&self) -> usize {
        self.reports.len()
    }

    /// True when an error recorded since `first_report` stands before token
    /// position `end`.
    pub(crate) fn errors_between(&self, first_report: usize, end: usize) -> bool {
        self.reports[first_report..]
            .iter()
            .any(|report| report.at < end)
    }

    // ---- Probes ----

    /// Saves what a probe may change.
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            position: self.position,
            events: self.events.len(),
            reports: self.reports.len(),
            errors: self.errors,
            last_error_at: self.last_error_at,
            context: self.context,
            glued_assign: self.glued_assign,
            joined: self.joined,
        }
    }

    /// Puts back what `snapshot` saved, forgetting everything parsed and
    /// reported since.
    pub(crate) fn rollback(&mut self, snapshot: &Snapshot) {
        self.position = snapshot.position;
        self.events.truncate(snapshot.events);
        self.reports.truncate(snapshot.reports);
        self.errors = snapshot.errors;
        self.last_error_at = snapshot.last_error_at;
        self.context = snapshot.context;
        self.glued_assign = snapshot.glued_assign;
        self.joined = snapshot.joined;
    }

    /// Answers `choice` at the cursor by running `probe`, then takes back
    /// everything the probe did. The caller parses the chosen reading
    /// afterwards, for real.
    ///
    /// An answer is kept and reused: a probe may itself reach the same
    /// question at the same place many times, and without this the work
    /// would double with each level of nesting.
    pub(crate) fn choose(
        &mut self,
        choice: Choice,
        probe: impl FnOnce(&mut Self) -> Verdict,
    ) -> Verdict {
        let key = (choice, self.position, self.context.bits());
        // While a name at this very token is being read as a plain name,
        // answers here are not the usual ones, so they are not shared.
        let shared = self.no_generic_at != Some(self.position);
        if shared && let Some(verdict) = self.decisions.get(&key) {
            return *verdict;
        }
        let snapshot = self.snapshot();
        let no_generic_at = self.no_generic_at;
        // Inside the probe every error counts, including one at a token
        // that already has a report.
        self.last_error_at = None;
        self.probing += 1;
        let verdict = probe(self);
        self.probing -= 1;
        self.rollback(&snapshot);
        self.no_generic_at = no_generic_at;
        if shared {
            self.decisions.insert(key, verdict);
        }
        verdict
    }

    // ---- `=` after `>` ----

    /// True at `=`, or where a `>=` was split into `>` and `=` (LEX-17).
    pub(crate) fn at_assign(&self) -> bool {
        self.at(SyntaxKind::Eq) || self.glued_assign == Some(self.position)
    }

    /// Takes the `=` that [`Parser::at_assign`] saw. What follows it may
    /// stand on the next line (LEX-5).
    pub(crate) fn eat_assign(&mut self) {
        if self.glued_assign == Some(self.position) {
            // The token was taken as the closing `>`; nothing is left.
            self.glued_assign = None;
        } else if self.at(SyntaxKind::Eq) {
            self.bump();
        }
        self.continue_line();
    }

    /// Takes the `>` that closes an angle-bracketed list.
    ///
    /// LEX-17: `>` directly followed by `=` is the one token `>=`, even
    /// after generic arguments. Such a `>=` is reported, then read as the
    /// two tokens the author meant so that parsing can go on.
    pub(crate) fn close_angle(&mut self) -> bool {
        // `at`, not `eat`: the `>` belongs to the list, where line breaks
        // do not count, though the list's context has been left by now.
        if self.at(SyntaxKind::Gt) {
            self.bump();
            return true;
        }
        if self.at(SyntaxKind::GtEq) {
            self.error(
                "`>=` is one token and does not close generic arguments",
                "LEX-17",
                Some("write `> =`, with a space"),
            );
            self.bump();
            self.glued_assign = Some(self.position);
            self.continue_line();
            return true;
        }
        self.expected("`>`");
        false
    }

    /// Takes the token `close` that ends a delimiter holding one thing,
    /// such as the `]` of an index. Anything else before it is reported
    /// and stepped over.
    pub(crate) fn close(&mut self, close: SyntaxKind) {
        if !self.eat(close) {
            self.expected(&describe(close));
            self.recover_to(close);
            self.eat(close);
        }
    }

    /// Takes the `}` that closes a block or body, on whatever line it
    /// stands; reports when it is missing.
    pub(crate) fn close_brace(&mut self) {
        if self.at(SyntaxKind::RBrace) {
            self.bump();
        } else {
            self.expected("`}`");
        }
    }

    // ---- Lists ----

    /// Parses `entry {, entry} [,]` and the closing token `close`. The
    /// opening token has been taken.
    ///
    /// `entry` parses one entry and leaves the cursor where it was when it
    /// finds none. A comma stays on its entry's line where line breaks
    /// count (an enum body); `trailing` says whether a comma may follow
    /// the last entry.
    pub(crate) fn list(
        &mut self,
        close: SyntaxKind,
        what: &str,
        trailing: bool,
        mut entry: impl FnMut(&mut Self),
    ) {
        let mut recovered = false;
        loop {
            if self.at(close) {
                break;
            }
            // An entry may start on a new line, where line breaks count.
            self.continue_line();
            let start = self.position;
            let errors = self.errors;
            entry(self);
            if self.position == start {
                if self.errors == errors {
                    self.expected(what);
                }
                self.recover_to(close);
                recovered = true;
                break;
            }
            if self.at(SyntaxKind::Comma) && !self.nl() {
                self.bump();
                self.continue_line();
                if self.at(close) && !trailing {
                    self.expected(what);
                }
                continue;
            }
            if !self.at(close) {
                self.expected(&format!("`,` or {}", describe(close)));
                self.recover_to(close);
                recovered = true;
            }
            break;
        }
        // The closing token may stand on its own line.
        if self.at(close) {
            self.bump();
        } else if !recovered {
            // After recovery the error is reported already; a missing
            // closer is not a second one.
            self.expected(&describe(close));
        }
    }

    /// Parses `< entry {, entry} [,] >` with the cursor on the `<`.
    ///
    /// `entry` parses one entry and says whether it was a type. Nothing is
    /// skipped on an error: the tokens after a `<` that turns out not to
    /// open a list belong to whatever follows.
    ///
    /// `arguments` says the list is one of generic arguments, the kind a
    /// probe may read only to find that `<` was a comparison. In
    /// `f(a < b, c < d, e < f)` every `<` starts such a reading, and each
    /// would run to the `)` before failing, which for a long list is a
    /// great deal of reading. So where an unclosed list's entries started
    /// is remembered, and a probe that reaches one of those places stops
    /// there: the list it is reading cannot close either.
    pub(crate) fn angle_list(
        &mut self,
        what: &str,
        trailing: bool,
        arguments: bool,
        mut entry: impl FnMut(&mut Self) -> bool,
    ) -> AngleList {
        let mut shape = AngleList {
            count: 0,
            commas: Vec::new(),
            closed: false,
            all_types: true,
        };
        // Where each entry of this list started.
        let mut starts: Vec<usize> = Vec::new();
        self.bump();
        let outer = std::mem::replace(&mut self.context, Context::ANGLE);
        loop {
            let at_close = |parser: &Self| parser.at(SyntaxKind::Gt) || parser.at(SyntaxKind::GtEq);
            if at_close(self) {
                if shape.count == 0 {
                    self.expected(what);
                }
                break;
            }
            let start = self.position;
            if arguments {
                if self.probing > 0 && self.unclosed_from.contains(&start) {
                    // No `>` is reached from here. Outside a probe the
                    // list is read on all the same, for its diagnostics.
                    break;
                }
                starts.push(start);
            }
            let errors = self.errors;
            let is_type = entry(self);
            if self.position == start {
                if self.errors == errors {
                    self.expected(what);
                }
                break;
            }
            shape.count += 1;
            shape.all_types &= is_type;
            if !self.at(SyntaxKind::Comma) {
                break;
            }
            shape.commas.push(self.position);
            self.bump();
            if at_close(self) {
                if !trailing {
                    self.expected(what);
                }
                break;
            }
        }
        self.context = outer;
        shape.closed = self.close_angle();
        if !shape.closed {
            self.unclosed_from.extend(starts);
        }
        shape
    }

    /// Parses entries that stand one to a line, up to a `}` or the end of
    /// the input: the statements of a block, the fields of a struct.
    ///
    /// `entry` parses one entry and leaves the cursor where it was when it
    /// finds none. `stop` is asked before each entry and ends the run
    /// early. `rule` is cited when two entries share a line.
    pub(crate) fn lines(
        &mut self,
        what: &str,
        rule: &'static str,
        stop: impl Fn(&Self) -> bool,
        mut entry: impl FnMut(&mut Self),
    ) {
        loop {
            if self.at(SyntaxKind::RBrace) || self.at(SyntaxKind::Eof) || stop(self) {
                break;
            }
            if self.at(SyntaxKind::Semicolon) {
                self.stray_semicolon();
                continue;
            }
            // The line break before this entry has done its work as a
            // separator; the entry's first token is not "across" it.
            self.continue_line();
            let start = self.position;
            let errors = self.errors;
            entry(self);
            if self.position == start {
                if self.errors == errors {
                    self.expected(what);
                }
                self.skip_line(true);
            } else if self.errors != errors {
                // The entry is broken somewhere; what is left of its line
                // would only produce reports that follow from that.
                self.skip_line(false);
            } else if self.at(SyntaxKind::Semicolon) {
                self.stray_semicolon();
            } else if !(self.at(SyntaxKind::RBrace)
                || self.at(SyntaxKind::Eof)
                || self.nl()
                || stop(self))
            {
                self.expected_rule("a line break", rule);
            }
        }
    }

    /// Reports a `;` and steps over it. LEX-4: a semicolon does not end a
    /// statement; it appears only in native hook markers.
    fn stray_semicolon(&mut self) {
        self.error(
            "`;` does not end a statement",
            "LEX-4",
            Some("a line break separates statements; `;` belongs to native hook markers"),
        );
        self.leaf(SyntaxKind::Error);
    }

    // ---- Recovery ----

    /// Steps over tokens, into one `Error` node, up to the `close` that
    /// matches the list the cursor is in.
    ///
    /// Stops early at the end of the input, at a closing token that
    /// belongs to an enclosing construct, and at a line that starts with a
    /// word no entry of a list can start with: the list was most likely
    /// never closed, and the next statement should be read as one.
    pub(crate) fn recover_to(&mut self, close: SyntaxKind) {
        let mut depth = 0usize;
        let mut open = false;
        loop {
            let kind = self.current();
            if kind == SyntaxKind::Eof {
                break;
            }
            if depth == 0 && (kind == close || is_closer(kind)) {
                break;
            }
            if self.input.line_break_before(self.position) && starts_a_line(kind) {
                break;
            }
            if is_opener(kind) {
                depth += 1;
            } else if is_closer(kind) {
                depth -= 1;
            }
            if !open {
                self.start(SyntaxKind::Error);
                open = true;
            }
            self.bump();
        }
        if open {
            self.finish();
        }
    }

    /// Steps over the rest of the line, into one `Error` node.
    ///
    /// The line ends at a line break outside any brackets opened on it, at
    /// a `}` that closes the enclosing block, or at the end of the input.
    /// With `at_least_one` the cursor's token is taken even when it is the
    /// first of its line, so that the caller is certain to move on.
    pub(crate) fn skip_line(&mut self, at_least_one: bool) {
        let mut depth = 0usize;
        let mut open = false;
        loop {
            let kind = self.current();
            if kind == SyntaxKind::Eof || (depth == 0 && kind == SyntaxKind::RBrace) {
                break;
            }
            let forced = at_least_one && !open;
            if depth == 0 && !forced && self.input.line_break_before(self.position) {
                break;
            }
            if is_opener(kind) {
                depth += 1;
            } else if is_closer(kind) {
                depth = depth.saturating_sub(1);
            }
            if !open {
                self.start(SyntaxKind::Error);
                open = true;
            }
            self.bump();
        }
        if open {
            self.finish();
        }
    }
}

/// How a token kind is named in an "expected" message.
pub(crate) fn describe(kind: SyntaxKind) -> String {
    // A keyword or symbol is named by its spelling, found in the lexer's
    // own tables.
    let fixed = TokenKind::KEYWORDS
        .iter()
        .chain(TokenKind::PUNCTUATION)
        .find(|(_, token)| SyntaxKind::from(*token) == kind);
    match (fixed, kind) {
        (Some((text, _)), _) => format!("`{text}`"),
        (None, SyntaxKind::Ident) => "a name".to_string(),
        (None, SyntaxKind::Str) => "a string literal".to_string(),
        (None, SyntaxKind::Eof) => "end of file".to_string(),
        (None, other) => format!("{other:?}"),
    }
}

/// True for `(`, `[` and `{`.
fn is_opener(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LParen | SyntaxKind::LBracket | SyntaxKind::LBrace
    )
}

/// True for `)`, `]` and `}`.
fn is_closer(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::RParen | SyntaxKind::RBracket | SyntaxKind::RBrace
    )
}

/// True for a reserved word that starts a statement or declaration and can
/// start nothing inside a list.
fn starts_a_line(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::KwLet
            | SyntaxKind::KwVar
            | SyntaxKind::KwReturn
            | SyntaxKind::KwState
            | SyntaxKind::KwCache
            | SyntaxKind::KwInject
            | SyntaxKind::KwStart
            | SyntaxKind::KwStop
            | SyntaxKind::KwWhen
            | SyntaxKind::KwFor
            | SyntaxKind::KwWhile
            | SyntaxKind::KwYield
            | SyntaxKind::KwAssert
            | SyntaxKind::KwTest
            | SyntaxKind::KwUse
            | SyntaxKind::KwExport
            | SyntaxKind::KwImpl
            | SyntaxKind::KwStruct
            | SyntaxKind::KwEnum
            | SyntaxKind::KwOperator
            | SyntaxKind::KwInstantiate
            | SyntaxKind::KwModule
            | SyntaxKind::KwAbstract
    )
}
