//! The candidate of an operator that a call selects: which candidates
//! admit the call, how specific each is, and the call of the one selected
//! (GEN-2, GEN-5, GEN-6). Part of the typing pass ([`super`]): a symbol
//! and a call of an operator by its name both select here.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Typer;
use super::constants::{constant_text, default_at, generic_default};
use super::control::value_of_branch;
use super::expressions::takes_its_type;
use super::fit::{accepts_float, arithmetic, is_integer_literal, negated, not_null, whole};
use super::inference::{bare_parameter, infer, placed};
use super::requirements::{Condition, holds};
use crate::constant::Fault;
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Form};
use crate::program::{Declaration, OperatorRef, Role};
use crate::syntax::{child, tokens, ungrouped};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, GenericKind, GenericParameter, Signature, Type,
};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// The candidates of `operator` that admit a call, the most
    /// specific of them (GEN-5), and whether one could not be told to
    /// admit it or not.
    ///
    /// EXPR-5: an integer literal is an `f64` where one is expected; it
    /// selects as the `i64` it is where a candidate takes one. So the
    /// candidates are tried first with no literal widened.
    pub(super) fn admitted(
        &mut self,
        operator: OperatorRef,
        candidates: &[(usize, usize, &'a Signature)],
        given: &[Given],
        wanted: &Wanted,
    ) -> (Vec<Chosen<'a>>, bool) {
        let program = self.program;
        let operator_declaration =
            &program.modules[operator.module].declarations[operator.declaration];
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return (Vec::new(), false);
        };
        let mut selected: Vec<Chosen> = Vec::new();
        let mut untold = false;
        // True when the candidates were tried with integer literals
        // widened.
        let mut widened = false;
        for widening in [false, true] {
            // True when a candidate takes the call by its result and
            // parameter types, whatever its condition says.
            let mut fits = false;
            for &(module, declaration, signature) in candidates {
                let candidate = &program.modules[module].declarations[declaration];
                // The candidate as this call takes it: its generic
                // parameters resolved (GEN-2).
                let widens = |decided: &Type, later: &Type| self.widens(decided, later);
                let defaults =
                    |generic: &GenericParameter| self.default_constant(candidate, generic);
                let constant = |node: &SyntaxNode, ty: Option<&Type>| self.read_constant(node, ty);
                let Some((taken, generics, defaulted)) = instance(
                    signature,
                    public,
                    given,
                    wanted,
                    (&widens, &defaults, &constant),
                ) else {
                    continue;
                };
                // GEN-13: a generic value parameter has an ordinary
                // type, and the default it takes is of that type as
                // this call resolves it: the `2` of
                // `<T, const fallback: T = 2>` is no `str`. A default
                // that is not of the type as it is declared is reported
                // there, and a call that takes it is not judged
                // (ERR-9).
                let unfit = defaulted.iter().find_map(|&index| {
                    let default = generic_default(&candidate.syntax, &signature.generics[index])?;
                    let (GenericKind::Value(resolved), GenericKind::Value(declared)) =
                        (&taken.generics[index].kind, &signature.generics[index].kind)
                    else {
                        return None;
                    };
                    // Read as a value of each type, as it would be read
                    // there (EXPR-5): `9007199254740993 + 0` of an
                    // `f64` is that `f64`.
                    let fits = |ty: &Type| {
                        self.typed_in(candidate.at.file, &default, Some(ty))
                            .and_then(|read| crate::constant::fit(read, Some(ty), program))
                            .is_ok()
                    };
                    (whole(resolved) && !fits(resolved)).then(|| !fits(declared))
                });
                match unfit {
                    Some(true) => {
                        untold = true;
                        continue;
                    }
                    Some(false) => continue,
                    None => {}
                }
                let admits = self.admits_call(
                    (candidate, &taken),
                    (operator_declaration, public),
                    (module, &generics),
                    (given, wanted),
                    widening,
                );
                fits |= admits != Admits::Misfit;
                match admits {
                    Admits::Yes => selected.push(Chosen {
                        module,
                        declaration,
                        declared: signature,
                        taken,
                        generics,
                    }),
                    Admits::No | Admits::Misfit => {}
                    Admits::Unknown => untold = true,
                }
            }
            // GEN-5: "Whether a literal is widened is settled by the
            // candidates' result and parameter types, before their
            // conditions: where a candidate that gives the requested
            // output takes every integer literal as an `i64`, none is
            // widened, and the conditions select among the candidates
            // that do. An option so does not change what a literal is."
            if fits {
                widened = widening;
                break;
            }
        }
        // GEN-5: "Select exactly one most-specific admitted candidate.
        // Concrete types outrank variables; structurally constrained
        // variables outrank unconstrained ones."
        most_specific(&mut selected, public.parameters.len());
        if widened && selected.len() > 1 {
            Self::prefer_widened(&mut selected, public, given);
        }
        // GEN-5: in composition "a temporal candidate outranks a value
        // candidate of the same parameter types, as composition chooses
        // the temporal definition of a pair (FN-4)."
        if self.graph && selected.len() > 1 {
            let temporal = |chosen: &Chosen| {
                matches!(
                    program.modules[chosen.module].declarations[chosen.declaration]
                        .kind
                        .candidate(),
                    Some((Role::Temporal, _))
                )
            };
            let types = |chosen: &Chosen| -> Vec<Type> {
                chosen
                    .taken
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone())
                    .collect()
            };
            let outranked: Vec<bool> = selected
                .iter()
                .map(|chosen| {
                    !temporal(chosen)
                        && selected
                            .iter()
                            .any(|other| temporal(other) && types(other) == types(chosen))
                })
                .collect();
            let mut position = 0;
            selected.retain(|_| {
                position += 1;
                !outranked[position - 1]
            });
        }
        (selected, untold)
    }

    /// GEN-5: "Among candidates that tie and differ only in which
    /// integer literals they take as an `f64`, the one that takes every
    /// one of them as an `f64` is selected." So `1 + 2` where an `f64`
    /// is expected is the candidate for two `f64`, and not a tie with
    /// those that widen one literal.
    ///
    /// `selected` is the tied candidates, and is left as it is where
    /// they differ in anything else, or no one of them widens every
    /// literal.
    fn prefer_widened(selected: &mut Vec<Chosen<'a>>, public: &Signature, given: &[Given]) {
        // The type each candidate has for each argument, in the order
        // the arguments are given; `None` for one given to a generic
        // value parameter.
        let places = |chosen: &Chosen| -> Option<Vec<Option<Type>>> {
            let bound = bound(&chosen.taken, public, given)?;
            Some(
                bound
                    .iter()
                    .map(|place| match place {
                        Place::Parameter(index) => Some(chosen.taken.parameters[*index].ty.clone()),
                        Place::Generic(_) => None,
                    })
                    .collect(),
            )
        };
        let Some(taken) = selected.iter().map(places).collect::<Option<Vec<_>>>() else {
            return;
        };
        let literal: Vec<bool> = given
            .iter()
            .map(|given| !given.by_place && is_integer_literal(&given.value))
            .collect();
        // They differ only in what they take the literals as: every
        // other argument is given to one type, and they give one
        // result.
        let first = &selected[0];
        let alike = selected.iter().zip(&taken).all(|(chosen, places)| {
            chosen.taken.result == first.taken.result
                && places
                    .iter()
                    .zip(&taken[0])
                    .zip(&literal)
                    .all(|((place, other), literal)| *literal || place == other)
        });
        if !alike || !literal.contains(&true) {
            return;
        }
        // The candidates that take every literal as an `f64`.
        let widest: Vec<usize> = taken
            .iter()
            .enumerate()
            .filter(|(_, places)| {
                places
                    .iter()
                    .zip(&literal)
                    .filter(|(_, literal)| **literal)
                    .all(|(place, _)| place.as_ref().is_some_and(accepts_float))
            })
            .map(|(position, _)| position)
            .collect();
        if let [only] = widest[..] {
            let chosen = selected.swap_remove(only);
            selected.clear();
            selected.push(chosen);
        }
    }

    /// The call of the candidate `chosen` of `operator` with the
    /// arguments `given`, written as `form` says. Each argument is of
    /// the type the candidate's parameter has: the candidate was
    /// admitted, so each is given to one of its parameters.
    pub(super) fn called(
        &mut self,
        operator: OperatorRef,
        chosen: Chosen,
        given: Vec<Given>,
        (form, at): (Form, Location),
    ) -> (ExprKind, Type) {
        let Chosen {
            module,
            declaration,
            taken,
            generics,
            ..
        } = chosen;
        let signature: &Signature = &taken;
        let program = self.program;
        let bound = match program.modules[operator.module]
            .details
            .get(operator.declaration)
        {
            Some(Detail::Callable(public)) => bound(signature, public, &given).unwrap_or_default(),
            _ => Vec::new(),
        };
        // FN-15: a `var` parameter "is always given: it has no default,
        // and is not left out where its type is written `T?`". The mark
        // takes no part in selecting the candidate, which admits a call
        // that leaves a `T?` out (UNIT-8): it is asked for here, of the
        // candidate the call resolved to.
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if parameter.var && !bound.contains(&Place::Parameter(index)) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_MISSING,
                    at,
                    format!(
                        "`{}` is given no `{}`, a `var` parameter, which is always given a place",
                        program.modules[module].declarations[declaration].name, parameter.name
                    ),
                    "FN-15",
                ));
            }
        }
        // EXPR-14: a symbol's configuration, `{abs: tolerance}`, given
        // to a value candidate in a graph binds where the graph is
        // built. The entries given by name to a symbol are its
        // configuration (EXPR-10); its operands are given in position.
        let lifted = self.graph
            && !matches!(form, Form::Named)
            && matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .candidate(),
                Some((Role::Value, _))
            );
        let arguments = given
            .into_iter()
            .enumerate()
            .filter_map(|(position, given)| {
                let index = match bound.get(position) {
                    Some(Place::Parameter(index)) => *index,
                    // What a generic value parameter is given is a
                    // compile-time constant, kept among the call's
                    // generic arguments and not among its arguments.
                    Some(Place::Generic(_)) => return None,
                    None => {
                        return Some(hir::Argument {
                            parameter: None,
                            value: given.value,
                        });
                    }
                };
                let parameter = &signature.parameters[index];
                let place = format!("the parameter `{}`", parameter.name);
                let named = matches!(given.key, Key::Name(_));
                let value = self.argument_value(given, &parameter.ty, &place);
                // A `const` parameter given a connection is reported as
                // one where the call is read (FN-3).
                if lifted && !parameter.constant && named {
                    self.configuration_input(form, &parameter.name, &value);
                }
                Some(hir::Argument {
                    parameter: Some(index),
                    value,
                })
            })
            .collect();
        let requirements = self.answers(module, declaration, &generics);
        let kind = ExprKind::Call {
            callee: Callee::Candidate {
                operator: (operator.module, operator.declaration),
                module,
                declaration,
                generics,
                requirements,
            },
            arguments,
            form,
        };
        self.finished_call(&kind, false);
        (kind, signature.result.clone().unwrap_or(Type::Unknown))
    }

    /// True when the candidate `signature`, of an operator whose public
    /// signature is `public`, admits a call that gives it `given` and of
    /// which `wanted` is expected (GEN-5). With `widening`, an integer
    /// literal is admitted where an `f64` is taken (EXPR-5).
    fn admits_call(
        &mut self,
        (candidate, signature): (&Declaration, &Signature),
        (operator, public): (&Declaration, &Signature),
        (module, generics): (usize, &[Argument]),
        (given, wanted): (&[Given], &Wanted),
        widening: bool,
    ) -> Admits {
        let result_fits = match (wanted.place, &signature.result) {
            // An output requested in part, a `list<T, 3>` with `T` not
            // said, is matched as far as it is known: `admits` compares
            // what each type says and lets what is not said pass, and
            // keeps a `T` a value of a `T?` (TYPE-14).
            (Some(wanted), Some(result)) => self.admits(wanted, result),
            (Some(_), None) => false,
            (None, _) => true,
        };
        // The operator's own requirement of its result.
        let among = match (&wanted.among, &signature.result) {
            (Some(among), Some(result)) => among.contains(result),
            (Some(_), None) => false,
            (None, _) => true,
        };
        if !result_fits || !among {
            return Admits::Misfit;
        }
        // GEN-6: "A call is matched against each candidate's own
        // signature. One that names an option admits only the
        // candidates that declare it."
        let Some(bound) = bound(signature, public, given) else {
            return Admits::Misfit;
        };
        let own = public.parameters.len();
        // GEN-13: a generic value parameter has an ordinary type, and
        // what the call gives it is of that type.
        for (position, place) in bound.iter().enumerate() {
            let Place::Generic(index) = *place else {
                continue;
            };
            let GenericKind::Value(ty) = &signature.generics[index].kind else {
                continue;
            };
            let given = &given[position];
            let admitted = if given.by_place {
                self.fits_place(&given.node, ty, widening)
            } else {
                self.admits(ty, &given.value.ty)
                    || widening && is_integer_literal(&given.value) && accepts_float(ty)
            };
            if !admitted {
                return Admits::Misfit;
            }
        }
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let argument = bound
                .iter()
                .position(|&place| place == Place::Parameter(index))
                .map(|position| &given[position]);
            let admitted = match argument {
                // What the call leaves out has a default, the
                // candidate's or for one of its own parameters the
                // operator's, or is written `T?` (UNIT-8). GEN-6: "a
                // candidate whose option has none is admitted only when
                // the call gives it."
                None => {
                    parameter.has_default
                        || (index < own && public.parameters[index].has_default)
                        || matches!(parameter.ty, Type::Nullable(_))
                }
                Some(given) if given.by_place => {
                    self.fits_place(&given.node, &parameter.ty, widening)
                }
                Some(given) => {
                    self.admits(&parameter.ty, &given.value.ty)
                        || widening
                            && is_integer_literal(&given.value)
                            && accepts_float(&parameter.ty)
                }
            };
            if !admitted {
                return Admits::Misfit;
            }
        }
        // GEN-6: "A candidate states in `requires` the values of a
        // const option it is for ... A call admits the candidate only
        // where that holds for the option's value at the call, a
        // default included."
        let Some(clause) = child(&candidate.syntax, SyntaxKind::RequiresClause) else {
            return Admits::Yes;
        };
        let Some(condition) = clause.children().next() else {
            return Admits::Yes;
        };
        // GEN-16: "a named operator requirement tests callability with
        // the supplied types." Each one the condition states is tested
        // here, before the rest of the condition is read.
        let callable = self.requirements_hold(
            candidate,
            &signature.generics,
            (module, generics),
            &condition,
        );
        let callable = |node: &SyntaxNode| -> Option<bool> {
            let start = node.span().start;
            callable
                .iter()
                .find(|(at, _)| *at == start)
                .and_then(|(_, verdict)| *verdict)
        };
        // The value an option has at this call: what the call gives
        // it, or its default, the candidate's and then, for one of the
        // operator's own parameters, the operator's. `None` when it is
        // no constant this pass reads. Each is a value of the option's
        // type (EXPR-5): what `instance` resolved for a generic value
        // parameter, and what is given or defaulted for a parameter.
        let this = &*self;
        let option_type = |name: &str| -> Option<Type> {
            if let Some(generic) = signature.generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
            }) {
                return match &signature.generics[generic].kind {
                    GenericKind::Value(ty) => Some(ty.clone()),
                    _ => None,
                };
            }
            signature
                .parameters
                .iter()
                .find(|parameter| parameter.name == name)
                .map(|parameter| parameter.ty.clone())
        };
        let value_of = |name: &str| -> Option<ConstantValue> {
            // An option that selects: a generic value parameter.
            let generic = signature.generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
            });
            if let Some(generic) = generic {
                return match generics.get(generic) {
                    Some(Argument::Constant(constant)) => Some(constant.value.clone()),
                    _ => None,
                };
            }
            let index = signature
                .parameters
                .iter()
                .position(|parameter| parameter.name == name)?;
            let ty = &signature.parameters[index].ty;
            let argument = bound
                .iter()
                .position(|&place| place == Place::Parameter(index));
            if let Some(position) = argument {
                return this.read_constant(&given[position].node, Some(ty)).ok();
            }
            // A default and the file it is written in: the candidate's,
            // or else the operator's.
            let default = default_at(&candidate.syntax, &signature.parameters[index])
                .map(|default| (candidate.at.file, default))
                .or_else(|| {
                    public
                        .parameters
                        .get(index)
                        .and_then(|parameter| default_at(&operator.syntax, parameter))
                        .map(|default| (operator.at.file, default))
                });
            match default {
                Some((file, default)) => this.read_in(file, &default, Some(ty)).ok(),
                // Written `T?` and left out: it is null.
                None => Some(ConstantValue::Null),
            }
        };
        // What a constant the condition compares an option with comes
        // to, read where the candidate is written.
        let constant = |node: &SyntaxNode, name: &str| -> Option<ConstantValue> {
            let ty = option_type(name)?;
            this.read_in(candidate.at.file, node, Some(&ty)).ok()
        };
        // GEN-3: "`requires` may use type/category membership,
        // equalities ...". What a generic parameter of the candidate
        // stands for at this call, and what a type written in the
        // condition means with those in place.
        let own: Vec<Location> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        let type_of = |name: &str| -> Option<Type> {
            let index = signature
                .generics
                .iter()
                .position(|generic| generic.name == name)?;
            match generics.get(index)? {
                Argument::Type(ty) => Some(ty.clone()),
                Argument::Constant(_) => None,
            }
        };
        let declared_in = &self.program.modules[module];
        let file = declared_in.files[candidate.file].file;
        let written = |node: &SyntaxNode| -> Option<Type> {
            let start = node.span().start;
            let read = declared_in
                .written
                .iter()
                .find(|written| written.at.file == file && written.at.span.start == start)?;
            let ty = placed(substitute_within(&read.ty, &map), &own);
            whole(&ty).then_some(ty)
        };
        // A candidate's condition not told is no admission either way
        // (`Admits::Unknown`), whatever it is not told for.
        let unvalued = std::cell::Cell::new(false);
        let condition_of = Condition {
            value_of: &value_of,
            constant: &constant,
            type_of: &type_of,
            written: &written,
            callable: &callable,
            unvalued: &unvalued,
        };
        match holds(&condition, &condition_of) {
            Some(true) => Admits::Yes,
            Some(false) => Admits::No,
            None => Admits::Unknown,
        }
    }

    /// True when the expression at `node`, which is read as the type of
    /// its place, is a value of `ty`. It is read as one, and what that
    /// reading reports is taken back: the caller asks, and reports for
    /// itself. Without `widening`, it is no value of `ty` when an
    /// integer literal among its entries had to be widened to be one:
    /// `[1]` is then a `list<i64>` and no `list<f64>`.
    ///
    /// The expression has been read once already, with nothing expected
    /// of it, so a reading here changes nothing but the diagnostics.
    pub(super) fn fits_place(&mut self, node: &SyntaxNode, ty: &Type, widening: bool) -> bool {
        // Past the function's bound nothing more is tried, and the
        // caller leaves its call unjudged.
        if !self.may_reread() {
            return true;
        }
        let mark = self.mark();
        let value = self.expression(node, Some(ty));
        let value = self.fit(value, ty, "its place");
        let fits = self.take_back(mark, false);
        fits && (widening || !widens_an_entry(&value))
    }
}

/// One argument of an operator's call, before a candidate is selected.
pub(super) struct Given {
    /// How it is written: in a position, or by a name. Which parameter
    /// of a candidate that is, is the candidate's to say (GEN-6).
    pub(super) key: Key,
    /// Where it is written.
    pub(super) node: SyntaxNode,
    /// True when it is read as the type of the parameter it is given
    /// to: see [`typed_by_place`].
    pub(super) by_place: bool,
    /// Its value, read with nothing expected of it.
    pub(super) value: Expr,
    /// True when that reading has been taken back, the argument being
    /// read again as the selected candidate's parameter: `value` is
    /// then not what stands.
    pub(super) retried: bool,
}

impl Given {
    /// An operand of a symbol, which is read before the symbol's
    /// candidates are: in the place `position`, with the value it has.
    pub(super) fn read(position: usize, node: SyntaxNode, value: Expr) -> Given {
        Given {
            key: Key::Position(position),
            node,
            by_place: false,
            value,
            retried: false,
        }
    }
}

/// How a call writes an argument.
pub(super) enum Key {
    /// With no name: the place it stands in, from 0.
    Position(usize),
    /// `name: value`.
    Name(String),
}

/// Whether a candidate admits a call.
#[derive(Clone, Copy, PartialEq)]
enum Admits {
    Yes,
    /// Its condition does not hold for the call (GEN-6).
    No,
    /// It does not take the call's arguments or give the result asked
    /// for: the types do not fit, or the call names an option it does
    /// not declare.
    Misfit,
    /// It cannot be told: a condition of the candidate reads an option
    /// whose value at the call is no constant this pass reads.
    Unknown,
}

/// What a call asks of a candidate's result (GEN-5: "the requested
/// output before inputs").
pub(super) struct Wanted<'w> {
    /// The type the call's place expects, or the one the call's generic
    /// arguments make of the operator's result.
    pub(super) place: Option<&'w Type>,
    /// The types the operator's own requirement admits for its result,
    /// when it states them and they are known: for
    /// `operator convert<T, V, O>(value: V) -> O
    ///     requires O in {T, result<T, error>}`
    /// called as `convert<f64>(count)`, an `f64` and a
    /// `result<f64, error>`.
    pub(super) among: Option<Vec<Type>>,
}

/// What an argument of a call is given to in a candidate.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    /// The parameter at this position of the candidate's.
    Parameter(usize),
    /// The generic value parameter at this position of the candidate's
    /// generic parameters: an option that selects (GEN-6).
    Generic(usize),
}

/// What each argument of a call is given to in a candidate, in the
/// order the arguments are written; `None` when the candidate does not
/// take them.
///
/// GEN-6: "An operator declares the parameters every candidate takes. A
/// candidate's shape honors them ... and it may declare further options
/// of its own". So a candidate's first parameters are the operator's,
/// place for place, and are named as the operator names them (EXPR-2);
/// those after are its options, by its own names. An argument with no
/// name is given to the parameter in its position. GEN-8: "A name binds
/// the callee's generic value parameter of that name, or else its
/// parameter of that name."
fn bound(signature: &Signature, public: &Signature, given: &[Given]) -> Option<Vec<Place>> {
    let own = public.parameters.len();
    if signature.parameters.len() < own {
        return None;
    }
    let mut taken = vec![false; signature.parameters.len()];
    let mut named = vec![false; signature.generics.len()];
    let mut places = Vec::with_capacity(given.len());
    for given in given {
        let place = match &given.key {
            Key::Position(position) => Place::Parameter(*position),
            Key::Name(name) => {
                let generic = signature.generics.iter().position(|generic| {
                    matches!(generic.kind, GenericKind::Value(_)) && generic.name == *name
                });
                match generic {
                    Some(generic) => Place::Generic(generic),
                    None => Place::Parameter(
                        public
                            .parameters
                            .iter()
                            .position(|parameter| parameter.name == *name)
                            .or_else(|| {
                                signature
                                    .parameters
                                    .iter()
                                    .skip(own)
                                    .position(|parameter| parameter.name == *name)
                                    .map(|option| option + own)
                            })?,
                    ),
                }
            }
        };
        // Each is given once.
        let slot = match place {
            Place::Parameter(index) => taken.get_mut(index)?,
            Place::Generic(index) => named.get_mut(index)?,
        };
        if *slot {
            return None;
        }
        *slot = true;
        places.push(place);
    }
    Some(places)
}

/// A candidate a call admits.
pub(super) struct Chosen<'s> {
    /// The position of its module in `Program::modules`.
    pub(super) module: usize,
    /// Its position in that module's `declarations`.
    pub(super) declaration: usize,
    /// Its signature as it is declared, generic parameters and all.
    declared: &'s Signature,
    /// Its signature as this call takes it: what its generic
    /// parameters stand for put in place.
    taken: std::borrow::Cow<'s, Signature>,
    /// What each of its generic parameters stands for at this call.
    pub(super) generics: Vec<Argument>,
}

/// A candidate as a call takes it: itself where it has no generic
/// parameter, and otherwise with each resolved (GEN-2) and put in
/// place. `None` when the call does not say what one of them is, or
/// does not fit the candidate's parameters at all.
///
/// GEN-5: "Match candidates against the requested output before
/// inputs". GEN-12: "A prior binding constrains every later occurrence
/// in declared parameter order." An argument whose type is not known
/// all the way down, `null`, `.member` or `[]`, says nothing.
///
/// `defaults` gives the constant a generic value parameter's default
/// comes to (GEN-2), and `constant` what a constant the call gives an
/// option comes to as a value of the type given (`crate::constant`).
#[allow(clippy::type_complexity)]
fn instance<'s>(
    signature: &'s Signature,
    public: &Signature,
    given: &[Given],
    wanted: &Wanted,
    (widens, defaults, constant): (
        &dyn Fn(&Type, &Type) -> bool,
        &dyn Fn(&GenericParameter) -> Option<Constant>,
        &dyn Fn(&SyntaxNode, Option<&Type>) -> Result<ConstantValue, Fault>,
    ),
) -> Option<(std::borrow::Cow<'s, Signature>, Vec<Argument>, Vec<usize>)> {
    let generics = &signature.generics;
    if generics.is_empty() {
        return Some((
            std::borrow::Cow::Borrowed(signature),
            Vec::new(),
            Vec::new(),
        ));
    }
    let bound = bound(signature, public, given)?;
    let mut found: Vec<Option<Argument>> = vec![None; generics.len()];
    // GEN-2: what the call writes decides first. A generic value
    // parameter it gives by name is that, whatever an argument's type
    // would say of it: `pick(pair, N: 3)` for `value: list<i64, N>` asks
    // for a list of three, and a list of two is then no argument for it.
    for (position, place) in bound.iter().enumerate() {
        if let Place::Generic(index) = *place {
            let given = &given[position];
            // EXPR-5: a value of the option's type, as this candidate
            // declares it: `scale: 1` is the `f64` one.
            let expected = match &generics[index].kind {
                GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
                _ => None,
            };
            let value = constant(&given.node, expected).unwrap_or(ConstantValue::Unknown);
            found[index] = Some(Argument::Constant(Constant {
                text: constant_text(&given.node),
                value,
            }));
        }
    }
    if let (Some(place), Some(result)) = (wanted.place, &signature.result) {
        let place = match result {
            Type::Nullable(_) => place,
            _ => not_null(Some(place)).unwrap_or(place),
        };
        infer(result, place, generics, &mut found);
    }
    // The generic parameters an argument decided: what the call writes,
    // or its place expects, is not widened (GEN-2).
    let mut by_argument = vec![false; generics.len()];
    for (index, parameter) in signature.parameters.iter().enumerate() {
        let argument = bound
            .iter()
            .position(|&place| place == Place::Parameter(index));
        let Some(position) = argument else {
            continue;
        };
        let actual = &given[position].value.ty;
        // GEN-2: "an abstract struct that the type decided inherits
        // from widens the parameter to that struct".
        if let Some(generic) = bare_parameter(&parameter.ty, generics)
            && by_argument[generic]
            && let Some(Argument::Type(decided)) = &found[generic]
            && widens(decided, actual)
        {
            found[generic] = Some(Argument::Type(actual.clone()));
            continue;
        }
        let before: Vec<bool> = found.iter().map(Option::is_some).collect();
        infer(&parameter.ty, actual, generics, &mut found);
        for (generic, was) in before.into_iter().enumerate() {
            by_argument[generic] |= !was && found[generic].is_some();
        }
    }
    // A generic value parameter's own type may name a type parameter:
    // the `T` of `<T, const fallback: T>`. What the call gives it says
    // what that is where the result and the parameters, which are
    // declared to take the call's arguments, have not.
    // GEN-12: in the order they are declared, whatever order the call
    // writes them in.
    for (index, generic) in generics.iter().enumerate() {
        let argument = bound
            .iter()
            .position(|&place| place == Place::Generic(index));
        if let (Some(position), GenericKind::Value(ty)) = (argument, &generic.kind) {
            infer(ty, &given[position].value.ty, generics, &mut found);
        }
    }
    // GEN-6: "An option that selects among candidates is a generic
    // value parameter". One that neither the call nor an argument's
    // type says anything of takes its default (GEN-2). GEN-6: "a
    // candidate whose option has none is admitted only when the call
    // gives it."
    let mut defaulted = Vec::new();
    for (index, generic) in generics.iter().enumerate() {
        if found[index].is_some() || !matches!(generic.kind, GenericKind::Value(_)) {
            continue;
        }
        if let Some(default) = defaults(generic) {
            found[index] = Some(Argument::Constant(default));
            defaulted.push(index);
        }
    }
    let resolved: Vec<Argument> = found.into_iter().collect::<Option<_>>()?;
    let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
    let mut taken = signature.clone();
    for parameter in &mut taken.parameters {
        parameter.ty = placed(substitute_within(&parameter.ty, &map), &own);
    }
    // A generic value parameter's type may name another generic
    // parameter: the `T` of `<T, const fallback: T>`.
    for generic in &mut taken.generics {
        if let GenericKind::Value(ty) = &mut generic.kind {
            *ty = placed(substitute_within(ty, &map), &own);
        }
    }
    taken.result = taken
        .result
        .as_ref()
        .map(|result| placed(substitute_within(result, &map), &own));
    Some((std::borrow::Cow::Owned(taken), resolved, defaulted))
}

/// How specific one type of a candidate's signature is beside another's
/// in the same place.
#[derive(Clone, Copy, PartialEq)]
enum Specific {
    /// As specific: the same shape, variables where the other has them.
    Same,
    /// More: concrete, or structured, where the other has a variable.
    More,
    /// Less.
    Less,
    /// Neither: more specific in one part and less in another, or of
    /// another shape altogether.
    Neither,
}

impl Specific {
    /// The two together: what holds of a whole whose parts are these.
    fn and(self, other: Specific) -> Specific {
        match (self, other) {
            (Specific::Same, other) | (other, Specific::Same) => other,
            (Specific::More, Specific::More) => Specific::More,
            (Specific::Less, Specific::Less) => Specific::Less,
            _ => Specific::Neither,
        }
    }
}

/// How specific `left` is beside `right`, each a type of a candidate's
/// signature as it is declared, so that a `Type::Parameter` in it is
/// one of that candidate's generic parameters.
///
/// GEN-5: "Concrete types outrank variables; structurally constrained
/// variables outrank unconstrained ones." GEN-12: "Compare structural
/// specificity component by component". So `tuple<i64, T>` is more
/// specific than `tuple<U, V>`, and `list<T>` than `T`.
fn specificity(left: &Type, right: &Type) -> Specific {
    let all = |pairs: &mut dyn Iterator<Item = Specific>| pairs.fold(Specific::Same, Specific::and);
    match (left, right) {
        (Type::Parameter { .. }, Type::Parameter { .. }) => Specific::Same,
        (Type::Parameter { .. }, _) => Specific::Less,
        (_, Type::Parameter { .. }) => Specific::More,
        // TYPE-14: a `T` is given where a `T?` is expected, so one
        // candidate may take `i64` where another takes `T?`. They are
        // compared as the types under the `?`.
        (Type::Nullable(left), right) if !matches!(right, Type::Nullable(_)) => {
            specificity(left, right)
        }
        (left, Type::Nullable(right)) if !matches!(left, Type::Nullable(_)) => {
            specificity(left, right)
        }
        (Type::Nullable(left), Type::Nullable(right))
        | (Type::Set(left), Type::Set(right))
        | (Type::Ref(left), Type::Ref(right))
        | (Type::Endpoint(left), Type::Endpoint(right))
        | (Type::Atomic(left), Type::Atomic(right))
        | (Type::Delta(left), Type::Delta(right))
        | (Type::List { element: left, .. }, Type::List { element: right, .. })
        | (Type::RollingWindow { value: left, .. }, Type::RollingWindow { value: right, .. }) => {
            specificity(left, right)
        }
        (Type::Tuple(left), Type::Tuple(right)) if left.len() == right.len() => all(&mut left
            .iter()
            .zip(right)
            .map(|(left, right)| specificity(left, right))),
        (
            Type::Dict { key, value },
            Type::Dict {
                key: other_key,
                value: other_value,
            },
        ) => specificity(key, other_key).and(specificity(value, other_value)),
        (
            Type::Result { value, error },
            Type::Result {
                value: other_value,
                error: other_error,
            },
        ) => specificity(value, other_value).and(specificity(error, other_error)),
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: other_module,
                declaration: other_declaration,
                arguments: other_arguments,
                ..
            },
        ) if (module, declaration) == (other_module, other_declaration)
            && arguments.len() == other_arguments.len() =>
        {
            all(&mut arguments
                .iter()
                .zip(other_arguments)
                .map(|pair| match pair {
                    (Argument::Type(left), Argument::Type(right)) => specificity(left, right),
                    // A constant is as specific as a constant.
                    _ => Specific::Same,
                }))
        }
        // Two types with no variable in them: one type, where both
        // candidates admit the call.
        (left, right) if left == right => Specific::Same,
        // Of another shape. GEN-5: "Concrete types outrank variables".
        (left, right) => match (names_parameter(left), names_parameter(right)) {
            (false, true) => Specific::More,
            (true, false) => Specific::Less,
            _ => Specific::Neither,
        },
    }
}

/// Keeps of the candidates a call admits the most specific: those no
/// other outranks. One is left where the call selects; more are a tie.
///
/// GEN-5: "Select exactly one most-specific admitted candidate." The
/// operator's own parameters, `own` of them, are compared place for
/// place ([`specificity`]). A candidate outranks another when it is as
/// specific in every one and more specific in one.
fn most_specific(selected: &mut Vec<Chosen>, own: usize) {
    if selected.len() < 2 {
        return;
    }
    let outranks = |better: &Chosen, worse: &Chosen| {
        let whole = better
            .declared
            .parameters
            .iter()
            .zip(&worse.declared.parameters)
            .take(own)
            .map(|(better, worse)| specificity(&better.ty, &worse.ty))
            .fold(Specific::Same, Specific::and);
        whole == Specific::More
    };
    let outranked: Vec<bool> = selected
        .iter()
        .map(|chosen| selected.iter().any(|other| outranks(other, chosen)))
        .collect();
    let mut position = 0;
    selected.retain(|_| {
        position += 1;
        !outranked[position - 1]
    });
}

/// True when an integer literal was widened among the entries of a
/// sequence or a tuple written out, at any depth. A literal widened
/// inside a call by name among the entries is that call's, and is not
/// counted; one widened under a symbol that is core's own arithmetic
/// is.
fn widens_an_entry(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Widen(_) => true,
        ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => {
            entries.iter().any(widens_an_entry)
        }
        // Either side of a pair: `["a": 1]` where an `f64` is asked for.
        ExprKind::Pairs(entries) => entries
            .iter()
            .any(|(key, value)| widens_an_entry(key) || widens_an_entry(value)),
        // The value of a block, and of each branch of an `if`.
        ExprKind::Block { .. } | ExprKind::If { .. } => value
            .kind
            .branch_values()
            .is_some_and(|values| values.into_iter().any(widens_an_entry)),
        // A symbol among the entries is read with the entry's type as
        // its place, so that core's own arithmetic over literals may
        // widen them: the `1` of `[-1]`, the `1` and `2` of `[1 + 2]`,
        // where an `f64` is asked for. That is the entry's widening.
        _ => {
            negated(value).is_some_and(widens_an_entry)
                || arithmetic(value)
                    .is_some_and(|(_, left, right)| widens_an_entry(left) || widens_an_entry(right))
        }
    }
}

/// True when an argument is read as the type of the parameter it is
/// given to: `null` and `.member`, which have no type of their own
/// (TYPE-14, TYPE-7), and a sequence or a tuple written out, whose
/// entries are read as the parameter's (EXPR-5, EXPR-9), so that `[1]`
/// is a `list<f64>` or a `set<i64>` where a candidate takes one. A block
/// is, when its value is; an `if`, when a branch is (CTRL-10).
pub(super) fn typed_by_place(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::SequenceExpr | SyntaxKind::TupleExpr => true,
        // An integer literal that is a block's value is an `f64` where
        // one is expected (EXPR-5), as the block's place says.
        SyntaxKind::Block => value_of_branch(node)
            .is_some_and(|value| typed_by_place(&value) || is_integer_written(&value)),
        // The condition is first; the branches follow it.
        SyntaxKind::IfExpr => node
            .children()
            .skip(1)
            .any(|branch| typed_by_place(&branch)),
        _ => takes_its_type(node),
    }
}

/// True when an expression is an integer literal as it is written,
/// under a minus or in parentheses or not.
fn is_integer_written(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::Int),
        SyntaxKind::UnaryExpr => node
            .children()
            .next()
            .is_some_and(|inner| is_integer_written(&inner)),
        _ => false,
    }
}
