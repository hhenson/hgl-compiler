//! `-value`, `!value` and the operators with two operands: each symbol
//! is read as the call of the core operator it stands for, with its
//! configuration, and `&&` and `||` as they are written (NUM-1, EXPR-6,
//! EXPR-10, GEN-5). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::candidates::{Given, Key, Settled, Wanted, typed_by_place};
use super::control::joined;
use super::expressions::{takes_its_type, written_null};
use super::fit::{open, whole};
use super::operators::{from_place, options_of, public_inputs, unreadable};
use super::{BOOL, Typer};
use crate::constant::is_least_magnitude;
use crate::hir::{BinaryOperator, Expr, ExprKind, Form, UnaryOperator, Value, ValueCategory};
use crate::program::{DeclarationKind, OperatorRef};
use crate::syntax::{child, tokens, ungrouped};
use crate::types::{Detail, Scalar, Signature, Type};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// `-value`, `!value`.
    ///
    /// `-` stands for core's `neg_` (the generics chapter: "Syntax binds
    /// these core identities"), and is read as the call of it: NUM-1,
    /// "Unary negation preserves the numeric type", and OPS-1 are what
    /// its candidates declare. `!` is of a bool (EXPR-6; OPS-5: "`&&`,
    /// `||`, and `!` take bools"), and is read as it is written.
    pub(super) fn unary(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let config = child(node, SyntaxKind::OperatorConfig);
        let operand = node
            .children()
            .find(|part| part.kind() != SyntaxKind::OperatorConfig);
        let (Some(token), Some(operand)) = (tokens(node).next(), operand) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let at = Location::new(self.file, token.span());
        match token.kind() {
            SyntaxKind::Bang => {
                let _ = self.symbol_operator(UnaryOperator::Not.identity(), at);
                // Configuration of `!` is for the pass that reads it through
                // core's `not_`.
                if config.is_some() {
                    return (ExprKind::Unread, Type::Unknown);
                }
                let operand = self.expression(&operand, Some(&BOOL));
                let operand = self.fit(operand, &BOOL, "the operand of `!`");
                let kind = ExprKind::Unary {
                    operator: UnaryOperator::Not,
                    operand: Box::new(operand),
                };
                (kind, BOOL)
            }
            SyntaxKind::Minus => {
                // LEX-7: 2^63 is an integer only as the operand of a
                // minus, where the two are the least `i64`.
                if config.is_none() && is_least_magnitude(&operand) {
                    return (
                        ExprKind::Literal(Value::Int(i64::MIN)),
                        Type::Scalar(Scalar::I64),
                    );
                }
                let operator = UnaryOperator::Negate;
                let identity = self.symbol_operator(operator.identity(), at);
                // EXPR-13: "Unary configuration precedes its operand."
                let (mut given, faulty) = match (&config, identity) {
                    (Some(config), Some(identity)) => {
                        self.configuration(operator.symbol(), config, identity)
                    }
                    // Configuration of an operator the program does not
                    // have.
                    (Some(_), None) => return (ExprKind::Unread, Type::Unknown),
                    (None, _) => (Vec::new(), false),
                };
                let value = self.expression(&operand, None);
                let symbol = Symbol {
                    form: Form::Unary(operator),
                    text: operator.symbol(),
                    at,
                    rule: "OPS-1",
                    unjudged: Type::Unknown,
                    place: expected.filter(|place| !open(place)).cloned(),
                };
                let only = Given::read(0, operand, value);
                // GEN-7: `-value` under `requires neg_(T) -> T`.
                if config.is_none()
                    && let Some(operator) = identity
                    && let Some(result) = self.required(operator, &[&only], (None, expected), None)
                {
                    return self.call_required(operator, result, vec![only], symbol.form);
                }
                // An operand whose operators are not judged here: see
                // `unjudged_operand`.
                // A nullable operand is an operand as another is: no
                // candidate takes one, and the minus is then rejected
                // (ERR-12).
                let identity = identity.filter(|_| {
                    matches!(only.value.ty, Type::Nullable(_)) || !unjudged_operand(&only.value.ty)
                });
                let Some(identity) = identity else {
                    if config.is_none() {
                        let kind = ExprKind::Unary {
                            operator,
                            operand: Box::new(only.value),
                        };
                        return (kind, Type::Unknown);
                    }
                    given.push(only);
                    return (self.unselected(given, symbol.form), Type::Unknown);
                };
                given.push(only);
                if faulty {
                    return (self.unselected(given, symbol.form), symbol.unjudged);
                }
                self.call_symbol(symbol, identity, given)
            }
            _ => (ExprKind::Unread, Type::Unknown),
        }
    }

    /// `left + right`, and each other operator with two operands.
    pub(super) fn binary(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        // A chain, `a + b + c`, nests to the left as deep as it is long
        // (the parser's tree depth, not its nesting). It is read from
        // its innermost link outward, in a loop, so that its length
        // costs no stack. A link that is not read, `&&` or `||` with
        // configuration, is not read inside either.
        let mut links = vec![node.clone()];
        loop {
            let last = links.last().expect("never empty");
            if unread_link(last) {
                break;
            }
            match last.children().next() {
                Some(left) if left.kind() == SyntaxKind::BinaryExpr => links.push(left),
                _ => break,
            }
        }
        // The link inside the one being read, as its left operand.
        let mut inside: Option<Expr> = None;
        loop {
            let link = links.pop().expect("never empty");
            // The type the place expects is of the whole chain, which
            // is its outermost link: a link inside is an operand, read
            // as an argument is.
            let place = if links.is_empty() { expected } else { None };
            let (kind, ty) = self.binary_link(&link, inside.take(), place);
            if links.is_empty() {
                return (kind, ty);
            }
            // As `expression` would have made it.
            if matches!(kind, ExprKind::Unread) {
                self.forget();
            }
            inside = Some(Expr {
                kind,
                ty,
                at: self.at(&link),
                category: ValueCategory::Unsettled,
            });
        }
    }

    /// One link of a chain of binary operators: `left operator right`.
    /// `read` is the left operand, when it is the link inside this one
    /// and has been read.
    ///
    /// `&&` and `||` are read as they are written (EXPR-6). Every other
    /// symbol stands for an operator of core's module (the generics
    /// chapter: "Syntax binds these core identities independently of
    /// local same-named functions"), and is read as the call of it: see
    /// [`Typer::symbol`].
    fn binary_link(
        &mut self,
        node: &SyntaxNode,
        read: Option<Expr>,
        place: Option<&Type>,
    ) -> (ExprKind, Type) {
        let config = child(node, SyntaxKind::OperatorConfig);
        let mut read = read;
        let mut operands = node
            .children()
            .filter(|part| part.kind() != SyntaxKind::OperatorConfig);
        let operator = tokens(node).find_map(|token| binary_operator(token.kind()));
        let operator_at = tokens(node)
            .find(|token| binary_operator(token.kind()).is_some())
            .map_or(self.at(node), |token| {
                Location::new(self.file, token.span())
            });
        let (Some(left), Some(right), Some(operator)) =
            (operands.next(), operands.next(), operator)
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if matches!(operator, BinaryOperator::And | BinaryOperator::Or) {
            if let Some(identity) = logical_identity(node, operator) {
                let _ = self.symbol_operator(identity, operator_at);
            }
            // Configuration of these is for the pass that reads them
            // through core's `and_` and `or_`.
            if config.is_some() {
                return (ExprKind::Unread, Type::Unknown);
            }
            // EXPR-6: "Boolean connectives short-circuit left to right."
            // Their operands are bools.
            let what = format!("an operand of `{}`", operator.symbol());
            let left = match read.take() {
                Some(left) => left,
                None => self.expression(&left, Some(&BOOL)),
            };
            let left = self.fit(left, &BOOL, &what);
            // EXPR-6: the right operand is not evaluated when the left
            // decides. What it assigns is assigned only on the path
            // that evaluates it, and a `return` in it does not end
            // every path (CTRL-11, CTRL-5).
            let before = self.assigned.clone();
            let before_returned = self.returned;
            // ERR-3 has "short-circuit paths" tracked, and ERR-12 the
            // same of null tests: the right operand of `&&` is reached
            // only where the left holds, and of `||` only where it does
            // not.
            let (when_true, when_false) = self.null_tests(&left);
            let frame = self.present.len();
            self.present.extend(match operator {
                BinaryOperator::And => when_true,
                _ => when_false,
            });
            let right = self.expression(&right, Some(&BOOL));
            self.present.truncate(frame);
            self.assigned = joined(
                (&self.assigned, self.returned),
                (&before, before_returned),
                self.locals.len(),
            );
            self.returned = before_returned;
            let right = self.fit(right, &BOOL, &what);
            let kind = ExprKind::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
            return (kind, BOOL);
        }
        // TST-4: "Harness comparison is intrinsic: compare presence,
        // exact present data, and shape/length; overload candidates
        // cannot change it."
        if self.testing
            && read.is_none()
            && config.is_none()
            && matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
            && (self.gives_trace(&left) || self.gives_trace(&right))
        {
            return self.trace_comparison(operator, &left, &right);
        }
        // A null presence check is intrinsic, unlike the calls of a
        // comparison identity (GEN-4 and the generics chapter).
        let identity = if matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
            && (written_null(&left) || written_null(&right))
        {
            None
        } else {
            self.symbol_operator(operator.identity(), operator_at)
        };
        // The generics chapter: a symbol's "operands are read as that
        // call's arguments are". Each is read with nothing expected of
        // it, and one with no type of its own, a `.member` or a
        // sequence written out, again as the parameter of the candidate
        // selected. EXPR-13: "left operand, configuration entries in
        // their written order, then right operand".
        let left = match read.take() {
            Some(value) => Given::read(0, left, value),
            None => self.operand(0, left),
        };
        let (entries, faulty) = match (&config, identity) {
            (Some(config), Some(identity)) => {
                self.configuration(operator.symbol(), config, identity)
            }
            // Configuration of an operator the program does not have.
            (Some(_), None) => return (ExprKind::Unread, Type::Unknown),
            (None, _) => (Vec::new(), false),
        };
        let right = self.operand(1, right);
        self.symbol(
            (operator, operator_at),
            identity,
            (left, right),
            (entries, faulty, config.is_some()),
            place,
        )
    }

    /// A symbol between two operands, as the call of the operator it
    /// stands for. `identity` is that operator, when the program has
    /// it; `entries` the configuration's entries, whether one of them
    /// was reported, and whether a configuration is written; `place`
    /// the type the symbol's place expects.
    ///
    /// The generics chapter: "A symbol is the call of its identity: its
    /// operands are read as that call's arguments are, the type its
    /// place expects is requested of the candidates (GEN-5), and its
    /// configuration gives the call's named arguments (EXPR-10)." The
    /// candidates of core's operators say what a symbol admits: NUM-1
    /// for numbers, OPS-1's table for the others, OPS-2 for the
    /// comparisons. "Every other pairing is a source rejection."
    ///
    /// An operand whose type is not known and a nullable one, whose
    /// payload needs its guard, are not judged here, and neither is a
    /// test against `null`: the symbol is then recorded as it is
    /// written. An operand that is a generic parameter has the
    /// operations its declaration requires (GEN-7).
    /// The generics chapter: "null presence checks ... have their own
    /// language rules; token shape alone does not make them extensible
    /// candidate calls."
    pub(super) fn symbol(
        &mut self,
        (operator, at): (BinaryOperator, Location),
        identity: Option<OperatorRef>,
        (left, right): (Given, Given),
        (entries, faulty, configured): (Vec<Given>, bool, bool),
        place: Option<&Type>,
    ) -> (ExprKind, Type) {
        use BinaryOperator as Op;
        let comparison = matches!(
            operator,
            Op::Equal | Op::NotEqual | Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual
        );
        let symbol = Symbol {
            form: Form::Binary(operator),
            text: operator.symbol(),
            at,
            rule: if comparison || matches!(operator, Op::Compare) {
                "OPS-2"
            } else {
                "OPS-1"
            },
            // What the symbol gives when no candidate is selected: a
            // comparison is a bool whichever candidate it is.
            unjudged: if comparison { BOOL } else { Type::Unknown },
            place: place.filter(|place| !open(place)).cloned(),
        };
        // A nullable operand of `==` or `!=` is a test of presence, or a
        // comparison this pass does not judge. Of any other symbol it
        // is an operand as another is: no candidate takes one, unless a
        // module declares it, and the symbol is then rejected (ERR-12).
        // An operand whose type is known only in part says nothing of
        // the candidates either: a call that leaves a generic parameter
        // for its place to say, `repeat(1)`.
        let equality = matches!(operator, Op::Equal | Op::NotEqual);
        let as_written = |operand: &Given| {
            is_null(&operand.node)
                || !operand.by_place
                    && match &operand.value.ty {
                        Type::Nullable(_) => equality,
                        ty => unjudged_operand(ty) || !whole(ty),
                    }
        };
        // GEN-7: an operand that is a generic parameter of the enclosing
        // declaration has the operations its requirements state:
        // `lhs + rhs` under `requires add_(T, T) -> T`.
        if !configured
            && !faulty
            && let Some(operator) = identity
            && let Some(result) = self.required(operator, &[&left, &right], (None, place), None)
        {
            return self.call_required(operator, result, vec![left, right], symbol.form);
        }
        let identity = identity.filter(|_| !(as_written(&left) || as_written(&right)));
        let Some(identity) = identity else {
            let (left_node, right_node) = (left.node.clone(), right.node.clone());
            let (left, right) = self.operands_as_written(left, right);
            // ERR-15: a result "compared with `null`, where T is not
            // nullable, ... expects the `T?`", and where T is nullable
            // the T; beside any other operand that is no result, the T.
            let (left_type, right_type) = (left.ty.clone(), right.ty.clone());
            let left = self.operand_form(left, &right_type, is_null(&right_node));
            let right = self.operand_form(right, &left_type, is_null(&left_node));
            if !configured {
                let kind = ExprKind::Binary {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                };
                return (kind, symbol.unjudged);
            }
            let mut given = vec![Given::read(0, left_node, left)];
            given.extend(entries);
            given.push(Given::read(1, right_node, right));
            return (self.unselected(given, symbol.form), symbol.unjudged);
        };
        let mut given = vec![left];
        given.extend(entries);
        given.push(right);
        if faulty {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        self.call_symbol(symbol, identity, given)
    }

    /// An operand `value` of `==` or another symbol recorded as it is
    /// written, beside `other`, in the form its place takes: a result
    /// compared with `null` (`null_beside`) takes the form ERR-15 gives
    /// that comparison; one beside an operand that is no result, its T
    /// ("An operand of an operator expects the T where the operator's
    /// candidates take one"); beside a result, itself, as `==` of two
    /// results takes them whole (ERR-2).
    fn operand_form(&mut self, value: Expr, other: &Type, null_beside: bool) -> Expr {
        if !matches!(value.ty, Type::Result { .. }) {
            return value;
        }
        if null_beside {
            return self.compared_with_null(value);
        }
        if matches!(other, Type::Result { .. }) || open(other) {
            return value;
        }
        self.received(value, |_| false)
    }

    /// The operands of a symbol that is recorded as it is written, each
    /// read where it stands.
    ///
    /// TYPE-7: `.member` "names a member of the enum type its position
    /// expects": beside an operand, that operand's. "Parentheses select
    /// grouping" (EXPR-1): `(.buy)` is `.buy`. A `null` beside an
    /// operand is of that operand's type when it is nullable; beside
    /// any other, what it is of is not known here. So an operand whose
    /// first reading was taken back is read again with the other's
    /// type before it.
    fn operands_as_written(&mut self, left: Given, right: Given) -> (Expr, Expr) {
        // An operand read again: as the other's type where it takes its
        // type from it, and with nothing expected of it where it does
        // not, so that what it leaves unresolved is reported
        // (`generic.unresolved` for the `repeat(1)` of
        // `repeat(1) == repeat(2)`).
        //
        // `other` is the other operand's type where it has been read,
        // and `None` where it has not yet. A sequence or a tuple
        // written out beside an operand whose type is not known has
        // nothing to be read as, and that is the other operand's doing:
        // nothing is reported of it (ERR-9). So `eval f(...) == []` says
        // nothing of its `[]` while `eval` is not read.
        let reread = |this: &mut Self, operand: &Given, other: Option<&Type>| {
            if takes_its_type(&operand.node) {
                let expected = beside(&operand.node, other.unwrap_or(&Type::Unknown));
                this.expression(&operand.node, Some(&expected))
            } else if operand.by_place && other.is_some_and(open) {
                this.expression(&operand.node, Some(&Type::Unknown))
            } else {
                this.expression(&operand.node, None)
            }
        };
        match (left.retried, right.retried) {
            (false, false) => (left.value, right.value),
            (false, true) => {
                let right = reread(self, &right, Some(&left.value.ty));
                (left.value, right)
            }
            (true, false) => {
                let left = reread(self, &left, Some(&right.value.ty));
                (left, right.value)
            }
            // One that takes its type from the other is read after it.
            (true, true) if takes_its_type(&left.node) && !takes_its_type(&right.node) => {
                let right = reread(self, &right, None);
                let left = reread(self, &left, Some(&right.ty));
                (left, right)
            }
            (true, true) => {
                let left = reread(self, &left, None);
                let right = reread(self, &right, Some(&left.ty));
                (left, right)
            }
        }
    }

    /// An operand of a symbol, read as an argument is: with nothing
    /// expected of it. A reading that reports nothing and settles no
    /// whole type is taken back, as one of an operand with no type of
    /// its own is, and the operand is read again: as the selected
    /// candidate's parameter, or where the symbol is recorded as it is
    /// written, by [`Typer::operands_as_written`].
    pub(super) fn operand(&mut self, position: usize, node: SyntaxNode) -> Given {
        let by_place = typed_by_place(&node);
        let mark = self.mark();
        let value = self.expression(&node, Some(&Type::Unknown));
        let retried = (by_place || !whole(&value.ty))
            && self.diagnostics.len() == mark.diagnostics
            && self.may_reread();
        if retried {
            self.take_back(mark, true);
        }
        Given {
            key: Key::Position(position),
            by_place,
            value,
            explicit: None,
            retried,
            node,
        }
    }

    /// The operator of core's module that a symbol stands for, by its
    /// name: `add_` for `+`. GEN-4 and UNIT-3 apply to symbol calls too.
    pub(super) fn symbol_operator(
        &mut self,
        name: &'static str,
        at: Location,
    ) -> Option<OperatorRef> {
        let program = self.program;
        let found = self.identities.entry(name).or_insert_with(|| {
            let core = program.core()?;
            let module = &program.modules[core];
            let declaration = module
                .scope
                .get(name)?
                .iter()
                .copied()
                .find(|&declaration| {
                    module.declarations[declaration].kind == DeclarationKind::Operator
                })?;
            Some(OperatorRef {
                module: core,
                declaration,
            })
        });
        let identity = (*found)?;
        let core = &program.modules[identity.module];
        let declaration = &core.declarations[identity.declaration];
        if core.path != self.module.path && !declaration.is_public() {
            self.diagnostics.emit(
                error(
                    Category::Operator,
                    codes::OPERATOR_NO_CANDIDATE,
                    at,
                    format!("this symbol calls `{name}`, which is private to module `core`"),
                    "GEN-4",
                )
                .with_related(declaration.at, "declared here"),
            );
            return None;
        }
        Some(identity)
    }

    /// The entries of a symbol's configuration, `{zero: .error}`, each
    /// read as a named argument of a call of `operator`, and whether
    /// one of them was reported or is of no known type.
    ///
    /// EXPR-10: "Bind these arguments to the same core identity as the
    /// named arguments of a call of it: to the operator's parameters
    /// and the options its candidates declare (GEN-6), with their
    /// roles, defaults, and ordinary argument checks."
    fn configuration(
        &mut self,
        symbol: &str,
        config: &SyntaxNode,
        operator: OperatorRef,
    ) -> (Vec<Given>, bool) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return (Vec::new(), true);
        };
        let candidates = self.value_candidates(operator);
        let options = options_of(public, &candidates);
        let reported = self.diagnostics.len();
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut given = Vec::new();
        for entry in config
            .children()
            .filter(|entry| entry.kind() == SyntaxKind::ConfigArg)
        {
            // The entry's name is its first token: any word (LEX-9).
            let (Some(name), Some(value)) = (tokens(&entry).next(), entry.children().next()) else {
                continue;
            };
            let text = name.text();
            let at = Location::new(self.file, name.span());
            let key = options.name_key(text, true);
            // Ordinary public parameters are supplied by the operands.
            // A configuration parameter or generic value parameter of
            // the same name takes precedence in braces (EXPR-10).
            let operand =
                key.public_parameter(public).is_some() && !matches!(key, Key::Configuration(_));
            if key.public_parameter(public).is_none() && !options.accepts_name(&key) {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_UNKNOWN,
                    at,
                    format!("`{symbol}` has no option `{text}`"),
                    "EXPR-10",
                ));
            } else if operand || !names.insert(text.to_string()) {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    at,
                    format!("`{text}` is given twice"),
                    "EXPR-10",
                ));
            }
            given.push(self.give(key, value));
        }
        self.constants_given(&given, public);
        let faulty = self.diagnostics.len() != reported
            || given
                .iter()
                .any(|given| !given.by_place && open(&given.value.ty));
        (given, faulty)
    }

    /// Checks each candidate-only selector as a constant where the call
    /// stands. The public values were read with the public signature.
    ///
    /// GEN-8: "The value a call gives a generic value parameter is a
    /// compile-time constant ... Any other rejects."
    pub(super) fn constants_given(&mut self, given: &[Given], public: &Signature) {
        for given in given {
            let Key::Generic(name) = &given.key else {
                continue;
            };
            if given.key.generic_parameter(public).is_some() {
                continue;
            }
            // Its type is the candidate's to say (GEN-6): it is read as a
            // value of that once the candidate is (`instance`). Whether
            // it is a constant at all is asked here, once.
            let value = self.read_constant(&given.node, None);
            if let Some(why) = Self::not_constant(&value) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(&given.node),
                    format!(
                        "`{name}` is a generic value parameter, and this is no compile-time \
                         constant: {why}"
                    ),
                    "GEN-8",
                ));
            }
        }
    }

    /// The call a symbol stands for: of `operator`, with `given`, the
    /// operands in their places and the configuration's entries by
    /// their names.
    ///
    /// GEN-5: "Select exactly one most-specific admitted candidate ...
    /// No match rejects; tied most-specific candidates are ambiguous."
    /// A symbol's operands select as the arguments of the call do. The
    /// type its place expects is not asked of the candidates: the
    /// result is checked against it where the symbol stands.
    fn call_symbol(
        &mut self,
        symbol: Symbol,
        operator: OperatorRef,
        given: Vec<Given>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        };
        let candidates = self.value_candidates(operator);
        if unreadable(public, &candidates) {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        if self.missing_configuration(public, &given, symbol.text, symbol.at) {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        let own_generics: Vec<Location> =
            public.generics.iter().map(|generic| generic.at).collect();
        let reported = self.diagnostics.len();
        let mut found = vec![None; public.generics.len()];
        let said = self.operator_generic_arguments(
            public,
            given.iter().map(|given| (&given.key, &given.node)),
            &mut found,
        );
        if self.diagnostics.len() != reported {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        let (requested, among) = if said {
            from_place(public, symbol.place.as_ref(), &mut found);
            let supplied = public_inputs(public, &given);
            let requested = self.output_requested(operator, public, found, &supplied, true);
            if let Some(unfit) = &requested.unfit {
                self.diagnostics.emit(unfit.diagnostic(public, symbol.at));
                return (self.unselected(given, symbol.form), symbol.unjudged);
            }
            (requested.place, requested.among)
        } else {
            (
                None,
                self.result_among(operator, public, &[], &own_generics),
            )
        };
        let wanted = Wanted {
            place: requested.as_ref().or(symbol.place.as_ref()),
            among,
            written: false,
        };
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        // GEN-7: "A generic body is checked under its stated
        // requirements". Where which candidates admit the symbol cannot be
        // told for a generic parameter of the declaration being read,
        // what its `requires` states settles it
        // ([`Typer::select_per_instance`]). The symbol's expression is the
        // one its first operand is part of.
        // Where it is rejected the symbol's own report is made, with
        // what says why beside it.
        let mut unsettled: Vec<String> = Vec::new();
        {
            let written = given
                .iter()
                .find(|given| matches!(given.key, Key::Position(0)))
                .and_then(|given| given.node.parent())
                .map_or(symbol.at, |node| self.at(&node));
            let settled = self.select_per_instance(
                operator,
                &given,
                (&wanted, None),
                (symbol.form, written, symbol.at),
                untold,
            );
            match settled {
                Settled::Deferred(ty) => {
                    let ty = if matches!(ty, Type::Unknown) {
                        symbol.unjudged
                    } else {
                        ty
                    };
                    return (self.unselected(given, symbol.form), ty);
                }
                Settled::Elsewhere => {}
                Settled::Rejected(report) => {
                    if report.code.as_str() == codes::OPERATOR_AMBIGUOUS
                        && report.rule == Some("ERR-15")
                    {
                        self.diagnostics.emit(report);
                        return (self.unselected(given, symbol.form), Type::Unknown);
                    }
                    unsettled = report.notes;
                    selected.clear();
                }
            }
        }
        let untold = untold && unsettled.is_empty();
        // Which candidates admit the call cannot be told, or an entry
        // could not be tried once the function's readings were used up
        // (`REREADS`): the symbol is then not judged.
        if untold || self.rereads == 0 && given.iter().any(|given| given.by_place) {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        if selected.len() == 1
            && let Some(chosen) = selected.pop()
        {
            if let Some(diagnostic) = self.ambiguous_result_reads(&chosen, symbol.at) {
                self.diagnostics.emit(diagnostic);
                return (self.unselected(given, symbol.form), Type::Unknown);
            }
            return self.called(operator, chosen, given, (symbol.form, symbol.at));
        }
        // `i64` and `f64`, and after them what the configuration gives:
        // `zero: .nan`.
        let operands: Vec<String> = given
            .iter()
            .filter(|given| matches!(given.key, Key::Position(_)))
            .map(|given| {
                // An operand with no type of its own is named as it is
                // written: `.buy`.
                if given.by_place {
                    format!("`{}`", given.node.text().to_string().trim())
                } else {
                    format!("`{}`", given.value.ty)
                }
            })
            .collect();
        let entries: Vec<String> = given
            .iter()
            .filter_map(|given| {
                let name = given.key.name()?;
                let value = if given.by_place {
                    given.node.text().to_string().trim().to_string()
                } else {
                    given.value.ty.to_string()
                };
                Some(format!("`{name}: {value}`"))
            })
            .collect();
        let mut takes = operands.join(" and ");
        if !entries.is_empty() {
            takes = format!("{takes} with {}", entries.join(", "));
        }
        // GEN-5: "the requested output before inputs".
        if let Some(place) = &symbol.place {
            takes = format!("{takes} to give `{place}`");
        }
        let diagnostic = if selected.is_empty() {
            error(
                Category::Operator,
                codes::OPERATOR_NO_CANDIDATE,
                symbol.at,
                format!("`{}` takes no {takes}", symbol.text),
                symbol.rule,
            )
        } else {
            let name = &program.modules[operator.module].declarations[operator.declaration].name;
            let mut diagnostic = error(
                Category::Operator,
                codes::OPERATOR_AMBIGUOUS,
                symbol.at,
                format!(
                    "{} value candidates of operator `{name}`, which `{}` stands for, take \
                     {takes} equally",
                    selected.len(),
                    symbol.text
                ),
                "GEN-5",
            );
            for chosen in &selected {
                let candidate = &program.modules[chosen.module].declarations[chosen.declaration];
                diagnostic = diagnostic.with_related(candidate.at, "this candidate");
            }
            diagnostic
        };
        let diagnostic = unsettled
            .into_iter()
            .fold(diagnostic, |diagnostic, note| diagnostic.with_note(note));
        self.diagnostics.emit(diagnostic);
        // What the place expects was asked of the candidates and none
        // gives it: the place is not then told the symbol is of another
        // type (ERR-9).
        let ty = match symbol.place {
            Some(_) => Type::Unknown,
            None => symbol.unjudged,
        };
        (self.unselected(given, symbol.form), ty)
    }
}

/// True when an operator's contract is not applied to an operand of
/// this type here: it is not known, or it is nullable, and its payload
/// needs its guard first. A generic parameter is judged: it has the
/// operations its declaration's requirements state (GEN-7), and those
/// of a candidate that takes any type.
fn unjudged_operand(ty: &Type) -> bool {
    open(ty) || matches!(ty, Type::Nullable(_))
}

/// True for a link of a chain of binary operators that is not read:
/// `&&` or `||` with configuration.
fn unread_link(node: &SyntaxNode) -> bool {
    child(node, SyntaxKind::OperatorConfig).is_some()
        && tokens(node)
            .find_map(|token| binary_operator(token.kind()))
            .is_some_and(|operator| matches!(operator, BinaryOperator::And | BinaryOperator::Or))
}

/// A symbol being read as the call of the operator it stands for.
struct Symbol {
    /// How the call is written.
    form: Form,
    /// The symbol: `+`.
    text: &'static str,
    /// Where it is written.
    at: Location,
    /// The rule that says what the symbol admits.
    rule: &'static str,
    /// The type of the expression where no candidate is selected.
    unjudged: Type,
    /// The type the symbol's place expects, when it is known: the
    /// output requested of the candidates (GEN-5).
    place: Option<Type>,
}

/// What is expected of an expression that takes its type from its
/// place, `node`, beside an operand of type `other`: for `.member`, the
/// operand's type; for `null`, the operand's type when it is nullable.
fn beside(node: &SyntaxNode, other: &Type) -> Type {
    if is_implied_member(node) || matches!(other, Type::Nullable(_)) {
        other.clone()
    } else {
        Type::Unknown
    }
}

/// True when an expression is the literal `null`, in parentheses or not.
pub(super) fn is_null(node: &SyntaxNode) -> bool {
    takes_its_type(node) && !is_implied_member(node)
}

/// True when an expression is `.member`, in parentheses or not.
fn is_implied_member(node: &SyntaxNode) -> bool {
    ungrouped(node.clone()).kind() == SyntaxKind::ImpliedMember
}

/// Check the visibility of the Boolean identity once at the outer link
/// of an unparenthesized chain (the generics chapter). A grouped link
/// has a parenthesis node between it and the surrounding operator.
fn logical_identity(node: &SyntaxNode, operator: BinaryOperator) -> Option<&'static str> {
    let same_link = |part: SyntaxNode| {
        part.kind() == SyntaxKind::BinaryExpr
            && tokens(&part).find_map(|token| binary_operator(token.kind())) == Some(operator)
    };
    if node.parent().is_some_and(same_link) {
        return None;
    }
    let identity = if node.children().any(same_link) {
        if operator == BinaryOperator::And {
            "all_"
        } else {
            "any_"
        }
    } else {
        operator.identity()
    };
    Some(identity)
}

/// The operator a token is, between two operands.
fn binary_operator(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::Plus => BinaryOperator::Add,
        SyntaxKind::Minus => BinaryOperator::Subtract,
        SyntaxKind::Star => BinaryOperator::Multiply,
        SyntaxKind::Slash => BinaryOperator::Divide,
        SyntaxKind::SlashSlash => BinaryOperator::FloorDivide,
        SyntaxKind::Percent => BinaryOperator::Modulo,
        SyntaxKind::EqEq => BinaryOperator::Equal,
        SyntaxKind::BangEq => BinaryOperator::NotEqual,
        SyntaxKind::Lt => BinaryOperator::Less,
        SyntaxKind::LtEq => BinaryOperator::LessEqual,
        SyntaxKind::Gt => BinaryOperator::Greater,
        SyntaxKind::GtEq => BinaryOperator::GreaterEqual,
        SyntaxKind::Spaceship => BinaryOperator::Compare,
        SyntaxKind::AmpAmp => BinaryOperator::And,
        SyntaxKind::PipePipe => BinaryOperator::Or,
        _ => return None,
    })
}
