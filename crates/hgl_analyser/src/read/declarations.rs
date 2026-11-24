//! Reads declarations and settles generic parameter kinds.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::decode_integer;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::declared::names_one_of;
use crate::program::{Binding, Declaration, DeclarationKind, LocalKind, Program, Role};
use crate::syntax::{child, has_token, name_token, tokens};
use crate::types::{
    Detail, Field, FunctionParameter, FunctionType, GenericKind, GenericParameter, Pack, Parameter,
    Signature, StructType, Type,
};
use crate::{codes, error};

use super::{
    Id, Position, Reader, default_of, is_const, is_empty_sequence, is_null, is_type,
    names_before_colon, type_child,
};

/// What a generic parameter is, as far as is known (GEN-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A type.
    Type,
    /// A compile-time value.
    Value,
    /// A type pack.
    Pack,
    /// Not settled.
    Unknown,
}

impl Kind {
    pub(super) fn of(kind: &GenericKind) -> Kind {
        match kind {
            GenericKind::Plain => Kind::Unknown,
            GenericKind::Type => Kind::Type,
            GenericKind::Pack { .. } => Kind::Pack,
            GenericKind::Value(_) => Kind::Value,
        }
    }
}

/// The kind of each generic parameter of every struct of the program.
pub(super) type Kinds = HashMap<Id, Vec<Kind>>;

/// Settles the kind of each generic parameter of every struct.
///
/// A parameter written plain, `T`, is a type or a value by how the
/// struct's parents and fields use it (GEN-13). One use is as an
/// argument of another struct, `Grid<N>`, which says what `N` is only
/// when the kind of `Grid`'s parameter is known. So the structs are read
/// again and again, each time with what the time before settled, until a
/// reading settles nothing more. What these readings report is thrown
/// away: `read` reports it once, reading with every kind known.
pub(super) fn struct_kinds(program: &Program) -> Kinds {
    let mut kinds = Kinds::new();
    for (module, declared) in program.modules.iter().enumerate() {
        for (index, declaration) in declared.declarations.iter().enumerate() {
            if declaration.kind == DeclarationKind::Struct {
                let written = generic_forms(&declaration.syntax)
                    .iter()
                    .map(|(_, _, form)| match form {
                        Form::Plain => Kind::Unknown,
                        Form::Pack(..) => Kind::Pack,
                        Form::Value => Kind::Value,
                    })
                    .collect();
                kinds.insert((module, index), written);
            }
        }
    }
    loop {
        // What this reading settles: a parameter, by its struct and its
        // position among the struct's parameters, and its kind.
        let mut settled: Vec<(Id, usize, Kind)> = Vec::new();
        for (module, declared) in program.modules.iter().enumerate() {
            let mut unheard = Diagnostics::new();
            let Some(mut reader) = Reader::new(program, declared, &kinds, &mut unheard) else {
                continue;
            };
            for (index, declaration) in declared.declarations.iter().enumerate() {
                let id = (module, index);
                let open = kinds
                    .get(&id)
                    .is_some_and(|known| known.contains(&Kind::Unknown));
                if !open {
                    continue;
                }
                reader.declaration(declaration);
                for (position, generic) in reader.generics.iter().enumerate() {
                    let now = Kind::of(&generic.kind);
                    // A kind once settled stays, so the readings end.
                    if kinds[&id].get(position) == Some(&Kind::Unknown) && now != Kind::Unknown {
                        settled.push((id, position, now));
                    }
                }
            }
        }
        if settled.is_empty() {
            return kinds;
        }
        for (id, position, kind) in settled {
            if let Some(known) = kinds.get_mut(&id) {
                known[position] = kind;
            }
        }
    }
}

/// What the signature of a declaration says of one of its plain generic
/// parameters (GEN-13).
#[derive(Debug, Clone, Default)]
pub(super) struct Evidence {
    /// Where it is first used as a type.
    as_type: Option<Location>,
    /// Where it is first used as a value.
    as_value: Option<Location>,
    /// True when something this pass does not read may yet say what
    /// it is: a constraint names it, or it is given to a type whose
    /// declaration is not known.
    open: bool,
    /// True when it is given to a generic parameter of a struct whose
    /// own kind nothing settles. That says nothing of what it is.
    dependent: bool,
}

/// FN-18: a parameter declared in the braces before the parentheses.
pub(super) fn is_configuration_parameter(parameter: &SyntaxNode) -> bool {
    parameter
        .parent()
        .is_some_and(|list| list.kind() == SyntaxKind::ConfigParamList)
}

/// What a generic parameter is written as.
pub(super) enum Form {
    /// `T`
    Plain,
    /// `...Ts`, with the fewest and the most it holds.
    Pack(u64, Option<u64>),
    /// `const N: V`
    Value,
}

/// The name and form of a generic parameter.
fn generic_form(parameter: &SyntaxNode) -> Option<(SyntaxToken, Form)> {
    let name = tokens(parameter)
        .take_while(|token| token.kind() != SyntaxKind::Colon)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .last()?;
    let form = if has_token(parameter, SyntaxKind::Ellipsis) {
        let (minimum, maximum) = cardinality(child(parameter, SyntaxKind::Cardinality));
        Form::Pack(minimum, maximum)
    } else if has_token(parameter, SyntaxKind::Colon) {
        Form::Value
    } else {
        Form::Plain
    };
    Some((name, form))
}

/// The fewest and the most a pack holds (GEN-10): `{n}` is exactly n,
/// `{n:*}` n or more, `{n:m}` from n to m, and no cardinality any
/// number.
pub(super) fn cardinality(node: Option<SyntaxNode>) -> (u64, Option<u64>) {
    let Some(node) = node else {
        return (0, None);
    };
    let bounds: Vec<u64> = tokens(&node)
        .filter(|token| token.kind() == SyntaxKind::Int)
        .filter_map(|token| decode_integer(token.text()).ok())
        .collect();
    // `as_slice` lets the `match` look at how many there are and what
    // they are in one step.
    match (bounds.as_slice(), has_token(&node, SyntaxKind::Colon)) {
        ([exact], false) => (*exact, Some(*exact)),
        ([minimum], true) => (*minimum, None),
        ([minimum, maximum], _) => (*minimum, Some(*maximum)),
        _ => (0, None),
    }
}

/// The generic parameters of a declaration, as written.
pub(super) fn generic_forms(declaration: &SyntaxNode) -> Vec<(SyntaxNode, SyntaxToken, Form)> {
    child(declaration, SyntaxKind::GenericParams)
        .into_iter()
        .flat_map(|generics| generics.children())
        .filter(|parameter| parameter.kind() == SyntaxKind::GenericParam)
        .filter_map(|parameter| {
            let (name, form) = generic_form(&parameter)?;
            Some((parameter, name, form))
        })
        .collect()
}

impl Reader<'_> {
    // ---- Declarations ----

    /// Points the reader at a file, outside any declaration with generic
    /// parameters.
    pub(super) fn enter(&mut self, file: FileId) {
        self.file = file;
        self.generics.clear();
        self.evidence.clear();
        self.in_signature = false;
        self.reading_struct = false;
        self.value_function = false;
        self.option_owner = None;
        self.read.clear();
    }

    pub(super) fn declaration(&mut self, declaration: &Declaration) -> Detail {
        self.enter(self.module.files[declaration.file].file);
        let node = &declaration.syntax;
        self.in_signature = true;
        self.reading_struct = declaration.kind == DeclarationKind::Struct;
        // FN-1: a `const fn` of any form, a value candidate of an
        // operator included (GEN-4).
        self.value_function = declaration.kind.role() == Some(Role::Value);
        self.option_owner = match declaration.kind.candidate() {
            Some((Role::Value, operator)) => operator,
            _ => None,
        };
        self.generic_parameters(node);
        let mut detail = match node.kind() {
            SyntaxKind::FnDecl | SyntaxKind::OperatorDecl | SyntaxKind::NativeFnDecl => {
                Detail::Callable(self.signature(node))
            }
            SyntaxKind::StructDecl => Detail::Struct(self.structure(node)),
            SyntaxKind::NativeTypeDecl => Detail::Native(self.native_type(node, &declaration.name)),
            // UNIT-13: "A constant, `const name: T = expression`, names
            // one ordinary value of the type T."
            SyntaxKind::ConstantDecl => Detail::Constant(
                type_child(node).map_or(Type::Unknown, |written| self.outermost(&written)),
            ),
            _ => Detail::Plain,
        };
        self.in_signature = false;
        // GEN-13: what the signature and the constraints say each plain
        // generic parameter is.
        self.constrained(node);
        self.settle_kinds(declaration);
        match &mut detail {
            Detail::Callable(signature) => {
                signature.generics = self.generics.clone();
                self.result_only_parameters(node, signature);
            }
            Detail::Struct(read) => read.generics = self.generics.clone(),
            _ => {}
        }
        // Everything else the declaration writes: its constraints and
        // its body.
        self.scan(node);
        detail
    }

    /// Reads the generic parameters of a declaration into
    /// `self.generics`.
    fn generic_parameters(&mut self, declaration: &SyntaxNode) {
        let forms = generic_forms(declaration);
        // GEN-1: "Generic scope covers the whole declaration", so every
        // parameter is known before the type of any is read: in
        // `<const N: T, T>` the type of `N` is the `T` after it.
        self.generics = forms
            .iter()
            .map(|(_, name, form)| GenericParameter {
                name: name.text().to_string(),
                at: self.at(name.span()),
                kind: match form {
                    Form::Plain => GenericKind::Plain,
                    Form::Pack(minimum, maximum) => GenericKind::Pack {
                        minimum: *minimum,
                        maximum: *maximum,
                    },
                    Form::Value => GenericKind::Value(Type::Unknown),
                },
                inferred_list_size: false,
                result_only: false,
            })
            .collect();
        self.evidence = vec![Evidence::default(); self.generics.len()];

        // The type of each value parameter. A size inside one may name
        // another, declared before it or after:
        // `<const N: list<i64, D>, const D: duration>`. So each is read
        // twice: first to learn what it is, with what is reported set
        // aside and nothing recorded; then, with the type of every one
        // known, for the record.
        //
        // `mem::replace` puts an empty sink where the real one was and
        // gives the real one back, to be restored after.
        let reported = std::mem::replace(self.diagnostics, Diagnostics::new());
        let recorded = self.written.len();
        self.value_parameter_types(&forms);
        *self.diagnostics = reported;
        self.written.truncate(recorded);
        self.read.clear();
        self.value_parameter_types(&forms);
    }

    /// Reads the type of each `const N: V` among `forms` into
    /// `self.generics`.
    fn value_parameter_types(&mut self, forms: &[(SyntaxNode, SyntaxToken, Form)]) {
        for (index, (parameter, _, form)) in forms.iter().enumerate() {
            if let (Form::Value, Some(written)) = (form, type_child(parameter)) {
                self.generics[index].kind = GenericKind::Value(self.outermost(&written));
            }
        }
    }

    /// The parameters and result of a function, an operator or a native
    /// function.
    fn signature(&mut self, node: &SyntaxNode) -> Signature {
        let mut parameters = Vec::new();
        let mut dependent = Vec::new();
        for parameter in crate::read::parameters(node) {
            let Some(name) = names_before_colon(&parameter).pop() else {
                continue;
            };
            let named_pack = child(&parameter, SyntaxKind::PackType);
            let at = self.at(name.span());
            if let Some(clause) = named_pack
                .as_ref()
                .and_then(|pack| child(pack, SyntaxKind::PackFor))
            {
                dependent.push((at, clause));
            }
            let pack = if named_pack.is_some() {
                Some(Pack::Named)
            } else if has_token(&parameter, SyntaxKind::Ellipsis) {
                Some(Pack::Positional)
            } else {
                None
            };
            // `...{T}` holds its type inside the braces.
            let ty = match child(&parameter, SyntaxKind::FunctionType) {
                Some(template) => self.function_type(&template),
                None => type_child(named_pack.as_ref().unwrap_or(&parameter))
                    .map_or(Type::Unknown, |written| self.outermost(&written)),
            };
            let constant = is_const(&parameter);
            let has_default = has_token(&parameter, SyntaxKind::Eq);
            if let Some(default) = default_of(&parameter) {
                self.note_values_in(&default);
            }

            parameters.push(Parameter {
                name: name.text().to_string(),
                at,
                configuration: is_configuration_parameter(&parameter),
                constant,
                var: has_token(&parameter, SyntaxKind::KwVar),
                // FN-16: "`ref low: f64` ... The mark is a mode".
                reference: has_token(&parameter, SyntaxKind::KwRef),
                ty,
                pack,
                pack_bounds: cardinality(child(&parameter, SyntaxKind::Cardinality)),
                has_default,
            });
        }
        for (at, clause) in dependent {
            if let Some(pack) = self.dependent_pack(&clause, &parameters) {
                self.dependent_packs.insert(at, pack);
            }
        }
        let written = child(node, SyntaxKind::ReturnType);
        let result = written
            .as_ref()
            .and_then(type_child)
            .map(|written| self.outermost(&written));
        Signature {
            generics: self.generics.clone(),
            parameters,
            result,
            // FN-16: "and its result, `-> ref f64`".
            reference: written.is_some_and(|written| has_token(&written, SyntaxKind::KwRef)),
        }
    }

    /// GEN-1: identify each generic that appears only as a complete,
    /// ordinary result in the signature.
    fn result_only_parameters(&self, declaration: &SyntaxNode, signature: &mut Signature) {
        if !signature
            .generics
            .iter()
            .any(|generic| matches!(generic.kind, GenericKind::Type))
        {
            return;
        }
        let expressions = child(declaration, SyntaxKind::RequiresClause)
            .into_iter()
            .chain(
                crate::read::parameters(declaration)
                    .into_iter()
                    .filter_map(|parameter| default_of(&parameter)),
            )
            .chain(
                generic_forms(declaration)
                    .into_iter()
                    .filter_map(|(parameter, _, _)| default_of(&parameter)),
            );
        let mut mentioned = Vec::new();
        for expression in expressions {
            mentioned.extend(self.named_generics(&expression));
        }
        let results = signature
            .result
            .iter()
            .map(|ty| (ty, signature.reference))
            .chain(
                signature
                    .parameters
                    .iter()
                    .filter_map(|parameter| match &parameter.ty {
                        Type::Function(template) => {
                            Some((template.result.as_ref()?, template.reference))
                        }
                        _ => None,
                    }),
            );
        for generic in &mut signature.generics {
            if !matches!(generic.kind, GenericKind::Type) || mentioned.contains(&generic.at) {
                continue;
            }
            let names = |ty: &Type| {
                names_one_of(ty, &[generic.at])
                    || ty.any_part(&|_| false, &|constant| {
                        constant.value.names_one_of(&[generic.at])
                    })
            };
            let direct = |(ty, reference): (&Type, bool)| {
                !reference && matches!(ty, Type::Parameter { at, .. } if *at == generic.at)
            };
            let mut uses = results.clone().filter(|(ty, _)| names(ty)).peekable();
            let only_results = uses.peek().is_some() && uses.all(direct);
            let used_by_generic = self
                .generics
                .iter()
                .any(|other| matches!(&other.kind, GenericKind::Value(ty) if names(ty)));
            let used_by_input = signature
                .parameters
                .iter()
                .any(|parameter| match &parameter.ty {
                    Type::Function(template) => {
                        template.parameters.iter().any(|input| names(input.ty()))
                    }
                    ty => names(ty),
                });
            generic.result_only = only_results && !used_by_generic && !used_by_input;
        }
        for parameter in &mut signature.parameters {
            if let Type::Function(template) = &mut parameter.ty {
                template.result_admits_none = matches!(template.result.as_ref(),
                    Some(Type::Parameter { at, .. }) if signature.generics.iter().any(|generic| generic.at == *at && generic.result_only));
            }
        }
    }

    /// The input and result types a callable argument must have (FN-17).
    fn function_type(&mut self, template: &SyntaxNode) -> Type {
        let value = tokens(template)
            .next()
            .is_some_and(|token| token.text() == "const");
        let position = if value {
            Position::value("an input of a value function type", "FN-1")
        } else {
            Position::INPUT
        };
        let mut parameters = Vec::new();
        let mut named_pack = None;
        let inputs = child(template, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|input| input.kind() == SyntaxKind::FunctionTypeParam);
        for input in inputs {
            if let Some(pack) = child(&input, SyntaxKind::PackType)
                && child(&pack, SyntaxKind::PackFor).is_none()
            {
                if let Some(reflected) = self.reflected_pack(&input, &pack) {
                    named_pack.get_or_insert(reflected);
                }
                continue;
            }
            let name = names_before_colon(&input)
                .pop()
                .map(|name| name.text().to_string());
            let ty = type_child(&input).map_or(Type::Unknown, |written| {
                let ty = self.type_(&written, position);
                self.record(written.span(), ty)
            });
            parameters.push(FunctionParameter::Fixed { name, ty });
        }
        let annotation = child(template, SyntaxKind::ReturnType);
        let result = annotation.as_ref().and_then(type_child).map(|written| {
            let position = if value {
                Position::value("the result of a value function type", "FN-1")
            } else {
                Position::PLAIN
            };
            let ty = self.type_(&written, position);
            self.record(written.span(), ty)
        });
        let reference = annotation.is_some_and(|written| has_token(&written, SyntaxKind::KwRef));
        Type::Function(Box::new(FunctionType {
            value,
            parameters,
            named_pack,
            result,
            result_admits_none: false,
            reference,
        }))
    }

    /// The parents and members of a struct.
    fn structure(&mut self, node: &SyntaxNode) -> StructType {
        let parents: Vec<Type> = child(node, SyntaxKind::StructBases)
            .into_iter()
            .flat_map(|bases| bases.children())
            .filter(|parent| is_type(parent.kind()))
            .map(|parent| self.outermost(&parent))
            .collect();
        let mut fields = Vec::new();
        for member in node
            .children()
            .filter(|member| member.kind() == SyntaxKind::StructMember)
        {
            // A member's name is its first token. It may be a reserved
            // word (LEX-9).
            let Some(name) = tokens(&member).next() else {
                continue;
            };
            // `name = value` has no `:` and no type: it overrides the
            // default of a field a parent declares (TYPE-4).
            let ty = if has_token(&member, SyntaxKind::Colon) {
                Some(type_child(&member).map_or(Type::Unknown, |written| self.outermost(&written)))
            } else {
                None
            };
            let default = default_of(&member);
            if let Some(default) = &default {
                self.note_values_in(default);
            }
            // The default is the member's child that is no type.
            let written = member.children().find(|part| !is_type(part.kind()));
            let null_default = written.as_ref().is_some_and(is_null);
            let empty_default = written.as_ref().is_some_and(is_empty_sequence);
            fields.push(Field {
                name: name.text().to_string(),
                at: self.at(name.span()),
                ty,
                has_default: has_token(&member, SyntaxKind::Eq),
                null_default,
                empty_default,
                default_at: default.map(|default| self.at(default.span())),
            });
        }
        StructType {
            generics: self.generics.clone(),
            parents,
            fields,
            members: Vec::new(),
        }
    }

    /// GEN-13: a generic parameter named in an expression of the
    /// signature, a size or a default, is used as a value.
    pub(super) fn note_values_in(&mut self, expression: &SyntaxNode) {
        let named: Vec<(Location, Location)> = expression
            .descendants()
            .filter(|used| used.kind() == SyntaxKind::NameExpr)
            .filter_map(|used| {
                let name = name_token(&used)?;
                match self.bindings.get(&(self.file, name.span().start)) {
                    Some(Binding::Local {
                        at,
                        kind: LocalKind::GenericParameter,
                    }) => Some((*at, self.at(name.span()))),
                    _ => None,
                }
            })
            .collect();
        for (declared, used) in named {
            self.note_value(declared, used);
        }
    }

    // ---- The kind of a generic parameter (GEN-13) ----

    /// What the signature has said of the plain generic parameter
    /// declared at `declared`; `None` for one that is not plain, or not
    /// of the declaration being read.
    fn evidence_of(&mut self, declared: Location) -> Option<&mut Evidence> {
        let index = self
            .generics
            .iter()
            .position(|generic| generic.at == declared && generic.kind == GenericKind::Plain)?;
        self.evidence.get_mut(index)
    }

    /// Notes a use of a plain generic parameter as a type, at `used`.
    /// Only a use in the signature says what a parameter is: "Infer its
    /// kind from signature positions and constraints."
    pub(super) fn note_type(&mut self, declared: Location, used: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.as_type.get_or_insert(used);
        }
    }

    /// Notes a use of a plain generic parameter as a value, at `used`.
    pub(super) fn note_value(&mut self, declared: Location, used: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.as_value.get_or_insert(used);
        }
    }

    /// TYPE-10: a list's size is an integer count. Keep this evidence
    /// for a read in the body (GEN-7), independent of the written kind.
    pub(super) fn note_list_size(&mut self, declared: Location) {
        if self.in_signature
            && let Some(generic) = self
                .generics
                .iter_mut()
                .find(|generic| generic.at == declared && generic.kind == GenericKind::Plain)
        {
            generic.inferred_list_size = true;
        }
    }

    /// Notes a use of a plain generic parameter that may yet say what
    /// it is: as an argument of a type whose declaration is not known.
    pub(super) fn note_open(&mut self, declared: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.open = true;
        }
    }

    /// Notes a use of a plain generic parameter as an argument of a
    /// struct whose own parameter nothing settles.
    pub(super) fn note_dependent(&mut self, declared: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.dependent = true;
        }
    }

    /// Notes each generic parameter a constraint of `declaration`
    /// names. Which kind a constraint makes it is for the pass that
    /// reads constraints.
    fn constrained(&mut self, declaration: &SyntaxNode) {
        let Some(clause) = child(declaration, SyntaxKind::RequiresClause) else {
            return;
        };
        // GEN-3 and GEN-13: read category operands before kinds settle.
        // This also follows a named type's argument kinds in Box<T>.
        let before = std::mem::replace(&mut self.in_signature, true);
        for relation in clause.descendants().filter(|node| {
            node.kind() == SyntaxKind::ConstraintRelation
                && node
                    .children()
                    .any(|child| child.kind() == SyntaxKind::ConstraintCategory)
        }) {
            if let Some(left) = relation.children().next()
                && (is_type(left.kind())
                    || left.kind() == SyntaxKind::IndexExpr && self.indexed_pack(&left))
            {
                self.outermost(&left);
            }
        }
        self.in_signature = before;
        let named: Vec<Location> = self.named_generics(&clause).collect();
        for declared in named {
            if let Some(evidence) = self.evidence_of(declared) {
                evidence.open = true;
            }
        }
    }

    /// The generic parameters named by uses inside this syntax.
    fn named_generics<'a>(&'a self, syntax: &'a SyntaxNode) -> impl Iterator<Item = Location> + 'a {
        syntax
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .filter_map(
                move |token| match self.bindings.get(&(self.file, token.span().start)) {
                    Some(Binding::Local {
                        at,
                        kind: LocalKind::GenericParameter,
                    }) => Some(*at),
                    _ => None,
                },
            )
    }

    /// GEN-13: "Infer its kind from signature positions and
    /// constraints ... Reject conflicting kinds or an unresolved kind
    /// lacking a determining declaration/constraint."
    fn settle_kinds(&mut self, declaration: &Declaration) {
        for index in 0..self.generics.len() {
            if self.generics[index].kind != GenericKind::Plain {
                continue;
            }
            let evidence = self.evidence[index].clone();
            let name = self.generics[index].name.clone();
            // A second parameter of one name has been reported as a
            // duplicate (UNIT-12), and no use can name it.
            if self.generics[..index]
                .iter()
                .any(|earlier| earlier.name == name)
            {
                continue;
            }
            match (evidence.as_type, evidence.as_value) {
                (Some(as_type), Some(as_value)) => {
                    // Reported at the later use; the other is shown.
                    let (at, other, other_is) = if as_type.span.start > as_value.span.start {
                        (as_type, as_value, "used as a value here")
                    } else {
                        (as_value, as_type, "used as a type here")
                    };
                    self.diagnostics.emit(
                        error(
                            Category::Type,
                            codes::GENERIC_KIND_CONFLICT,
                            at,
                            format!("`{name}` is used as a type and as a value"),
                            "GEN-13",
                        )
                        .with_related(other, other_is),
                    );
                }
                (Some(_), None) => self.generics[index].kind = GenericKind::Type,
                (None, Some(_)) => self.generics[index].kind = GenericKind::Value(Type::Unknown),
                // Something may yet say what it is.
                (None, None) if evidence.open => {}
                // A callable's parameter that its signature does not
                // settle is not rejected where it is declared: a call
                // that supplies no argument for it is (the owner's
                // decision of 2026-11-15 on hgl_spec issue 21, reading
                // 33). A struct is always given its arguments, so
                // nothing later says what its parameter is.
                (None, None) if !self.reading_struct => {}
                (None, None) => {
                    self.diagnostics.emit(
                        error(
                            Category::Type,
                            codes::GENERIC_KIND_UNRESOLVED,
                            self.generics[index].at,
                            format!(
                                "nothing in the signature of `{}` says whether `{name}` is a \
                                 type or a value",
                                declaration.name
                            ),
                            "GEN-13",
                        )
                        .with_note(
                            "use it as a type or as a size in the signature, or declare a \
                             value `const N: i64`",
                        ),
                    );
                }
            }
        }
    }
}
