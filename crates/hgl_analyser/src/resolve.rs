//! Resolves the names used inside declarations (UNIT-10).
//!
//! > A name resolves to the nearest binding in scope for it: an earlier local
//! > or iteration binding of an enclosing block; a parameter of an
//! > enclosing function; a generic parameter of an enclosing declaration;
//! > a module declaration; a selective import; or the prelude.
//!
//! This pass walks every signature, body, type and constraint. Where a
//! name is bound (a `let`, a parameter) it adds the name to the scope,
//! and rejects it when an enclosing lexical scope already binds it (UNIT-12).
//! A use starts its lookup at the innermost scope and records what it finds.
//!
//! Scopes nest as the source does. A *frame* holds the bindings of one
//! level: a declaration (its generic parameters and parameters), a block,
//! the pattern of a `for`, an anonymous function. The frames in use form
//! a stack; a lookup reads it from the top.
//!
//! What a name *is* does not matter here: a type, a function and a local
//! are found the same way. Whether the thing found fits where it is used
//! is for the passes that know types.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::hir::{self, Callee, EndpointOperation, ExprKind};
use crate::program::{
    Binding, Decision, DeclarationKind, ImportKind, LocalKind, Module, NameUse, PreludeName,
    Program, RUN_CONFIG,
};
use crate::syntax::{
    child, has_token, is_arg_keys_call, leads_its_test, name_before_less_than, name_token,
    pack_for_names, parent_outside_grouping, suppresses_warning, tokens, ungrouped,
};
use crate::{Detail, INJECTABLES, RESULT_FORMS, Type, codes, error, warning};

/// Resolves the names of every module, filling in [`Module::uses`] and
/// [`Module::decisions`] and adding to [`Module::prelude_names`].
pub(crate) fn resolve(program: &mut Program, diagnostics: &mut Diagnostics) {
    for index in 0..program.modules.len() {
        // A module is made from at least one file.
        let Some(first) = program.modules[index].files.first() else {
            continue;
        };
        // The resolver reads the whole program and so cannot change it.
        // What it finds is taken out when it is done, and written back.
        let mut resolver = Resolver {
            program,
            module: &program.modules[index],
            index,
            file: 0,
            file_id: first.file,
            in_test: false,
            test_owner: None,
            configures: false,
            frames: Vec::new(),
            hidden: Vec::new(),
            diagnostics,
            uses: Vec::new(),
            contextual_type_reports: HashMap::new(),
            default_type_reports: HashMap::new(),
            hidden_callables: HashMap::new(),
            prelude: Vec::new(),
            decisions: Vec::new(),
        };
        resolver.module();
        let (
            mut uses,
            prelude,
            decisions,
            hidden_callables,
            contextual_type_reports,
            default_type_reports,
        ) = (
            resolver.uses,
            resolver.prelude,
            resolver.decisions,
            resolver.hidden_callables,
            resolver.contextual_type_reports,
            resolver.default_type_reports,
        );

        let module = &mut program.modules[index];
        // File order, then source order.
        let position: HashMap<FileId, usize> = module
            .files
            .iter()
            .enumerate()
            .map(|(position, file)| (file.file, position))
            .collect();
        uses.sort_by_key(|used| (position[&used.at.file], used.at.span.start));
        module.uses = uses;
        module.decisions = decisions;
        module.hidden_callables = hidden_callables;
        module.contextual_type_reports = contextual_type_reports;
        module.default_type_reports = default_type_reports;
        for name in prelude {
            if !module
                .prelude_names
                .iter()
                .any(|known| known.name == name.name)
            {
                module.prelude_names.push(name);
            }
        }
    }
}

/// Warn once at each function-typed binding that hides a callable (UNIT-12).
pub(crate) fn warn_hidden_callables(program: &Program, diagnostics: &mut Diagnostics) {
    let code = codes::NAME_HIDES_FUNCTION;
    for module in &program.modules {
        if module.hidden_callables.is_empty() {
            continue;
        }
        let mut remaining = module.hidden_callables.clone();
        let mut report = |name: &str, at: Location, ty: &Type, suppressed: bool| {
            if let Some(hidden) = remaining.remove(&at)
                && matches!(ty, Type::Function(_))
                && !suppressed
            {
                diagnostics.emit(
                    warning(
                        Category::Name,
                        code,
                        at,
                        format!("the function-typed binding `{name}` hides a function or operator"),
                        "UNIT-12",
                    )
                    .with_related(hidden, "the hidden declaration is here"),
                );
            }
        };
        for (index, declaration) in module.declarations.iter().enumerate() {
            let suppressed = suppresses_warning(&declaration.syntax, code);
            if let Some(Detail::Callable(signature)) = module.details.get(index) {
                for parameter in &signature.parameters {
                    report(
                        &parameter.name,
                        parameter.at,
                        &parameter.ty,
                        suppressed || parameter.pack.is_some(),
                    );
                }
            }
            let mut local =
                |bound: &hir::Local| report(&bound.name, bound.at, &bound.ty, suppressed);
            if let Some(Some(body)) = module.bodies.get(index) {
                visit_body_bindings(body, &mut local);
            }
            let defaults = module.defaults.get(index).into_iter().flatten().flatten();
            for written in defaults.chain(module.constants.get(index).and_then(Option::as_ref)) {
                written.locals.iter().for_each(&mut local);
                written
                    .value
                    .walk(&mut |expr| visit_anonymous_bindings(expr, &mut local));
            }
        }
        for (test, body) in module.tests.iter().zip(&module.test_bodies) {
            if let Some(body) = body {
                let suppressed = suppresses_warning(&test.syntax, code);
                visit_body_bindings(body, &mut |bound| {
                    report(&bound.name, bound.at, &bound.ty, suppressed);
                });
            }
        }
        let mut local = |bound: &hir::Local| report(&bound.name, bound.at, &bound.ty, false);
        if let Some(body) = &module.store_body {
            visit_body_bindings(body, &mut local);
        }
        if let Some(run) = &module.run_body {
            run.locals.iter().for_each(&mut local);
            run.call
                .walk(&mut |expr| visit_anonymous_bindings(expr, &mut local));
        }
    }
}

fn visit_body_bindings(body: &hir::Body, visit: &mut dyn FnMut(&hir::Local)) {
    body.locals.iter().for_each(&mut *visit);
    hir::walk_statements(&body.statements, &mut |expr| {
        visit_anonymous_bindings(expr, visit);
    });
}

fn visit_anonymous_bindings(expr: &hir::Expr, visit: &mut dyn FnMut(&hir::Local)) {
    let callee = match &expr.kind {
        ExprKind::Call { callee, .. } => callee,
        ExprKind::StaticCallable(callee) => callee.as_ref(),
        _ => return,
    };
    if let Callee::Anonymous(anonymous) = callee {
        visit_body_bindings(&anonymous.body, visit);
    }
}

/// One level of scope.
#[derive(Default)]
struct Frame {
    /// True for the frame of an anonymous function. Looking a name up
    /// past it is a capture (FN-14).
    lambda: bool,
    /// The bindings of this level.
    bound: HashMap<String, (Location, LocalKind)>,
    /// Names this block declares further down. One used before then is
    /// used before its declaration (UNIT-6).
    later: HashMap<String, Location>,
}

/// A name a default may not read (UNIT-7): a parameter of the same
/// declaration, or a field of the same struct.
struct Hidden {
    name: String,
    at: Location,
    what: &'static str,
}

struct Resolver<'a> {
    program: &'a Program,
    /// The module being resolved, and its position in the program.
    module: &'a Module,
    index: usize,
    /// The file being resolved: its position in the module, and itself.
    file: usize,
    file_id: FileId,
    /// True inside a test or a test helper, where the module's test
    /// scope is visible (TST-1).
    in_test: bool,
    /// The named test whose private helpers are visible in this declaration or body (TST-1).
    test_owner: Option<usize>,
    /// True where `inject run_config` has a place, and the configuration
    /// in braces after it is read (RUN-6): an application's statements
    /// before its run statement, and an integration test.
    configures: bool,
    frames: Vec<Frame>,
    hidden: Vec<Hidden>,
    diagnostics: &'a mut Diagnostics,
    uses: Vec<NameUse>,
    /// Value-only reports wait for the GEN-14 argument kind to be known.
    contextual_type_reports: HashMap<Location, Vec<hgl_diagnostics::Diagnostic>>,
    default_type_reports: HashMap<Location, Vec<hgl_diagnostics::Diagnostic>>,
    hidden_callables: HashMap<Location, Location>,
    prelude: Vec<PreludeName>,
    decisions: Vec<Decision>,
}

// ---- Reading the tree ----

/// The identifier tokens directly inside `node`, in order.
fn identifiers(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::Ident)
}

/// True when `node` is the name a call calls, `name(...)`, or is a
/// call that writes generic arguments after its name, `name<T>(...)`.
fn is_callee(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::NameExpr => node.parent().is_some_and(|call| {
            call.kind() == SyntaxKind::CallExpr && call.children().next().as_ref() == Some(node)
        }),
        SyntaxKind::TypedConstructor | SyntaxKind::UndecidedConstructor => true,
        _ => false,
    }
}

/// An unqualified call that keeps its language meaning in any scope
/// (ERR-13, UNIT-10, TIME-9).
fn reserved_call(name: &str) -> bool {
    RESULT_FORMS.contains(&name) || name == "scheduled"
}

/// The name a parameter or generic parameter declares: the last
/// identifier before its `:`. In `const n: i64` that is `n`, and in
/// `const: i64` it is `const` (FN-5).
fn declared_name(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .take_while(|token| token.kind() != SyntaxKind::Colon)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .last()
}

/// The child nodes of a parameter or struct member, split at its `=`:
/// its type, and its default.
fn split_at_default(node: &SyntaxNode) -> (Vec<SyntaxNode>, Vec<SyntaxNode>) {
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut seen = false;
    for element in node.children_with_tokens() {
        if element.kind() == SyntaxKind::Eq {
            seen = true;
        } else if let Some(node) = element.into_node() {
            if seen {
                after.push(node);
            } else {
                before.push(node);
            }
        }
    }
    (before, after)
}

impl Resolver<'_> {
    // ---- Reporting ----

    fn at(&self, token: &SyntaxToken) -> Location {
        Location::new(self.file_id, token.span())
    }

    /// Records a use that resolved, and gives the binding back.
    fn used(&mut self, token: &SyntaxToken, binding: Binding) -> Option<Binding> {
        self.uses.push(NameUse {
            name: token.text().to_string(),
            at: self.at(token),
            binding: binding.clone(),
            documentation_example: self
                .test_owner
                .is_some_and(|test| self.module.tests[test].documentation_example),
        });
        Some(binding)
    }

    // ---- The module ----

    /// Points the resolver at a file of the module.
    fn enter(&mut self, file: usize, in_test: bool) {
        self.file = file;
        self.file_id = self.module.files[file].file;
        self.in_test = in_test;
        self.test_owner = None;
        self.frames.clear();
        self.hidden.clear();
    }

    fn module(&mut self) {
        // `self.module` is a reference that outlives `self`'s borrow,
        // so the loops below may call `&mut self` methods.
        let module = self.module;
        for declaration in &module.declarations {
            self.enter(declaration.file, declaration.test_only);
            self.test_owner = declaration.test_owner;
            self.declaration(&declaration.syntax);
        }
        for (index, test) in module.tests.iter().enumerate() {
            self.enter(test.file, true);
            self.test_owner = Some(index);
            self.configures = test.integration;
            self.children(&test.syntax);
            self.configures = false;
        }
        // UNIT-14: the statements before the run share a block. RUN-6
        // carries its run_config binding into the run statement.
        let configuration = if let Some(first) = module.store.first() {
            self.enter(first.file, false);
            self.configures = true;
            self.frames.push(Frame::default());
            for statement in &module.store {
                self.children(&statement.syntax);
            }
            self.configures = false;
            self.frames
                .pop()
                .and_then(|mut frame| frame.bound.remove_entry(RUN_CONFIG))
        } else {
            None
        };
        if let Some(run) = &module.run {
            self.enter(run.file, false);
            self.frames.push(Frame {
                bound: configuration.into_iter().collect(),
                ..Frame::default()
            });
            // RUN-6: a configuration on an application's run statement
            // rejects (`run.configuration`), and what it holds is not read
            // (ERR-9): only the statement's call is resolved.
            for form in run.syntax.children() {
                for part in form.children() {
                    if part.kind() != SyntaxKind::OperatorConfig {
                        self.walk(&part);
                    }
                }
            }
            self.frames.pop();
        }
        for (file, node) in &module.instantiations {
            self.enter(*file, false);
            self.children(node);
        }
    }

    fn declaration(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::FnDecl | SyntaxKind::OperatorDecl | SyntaxKind::NativeFnDecl => {
                self.callable(node);
            }
            SyntaxKind::StructDecl => self.structure(node),
            // An enum's values and a native type name nothing of their
            // own; what they use is in the module's scope.
            _ => self.children(node),
        }
    }

    /// A function, an operator or a native function.
    fn callable(&mut self, node: &SyntaxNode) {
        self.frames.push(Frame::default());
        // GEN-18: a law default needs the bound function parameter.
        let (law_defaults, mut defaults): (Vec<_>, Vec<_>) = self
            .generic_params(node)
            .into_iter()
            .partition(|expr| crate::laws::predicate(expr).is_some());
        let parameters = crate::read::parameters(node);
        // The types first: they see the generic parameters.
        for parameter in &parameters {
            let (types, initializers) = split_at_default(parameter);
            for part in types {
                self.walk(&part);
            }
            defaults.extend(initializers);
        }
        if let Some(result) = child(node, SyntaxKind::ReturnType) {
            self.walk(&result);
        }
        // UNIT-7: a default is resolved "excluding the callable's formal
        // arguments".
        self.hidden = parameters
            .iter()
            .filter_map(declared_name)
            .map(|name| Hidden {
                name: name.text().to_string(),
                at: self.at(&name),
                what: "a parameter of the same declaration",
            })
            .collect();
        for default in defaults {
            self.walk(&default);
        }
        self.hidden.clear();
        for parameter in &parameters {
            if let Some(name) = declared_name(parameter) {
                // FN-14, FN-15: a `var` parameter names a mutable place.
                let kind = if has_token(parameter, SyntaxKind::KwVar) {
                    LocalKind::Var
                } else {
                    LocalKind::Parameter
                };
                self.bind(&name, kind);
            }
        }
        for default in law_defaults {
            self.walk(&default);
        }
        // FN-8: a pack's `for f` sees the callable's parameters. This is
        // separate from reading the pack's type or the parameter defaults.
        for parameter in &parameters {
            let clause = child(parameter, SyntaxKind::PackType)
                .and_then(|pack| child(&pack, SyntaxKind::PackFor));
            if let Some(clause) = clause
                && let Some((function, exclusions)) = pack_for_names(&clause)
            {
                self.lookup(&function);
                for exclusion in exclusions {
                    self.lookup(&exclusion);
                }
            }
        }
        // The constraints and body see the parameters.
        for part in node.children() {
            match part.kind() {
                SyntaxKind::GenericParams
                | SyntaxKind::ConfigParamList
                | SyntaxKind::ParamList
                | SyntaxKind::ReturnType => {}
                // A native contract's `inject` names stand in the
                // declaration's own scope: it has no block.
                SyntaxKind::NativeContractBody => self.children(&part),
                _ => self.walk(&part),
            }
        }
        self.frames.pop();
    }

    /// Binds generic names and resolves their types. The caller walks their
    /// defaults in the same UNIT-7 scope as the declaration's other defaults.
    fn generic_params(&mut self, declaration: &SyntaxNode) -> Vec<SyntaxNode> {
        let Some(generics) = child(declaration, SyntaxKind::GenericParams) else {
            return Vec::new();
        };
        let parameters: Vec<SyntaxNode> = generics
            .children()
            .filter(|parameter| parameter.kind() == SyntaxKind::GenericParam)
            .collect();
        // GEN-1: "Generic scope covers the whole declaration", the
        // generic parameters themselves included. Every name is bound
        // before any type is read, so that in `<const N: T, T>` the
        // type of `N` finds the `T` written after it.
        for parameter in &parameters {
            if let Some(name) = declared_name(parameter) {
                self.bind(&name, LocalKind::GenericParameter);
            }
        }
        let mut defaults = Vec::new();
        for parameter in &parameters {
            let (types, initializers) = split_at_default(parameter);
            for part in types {
                self.walk(&part);
            }
            defaults.extend(initializers);
        }
        defaults
    }

    fn structure(&mut self, node: &SyntaxNode) {
        self.frames.push(Frame::default());
        let mut defaults = self.generic_params(node);
        // UNIT-7: a default is resolved "excluding ... the struct's
        // instance fields". A field's name is the first token of its
        // member; `time` may be one (LEX-9) and is no name to look up.
        let fields: Vec<Hidden> = node
            .children()
            .filter(|member| member.kind() == SyntaxKind::StructMember)
            .filter_map(|member| identifiers(&member).next())
            .map(|name| Hidden {
                name: name.text().to_string(),
                at: self.at(&name),
                what: "a field of the same struct",
            })
            .collect();
        for part in node.children() {
            match part.kind() {
                SyntaxKind::GenericParams => {}
                SyntaxKind::StructMember => {
                    let (types, initializers) = split_at_default(&part);
                    // `name = value` has no `:`; everything is after `=`.
                    for part in types {
                        self.walk(&part);
                    }
                    defaults.extend(initializers);
                }
                _ => self.walk(&part),
            }
        }
        self.hidden = fields;
        for default in defaults {
            self.walk(&default);
        }
        self.hidden.clear();
        self.frames.pop();
    }

    // ---- Walking ----

    /// Walks the child nodes of `node`.
    fn children(&mut self, node: &SyntaxNode) {
        for part in node.children() {
            self.walk(&part);
        }
    }

    /// Walks one node: binds what it declares, looks up what it uses.
    ///
    /// This function calls itself, through `children`, once for every
    /// level of the tree, so what it keeps on the call stack is paid for
    /// at every level. It therefore only chooses: the work of each kind
    /// of node is in a function of its own, whose locals are on the
    /// stack only while that node is being read. `inline(never)` on
    /// those keeps an optimising build from folding them back in.
    fn walk(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::Block => self.block(node),
            // A named test's helpers are walked with the module's declarations.
            kind if crate::syntax::is_test_declaration(kind) => {}
            SyntaxKind::LocalDecl | SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                self.local(node);
            }
            SyntaxKind::InjectDecl => self.inject(node),
            SyntaxKind::ForStmt => self.for_statement(node),
            SyntaxKind::ListComprehension | SyntaxKind::PackComprehension => {
                self.comprehension(node)
            }
            SyntaxKind::FnExpr => self.function_expression(node),
            SyntaxKind::ConstraintEach | SyntaxKind::ConstraintAny => self.each_constraint(node),
            SyntaxKind::PackMemberType => {
                for name in identifiers(node) {
                    self.lookup(&name);
                }
            }
            SyntaxKind::NameExpr
            | SyntaxKind::NamedType
            | SyntaxKind::TypedConstructor
            | SyntaxKind::FunctionReference
            | SyntaxKind::ConstraintCall
            | SyntaxKind::ConstraintValue
            | SyntaxKind::Instantiation => self.name_use(node),
            SyntaxKind::UndecidedConstructor => self.undecided(node),
            SyntaxKind::BinaryExpr => self.binary(node),
            // The two names of `alias::name` are read by the node that
            // holds them.
            SyntaxKind::QualifiedName => {}
            _ => self.children(node),
        }
    }

    /// `let`, `var`, `state` or `cache`.
    #[inline(never)]
    fn local(&mut self, node: &SyntaxNode) {
        // The initialiser first: `let x = x` does not read itself
        // (UNIT-6: a local is visible after it is declared).
        self.children(node);
        let kind = match node.first_token().map(|token| token.kind()) {
            Some(SyntaxKind::KwVar) => LocalKind::Var,
            Some(SyntaxKind::KwState) => LocalKind::State,
            Some(SyntaxKind::KwCache) => LocalKind::Cache,
            _ => LocalKind::Let,
        };
        if let Some(name) = identifiers(node).next() {
            self.bind(&name, kind);
        }
    }

    /// `inject out, clock`, or `inject run_config{length: 3s}`: what a
    /// configuration in braces holds is read before the names are bound
    /// (RUN-6), as the value code of the statements before. Braces that
    /// are rejected, after any other capability (`inject.configuration`)
    /// or after a `run_config` where it has no place (`inject.position`),
    /// are not read (ERR-9): what they name is not looked up.
    #[inline(never)]
    fn inject(&mut self, node: &SyntaxNode) {
        // TST-17: an integration test injects it "before its other
        // statements"; an application anywhere before its run statement.
        let placed = self.configures && (!self.in_test || leads_its_test(node));
        let mut after_run_config = false;
        for element in node.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::Ident => {
                    after_run_config = token.text() == RUN_CONFIG;
                }
                SyntaxElement::Node(braces)
                    if braces.kind() == SyntaxKind::OperatorConfig
                        && after_run_config
                        && placed =>
                {
                    self.children(&braces);
                }
                _ => {}
            }
        }
        for name in identifiers(node) {
            // TIME-8: "`inject` names no other."
            if !INJECTABLES.contains(&name.text()) {
                self.diagnostics.emit(
                    error(
                        Category::Injectable,
                        codes::INJECT_UNKNOWN_CAPABILITY,
                        self.at(&name),
                        format!("`{}` is not a capability", name.text()),
                        "TIME-8",
                    )
                    .with_note(format!(
                        "`inject` names one of `{}`",
                        INJECTABLES.join("`, `")
                    )),
                );
            }
            // It is bound all the same, so that its uses find it.
            self.bind(&name, LocalKind::Inject);
        }
    }

    /// A node that uses a name: an expression, a type, a constructor, a
    /// constraint, an instantiation.
    #[inline(never)]
    fn name_use(&mut self, node: &SyntaxNode) {
        if !self.default_type_name(node) {
            self.name_of(node);
        }
        self.children(node);
    }

    /// UNIT-7 excludes values. A type in the same scope has its ordinary
    /// binding; GEN-14 decides which reading of a lone generic argument is used.
    fn default_type_name(&mut self, node: &SyntaxNode) -> bool {
        if self.hidden.is_empty() || node.kind() != SyntaxKind::NamedType {
            return false;
        }
        let qualified = child(node, SyntaxKind::QualifiedName);
        let Some(first) = identifiers(qualified.as_ref().unwrap_or(node)).next() else {
            return false;
        };
        if qualified.is_none() && first.text() == "Type"
            || self
                .frames
                .iter()
                .rev()
                .any(|frame| frame.bound.contains_key(first.text()))
            || !self.hidden.iter().any(|hidden| hidden.name == first.text())
        {
            return false;
        }
        let Some(name) = crate::syntax::name_token(node) else {
            return false;
        };
        let ambiguous = child(node, SyntaxKind::GenericArgs).is_none()
            && node
                .parent()
                .is_some_and(|parent| parent.kind() == SyntaxKind::GenericArgs);
        let before = self.diagnostics.len();
        let hidden = std::mem::take(&mut self.hidden);
        self.name_of(node);
        self.hidden = hidden;
        if ambiguous {
            let key = self.at(&name);
            let reports = self.diagnostics.as_slice()[before..].to_vec();
            self.diagnostics.truncate(before);
            if !reports.is_empty() {
                self.default_type_reports.insert(key, reports);
            }
            if let Some(report) = self.default_name_report(&first) {
                self.contextual_type_reports.insert(key, vec![report]);
            }
        }
        true
    }

    /// `name<...>(...)` that reads two ways.
    #[inline(never)]
    fn undecided(&mut self, node: &SyntaxNode) {
        // GEN-14: "directly following (LEX-27) a name resolved to a
        // generic type, function, or operator in an expression, `<`
        // opens generic arguments rather than a comparison."
        let binding = self.name_of(node);
        // ERR-13 and TIME-9: a reserved call is still the form when
        // generic arguments are written. The typing pass reports those
        // arguments; the brackets do not become a comparison.
        let form = child(node, SyntaxKind::QualifiedName).is_none()
            && identifiers(node)
                .next()
                .is_some_and(|name| reserved_call(name.text()));
        self.decisions.push(Decision {
            at: Location::new(self.file_id, node.span()),
            constructor: if form {
                Some(true)
            } else {
                binding.map(|binding| {
                    self.is_generic_type(&binding) || self.is_generic_callable(&binding)
                })
            },
        });
        // The names inside are the same names under either reading, so
        // they are resolved as they stand.
        self.children(node);
    }

    /// `a + b`. A chain of these is as deep as the tree may be, so this
    /// keeps nothing on the stack across its operands.
    fn binary(&mut self, node: &SyntaxNode) {
        self.children(node);
        self.check_application(node);
    }

    /// A block: "A block's scope is the scope around it plus its own
    /// bindings" (UNIT-12).
    ///
    /// "A `switch` case has no scope of its own: its statements bind in
    /// the block around the `switch`." So what a case declares is noted
    /// here, with the block's own statements.
    fn block(&mut self, node: &SyntaxNode) {
        let mut frame = Frame::default();
        self.declared_later(node, &mut frame);
        self.frames.push(frame);
        self.children(node);
        self.frames.pop();
    }

    /// Notes the names the statements of `node` declare in its block:
    /// those of its `let`, `var`, `state`, `cache` and `inject`
    /// statements, and of the same under the cases of a `switch`.
    fn declared_later(&self, node: &SyntaxNode, frame: &mut Frame) {
        for statement in node.children() {
            let names: Vec<SyntaxToken> = match statement.kind() {
                SyntaxKind::LocalDecl | SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                    identifiers(&statement).take(1).collect()
                }
                SyntaxKind::InjectDecl => identifiers(&statement).collect(),
                SyntaxKind::SwitchStmt | SyntaxKind::SwitchCase | SyntaxKind::SwitchDefault => {
                    self.declared_later(&statement, frame);
                    Vec::new()
                }
                _ => Vec::new(),
            };
            for name in names {
                let at = self.at(&name);
                frame.later.entry(name.text().to_string()).or_insert(at);
            }
        }
    }

    /// `for a, b in items { ... }`: the pattern's names are bound for the
    /// body, not for `items`.
    fn for_statement(&mut self, node: &SyntaxNode) {
        // `in` is a name with a meaning only here (LEX-10): the last
        // identifier directly inside the statement, when it is `in`.
        let mut names: Vec<SyntaxToken> = identifiers(node).collect();
        if names.last().is_some_and(|token| token.text() == "in") {
            names.pop();
        }
        for part in node.children() {
            if part.kind() != SyntaxKind::Block {
                self.walk(&part);
            }
        }
        self.frames.push(Frame::default());
        for name in &names {
            self.bind(name, LocalKind::Iteration);
        }
        if let Some(body) = child(node, SyntaxKind::Block) {
            self.walk(&body);
        }
        self.frames.pop();
    }

    /// A loop's name is bound only in its entry expression (EXPR-17, FN-19).
    fn comprehension(&mut self, node: &SyntaxNode) {
        let pack = node.kind() == SyntaxKind::PackComprehension;
        if pack
            && !parent_outside_grouping(node).is_some_and(|parent| {
                parent.kind() == SyntaxKind::PackSpreadArg
                    || (parent.kind() == SyntaxKind::LocalDecl
                        && has_token(&parent, SyntaxKind::KwLet))
            })
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                Location::new(self.file_id, node.span()),
                "a pack loop must be bound with `let` or spread into a call".to_string(),
                "FN-19",
            ));
        }
        let mut parts = node.children();
        let mut kind = LocalKind::Iteration;
        if let Some(sequence) = parts.next() {
            let source = ungrouped(sequence.clone());
            if pack && is_arg_keys_call(&source) {
                kind = LocalKind::PackIterationKey;
                // The name of the type pack is looked up, rather than
                // the contextual `arg_keys` form itself (GEN-17).
                if let Some(arguments) = child(&source, SyntaxKind::ArgList) {
                    self.children(&arguments);
                }
            } else {
                if pack {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        Location::new(self.file_id, sequence.span()),
                        "a pack loop needs `arg_keys` of a type pack".to_string(),
                        "FN-19",
                    ));
                }
                self.walk(&sequence);
            }
        }
        self.frames.push(Frame::default());
        if let Some(name) = identifiers(node).next() {
            self.bind(&name, kind);
        }
        if let Some(entry) = parts.next() {
            self.walk(&entry);
        }
        self.frames.pop();
    }

    /// `fn(x) => ...`: "An anonymous function's scope is the scope around
    /// it plus its parameters" (FN-14).
    fn function_expression(&mut self, node: &SyntaxNode) {
        self.frames.push(Frame {
            lambda: true,
            ..Frame::default()
        });
        let parameters: Vec<_> = child(node, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .collect();
        for parameter in &parameters {
            self.children(parameter);
        }
        if let Some(result) = child(node, SyntaxKind::ReturnType) {
            self.walk(&result);
        }
        for parameter in parameters {
            if let Some(name) = identifiers(&parameter).next() {
                self.bind(&name, LocalKind::Parameter);
            }
        }
        for part in node
            .children()
            .filter(|part| !matches!(part.kind(), SyntaxKind::ParamList | SyntaxKind::ReturnType))
        {
            self.walk(&part);
        }
        self.frames.pop();
    }

    /// `each T in types(Ts) { numeric(T) }`: `T` is bound inside the
    /// braces.
    fn each_constraint(&mut self, node: &SyntaxNode) {
        // The identifiers directly inside are `each`, the name, `in`.
        let name = identifiers(node).nth(usize::from(node.kind() == SyntaxKind::ConstraintEach));
        let mut parts = node.children();
        let mut kind = LocalKind::GenericParameter;
        // What is iterated is outside the scope of the name.
        if let Some(operand) = parts.next() {
            if is_arg_keys_call(&operand) {
                kind = LocalKind::PackKey;
            }
            self.walk(&operand);
        }
        self.frames.push(Frame::default());
        if let Some(name) = name {
            self.bind(&name, kind);
        }
        for part in parts {
            self.walk(&part);
        }
        self.frames.pop();
    }

    // ---- Binding ----

    /// Adds a binding if enclosing lexical scopes leave the name free (UNIT-12).
    fn bind(&mut self, token: &SyntaxToken, kind: LocalKind) {
        let name = token.text();
        let at = self.at(token);
        let innermost = self.frames.len().saturating_sub(1);
        if let Some(frame) = self.frames.last_mut() {
            frame.later.remove(name);
        }

        // ERR-13: "No declaration, import alias, parameter, or local
        // takes one of them (UNIT-12)".
        if RESULT_FORMS.contains(&name) {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    at,
                    format!("`{name}` is the name of a result form, and nothing else takes it"),
                    "ERR-13",
                )
                .with_note(format!(
                    "{} takes none of `{}`",
                    kind.describe(),
                    RESULT_FORMS.join("`, `")
                )),
            );
        }
        if matches!(name, "scheduled" | "modified" | "valid") {
            self.diagnostics.emit(error(
                Category::Name,
                codes::NAME_DUPLICATE,
                at,
                format!("`{name}` names an activation form and cannot be bound here"),
                "UNIT-12",
            ));
            return;
        }

        // A binding of an enclosing block, function or declaration.
        let enclosing = self
            .frames
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, frame)| frame.bound.get(name).map(|found| (depth, *found)));
        if let Some((depth, (first, first_kind))) = enclosing {
            // UNIT-5: "Duplicate ... parameter names ... or local
            // bindings in one scope are rejected."
            let rule = if depth == innermost {
                "UNIT-5"
            } else {
                "UNIT-12"
            };
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    at,
                    format!("`{name}` is already in scope"),
                    rule,
                )
                .with_related(
                    first,
                    format!("{} of that name is here", first_kind.describe()),
                ),
            );
            // The first keeps the name.
            return;
        }

        if let Some(test_local) = self
            .test_declarations(name)
            .and_then(|bound| bound.first())
            .map(|&index| &self.module.declarations[index])
        {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    at,
                    format!("`{name}` is already in scope"),
                    "UNIT-12",
                )
                .with_related(test_local.at, "declared in the enclosing test"),
            );
        } else if let Some(hidden) = self.hidden_callable(name) {
            self.hidden_callables.insert(at, hidden);
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.bound.insert(name.to_string(), (at, kind));
        }
    }

    /// A module, import or prelude callable that an ordinary binding hides.
    fn hidden_callable(&self, name: &str) -> Option<Location> {
        let declaration = if let Some(import) = self.module.files[self.file]
            .imports
            .iter()
            .find(|import| import.name == name)
        {
            if import.kind == ImportKind::Alias {
                return None;
            }
            self.program.modules[import.module?]
                .lookup(name)
                .find(|declared| declared.is_public())
        } else {
            self.module
                .lookup(name)
                .find(|declared| self.in_test || !declared.test_only)
                .or_else(|| {
                    let core = self.program.core().filter(|core| *core != self.index)?;
                    self.program.modules[core]
                        .lookup(name)
                        .find(|declared| declared.is_public())
                })
        }?;
        (declaration.kind == DeclarationKind::Operator || declaration.kind.binding_role().is_some())
            .then_some(declaration.at)
    }

    // ---- Looking up ----

    /// Resolves the name a node uses: its `alias::name`, or its first
    /// identifier.
    fn name_of(&mut self, node: &SyntaxNode) -> Option<Binding> {
        // EXPR-9, TYPE-5: an independently expected struct consumes this
        // word as a field label. Only a dictionary reads it as a name.
        if crate::syntax::is_keyed_field_name(node) {
            let name = crate::syntax::keyed_field_name(node)?;
            let before = self.diagnostics.len();
            let binding = self.lookup(&name);
            let reports = self.diagnostics.as_slice()[before..].to_vec();
            self.diagnostics.truncate(before);
            if !reports.is_empty() {
                self.contextual_type_reports.insert(self.at(&name), reports);
            }
            return binding;
        }
        // TYPE-18: a written type Type always names the descriptor. A lone
        // generic argument shares value/type syntax, however: GEN-14 chooses
        // its meaning once the target slot's kind is known. Record ordinary
        // lookup now, keeping its reports for the reader's value path.
        if node.kind() == SyntaxKind::NamedType
            && child(node, SyntaxKind::QualifiedName).is_none()
            && identifiers(node)
                .next()
                .is_some_and(|name| name.text() == "Type")
        {
            if node
                .parent()
                .is_some_and(|parent| parent.kind() == SyntaxKind::GenericArgs)
                && child(node, SyntaxKind::GenericArgs).is_none()
            {
                let name = identifiers(node).next()?;
                let before = self.diagnostics.len();
                let binding = self.lookup_or_form(&name, true);
                let reports = self.diagnostics.as_slice()[before..].to_vec();
                self.diagnostics.truncate(before);
                if !reports.is_empty() {
                    self.contextual_type_reports.insert(self.at(&name), reports);
                }
                return binding;
            }
            return None;
        }
        if let Some(qualified) = child(node, SyntaxKind::QualifiedName) {
            let mut names = identifiers(&qualified);
            let alias = names.next()?;
            let name = names.next()?;
            return self.qualified(&alias, &name);
        }
        let name = identifiers(node).next()?;
        // ERR-13 and UNIT-10: result forms and the TIME-9 wake selector
        // keep their meaning when called, even when the name is bound.
        if reserved_call(name.text()) && is_callee(node) {
            return None;
        }
        // GEN-17: in a requirement `arg_keys(Ps)` is a language form.
        // The argument still uses normal name resolution.
        if name.text() == "arg_keys"
            && node.kind() == SyntaxKind::NameExpr
            && is_callee(node)
            && node
                .ancestors()
                .any(|part| part.kind() == SyntaxKind::RequiresClause)
        {
            return None;
        }
        // GEN-18: law predicates name a compile-time language form.
        let predicate = if node.kind() == SyntaxKind::ConstraintCall {
            crate::laws::predicate(node)
        } else if is_callee(node) {
            node.parent()
                .and_then(|parent| crate::laws::predicate(&parent))
        } else {
            None
        };
        if predicate.is_some() {
            if node
                .ancestors()
                .any(|parent| parent.kind() == SyntaxKind::RequiresClause)
            {
                return self.lookup_or_form(&name, true);
            }
            if node
                .ancestors()
                .any(|parent| parent.kind() == SyntaxKind::GenericParam)
            {
                return None;
            }
        }
        self.lookup(&name)
    }

    /// The declarations in this test's scope that bind a name (TST-1, FN-4).
    fn test_declarations(&self, name: &str) -> Option<&[usize]> {
        self.module
            .tests
            .get(self.test_owner?)?
            .scope
            .get(name)
            .map(Vec::as_slice)
    }

    /// UNIT-10: resolves a name to its nearest binding in scope.
    fn lookup(&mut self, token: &SyntaxToken) -> Option<Binding> {
        self.lookup_or_form(token, false)
    }

    /// A contextual law form is used only after ordinary bindings have
    /// had their turn (GEN-16, GEN-18).
    fn lookup_or_form(&mut self, token: &SyntaxToken, fallback_form: bool) -> Option<Binding> {
        let name = token.text();
        let at = self.at(token);

        // A local, a parameter or a generic parameter, innermost first.
        let mut captured = false;
        let mut found = None;
        for frame in self.frames.iter().rev() {
            if let Some(&(bound, kind)) = frame.bound.get(name) {
                found = Some((bound, kind, captured));
                break;
            }
            // Past the frame of an anonymous function, a binding is one
            // of the function around it.
            captured |= frame.lambda;
        }
        if let Some((bound, kind, captured)) = found {
            // FN-14: "From the functions and blocks around it, its body
            // uses only parameters, generic parameters, `let` locals,
            // and iteration bindings."
            if captured && !kind.capture_allowed() {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_CAPTURED,
                        at,
                        format!(
                            "an anonymous function cannot use `{name}`, {} of the function around it",
                            kind.describe()
                        ),
                        "FN-14",
                    )
                    .with_related(bound, "declared here"),
                );
            }
            return self.used(token, Binding::Local { at: bound, kind });
        }

        if self.reject_default_name(token) {
            return None;
        }

        // A `use` of the file (UNIT-2: imports bind in their own file).
        let import = self.module.files[self.file]
            .imports
            .iter()
            .find(|import| import.name == name);
        if let Some(import) = import {
            if import.kind == ImportKind::Alias {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::ALIAS_UNQUALIFIED,
                        at,
                        format!("`{name}` is a module alias and names nothing by itself"),
                        "UNIT-2",
                    )
                    .with_related(import.at, "the alias is declared here")
                    .with_note(format!("write `{name}::` and the name of a declaration")),
                );
                return None;
            }
            // An import that names nothing has been reported where it
            // stands.
            let target = import.module?;
            let declarations = self.public(target, name);
            if declarations.is_empty() {
                return None;
            }
            return self.used(
                token,
                Binding::Declarations {
                    module: target,
                    declarations,
                },
            );
        }

        if let Some(bound) = self.test_declarations(name) {
            return self.used(
                token,
                Binding::Declarations {
                    module: self.index,
                    declarations: bound.to_vec(),
                },
            );
        }

        // A declaration of the module.
        if let Some(bound) = self.module.scope.get(name) {
            let visible: Vec<usize> = bound
                .iter()
                .copied()
                .filter(|&index| self.in_test || !self.module.declarations[index].test_only)
                .collect();
            if visible.is_empty() {
                // TST-2: "Production cannot refer to test-only
                // declarations."
                let helper = &self.module.declarations[bound[0]];
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_TEST_ONLY,
                        at,
                        format!("`{name}` is declared for tests and cannot be used here"),
                        "TST-2",
                    )
                    .with_related(helper.at, "declared here, in a test context"),
                );
            }
            let declarations = if visible.is_empty() {
                bound.clone()
            } else {
                visible
            };
            return self.used(
                token,
                Binding::Declarations {
                    module: self.index,
                    declarations,
                },
            );
        }

        // Earlier statements can still see a prelude name that a later
        // local would hide when declared (UNIT-6, UNIT-12).
        if let Some(core) = self.program.core()
            && core != self.index
        {
            let declarations = self.public(core, name);
            if !declarations.is_empty() {
                return self.used(
                    token,
                    Binding::Declarations {
                        module: core,
                        declarations,
                    },
                );
            }
        }

        // UNIT-6: "Ordinary block locals become visible after
        // declaration."
        let later = self
            .frames
            .iter()
            .rev()
            .find_map(|frame| frame.later.get(name).copied());
        if let Some(later) = later {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_USED_BEFORE_DECLARATION,
                    at,
                    format!("`{name}` is used before it is declared"),
                    "UNIT-6",
                )
                .with_related(later, "declared here"),
            );
            return None;
        }

        // TIME-9: "The operations on an endpoint are intrinsics of the
        // language ... These reserved intrinsics cannot be shadowed."
        // No declaration binds one, the prelude's included, so its name
        // is not one the prelude is waited on for.
        if EndpointOperation::language_form(name).is_some() {
            return None;
        }

        if fallback_form {
            return None;
        }

        // The prelude does not bind it, or the program has no module
        // `core`. See `Module::prelude_names`.
        if !self.prelude.iter().any(|known| known.name == name) {
            self.prelude.push(PreludeName {
                name: name.to_string(),
                at,
            });
        }
        // UNIT-10: "A name that none of these binds rejects
        // (`name.unknown`)." The prelude is core's module (UNIT-11): a
        // program that has none is no whole program, and what its
        // prelude would bind is not known.
        //
        // GEN-9, GEN-10: `types(Ts)` and `schemas(pack)` are forms of
        // the language. Leave these names for the passes that check
        // what each form supplies or where it is used.
        let form = matches!(name, "types" | "schemas");
        if self.program.core().is_some() && !form {
            self.diagnostics.emit(error(
                Category::Name,
                codes::NAME_UNKNOWN,
                at,
                format!("nothing binds `{name}` here"),
                "UNIT-10",
            ));
        }
        None
    }

    /// UNIT-7 rejects a formal or field name in a default before looking
    /// for the same name in the enclosing scope.
    fn reject_default_name(&mut self, token: &SyntaxToken) -> bool {
        if let Some(report) = self.default_name_report(token) {
            self.diagnostics.emit(report);
            return true;
        }
        false
    }

    fn default_name_report(&self, token: &SyntaxToken) -> Option<hgl_diagnostics::Diagnostic> {
        let name = token.text();
        let hidden = self.hidden.iter().find(|hidden| hidden.name == name)?;
        Some(
            error(
                Category::Name,
                codes::DEFAULT_FORMAL_ARGUMENT,
                self.at(token),
                format!("a default cannot read `{name}`, {}", hidden.what),
                "UNIT-7",
            )
            .with_related(hidden.at, "declared here"),
        )
    }

    /// UNIT-2: "an alias qualifies a public interface with
    /// `alias::name`."
    fn qualified(&mut self, alias: &SyntaxToken, name: &SyntaxToken) -> Option<Binding> {
        let bound = self
            .frames
            .iter()
            .rev()
            .find_map(|frame| frame.bound.get(alias.text()).copied());
        if bound.is_none() && self.reject_default_name(alias) {
            return None;
        }
        let import = self.module.files[self.file]
            .imports
            .iter()
            .find(|import| import.name == alias.text() && import.kind == ImportKind::Alias);
        let Some(import) = import else {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::ALIAS_UNKNOWN,
                    self.at(alias),
                    format!("`{}` is not a module alias of this file", alias.text()),
                    "UNIT-2",
                )
                .with_note(format!(
                    "declare one with `use <module> as {}`",
                    alias.text()
                )),
            );
            return None;
        };
        if let Some((at, kind)) = bound {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::ALIAS_UNKNOWN,
                    self.at(alias),
                    format!("`{}` does not name a module alias here", alias.text()),
                    "UNIT-2",
                )
                .with_related(at, format!("{} hides the alias", kind.describe()))
                .with_related(import.at, "the alias is declared here"),
            );
            return None;
        }
        // An alias of a module that no file declares has been reported.
        let target = import.module?;
        let module = &self.program.modules[target];
        if module.lookup(name.text()).next().is_none() {
            self.diagnostics.emit(error(
                Category::Name,
                codes::IMPORT_UNKNOWN_NAME,
                self.at(name),
                format!("module `{}` declares no `{}`", module.path, name.text()),
                "UNIT-2",
            ));
            return None;
        }
        let declarations = self.public(target, name.text());
        if declarations.is_empty() {
            self.diagnostics.emit(
                error(
                    Category::Module,
                    codes::IMPORT_PRIVATE,
                    self.at(name),
                    format!("`{}` is private to module `{}`", name.text(), module.path),
                    "UNIT-2",
                )
                .with_note(
                    "a function, a type, a constant or an operator is private unless marked `export` (UNIT-3)",
                ),
            );
            return None;
        }
        self.used(
            name,
            Binding::Declarations {
                module: target,
                declarations,
            },
        )
    }

    /// The positions of the public declarations of `name` in module
    /// `target`.
    fn public(&self, target: usize, name: &str) -> Vec<usize> {
        let module = &self.program.modules[target];
        module
            .scope
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| module.declarations[index].is_public())
            .collect()
    }

    // ---- Generic types (GEN-14) ----

    /// True when a binding is a generic type: a struct with generic
    /// parameters.
    fn is_generic_type(&self, binding: &Binding) -> bool {
        let Binding::Declarations {
            module,
            declarations,
        } = binding
        else {
            return false;
        };
        declarations.iter().any(|&index| {
            let declaration = &self.program.modules[*module].declarations[index];
            declaration.kind == DeclarationKind::Struct
                && child(&declaration.syntax, SyntaxKind::GenericParams).is_some()
        })
    }

    /// True when `binding` is a function or an operator with generic
    /// parameters: a call may write its generic arguments (GEN-8).
    fn is_generic_callable(&self, binding: &Binding) -> bool {
        let Binding::Declarations {
            module,
            declarations,
        } = binding
        else {
            return false;
        };
        declarations.iter().any(|&index| {
            let declaration = &self.program.modules[*module].declarations[index];
            matches!(
                declaration.kind,
                DeclarationKind::Function(_)
                    | DeclarationKind::NativeFunction(_)
                    | DeclarationKind::Operator
            ) && child(&declaration.syntax, SyntaxKind::GenericParams).is_some()
        })
    }

    /// GEN-14: reject a malformed application that the parser read as a
    /// comparison. A callable's opening `<` must touch its name (LEX-27).
    /// A type name cannot be ordered even when the `<` is spaced.
    #[inline(never)]
    fn check_application(&mut self, node: &SyntaxNode) {
        let Some(name) = name_before_less_than(node).and_then(|left| name_token(&left)) else {
            return;
        };
        // The name was resolved when the operand was walked.
        let at = self.at(&name);
        let Some(used) = self.uses.iter().rev().find(|used| used.at == at) else {
            return;
        };
        let kind = if self.is_generic_type(&used.binding) {
            "type"
        } else if self.is_generic_callable(&used.binding)
            && tokens(node).any(|token| {
                token.kind() == SyntaxKind::Lt
                    && token.text_range().start() == name.text_range().end()
            })
        {
            "callable"
        } else {
            return;
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::GENERIC_INVALID_APPLICATION,
                at,
                format!(
                    "`{}` is a generic {kind}: `<` after it opens its arguments, and these \
                     are not arguments",
                    name.text()
                ),
                "GEN-14",
            )
            .with_note(format!(
                "a generic {kind} is applied as `{}<...>(...)`",
                name.text()
            )),
        );
    }
}
