//! TYPE-18 identities that checked declarations and their concrete instances
//! require in struct fields and inherited types. Lowering supplies its actual
//! instance set before emission; targets only look up the resulting identities.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

use hgl_diagnostics::Location;

use crate::declared::substitute_within;
use crate::hir::{Body, Callee, DefaultExpr, Expr, ExprKind, Value, walk_statements};
use crate::{Argument, ConstantValue, Detail, NamedKind, Program, Signature, Type};

type QueryKey = (Location, Vec<(Location, String)>);
type Values = HashMap<QueryKey, Type>;

/// A program may be lowered with different instance limits. Later lowerings
/// can add checked applications and parallel readers retain stable answers.
#[derive(Clone, Debug, Default)]
pub(crate) struct LayoutQueries(OnceLock<Arc<RwLock<Values>>>);

fn query_key(at: Location, arguments: &[(Location, Argument)]) -> Option<QueryKey> {
    let keys = arguments
        .iter()
        .map(|(parameter, argument)| {
            let key = match argument {
                Argument::Type(ty) => format!("T:{}", ty.identity()?),
                Argument::Constant(c) => format!("V:{}", c.value.key()?),
                Argument::NoResult => "no-result".to_owned(),
            };
            Some((*parameter, key))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((at, keys))
}

impl Program {
    fn checked_layout_identity(
        &self,
        at: Location,
        arguments: &[(Location, Argument)],
    ) -> Option<Type> {
        let values = self.layout_queries.0.get()?;
        let key = query_key(at, arguments)?;
        values.read().unwrap().get(&key).cloned()
    }

    /// Apply generic arguments to a layout type using only identities already
    /// checked for this application (GEN-2, TYPE-18).
    pub fn checked_layout_type(&self, ty: &Type, arguments: &[(Location, &Argument)]) -> Type {
        self.resolve_query_types(substitute_within(ty, arguments), &|at, args| {
            self.checked_layout_identity(at, args)
        })
    }

    /// The already-checked Type tag in a struct layout. Open applications
    /// retain the query and its bindings for generic target templates.
    pub fn checked_layout_query(
        &self,
        value: &ConstantValue,
        arguments: &[(Location, &Argument)],
    ) -> ConstantValue {
        self.project_query(value, arguments, &|at, args| {
            self.checked_layout_identity(at, args)
        })
    }

    /// Prepare checked layouts for declarations before lowering follows calls
    /// in them, including calls made by omitted inherited field defaults.
    pub fn prepare_layout_queries(&self) -> LayoutApplications<'_> {
        let mut layout = LayoutApplications {
            program: self,
            concrete: HashSet::new(),
            open: HashMap::new(),
        };
        if !self.has_dependent_type_queries {
            return layout;
        }
        for module in &self.modules {
            for written in &module.written {
                layout.ty(&written.ty);
            }
            for detail in &module.details {
                match detail {
                    Detail::Struct(read) => {
                        for ty in read
                            .parents
                            .iter()
                            .chain(read.members.iter().map(|member| &member.ty))
                        {
                            layout.ty(ty);
                        }
                    }
                    Detail::Callable(signature) => layout.signature(signature),
                    Detail::Constant(ty) => layout.ty(ty),
                    _ => {}
                }
            }
            for body in module
                .bodies
                .iter()
                .chain(&module.test_bodies)
                .flatten()
                .chain(&module.store_body)
            {
                layout.body(body);
            }
            for default in module
                .constants
                .iter()
                .flatten()
                .chain(module.defaults.iter().flatten().flatten())
            {
                layout.default_expr(default);
            }
            if let Some(run) = &module.run_body {
                for local in &run.locals {
                    layout.ty(&local.ty);
                }
                run.call.walk(&mut |expr| layout.expr(expr));
            }
        }
        layout
    }
}

/// The struct applications already checked in this lowering, shared across
/// declarations and each subsequently selected callable instance.
pub struct LayoutApplications<'p> {
    program: &'p Program,
    concrete: HashSet<String>,
    open: HashMap<(usize, usize), Vec<Vec<Argument>>>,
}

impl LayoutApplications<'_> {
    /// An omitted field or configuration value can introduce new concrete
    /// layouts when its default is specialized for this call.
    pub fn default_expr(&mut self, default: &DefaultExpr) {
        if self.program.has_dependent_type_queries {
            for local in &default.locals {
                self.ty(&local.ty);
            }
            default.value.walk(&mut |expr| self.expr(expr));
        }
    }

    /// Check the field and parent types selected by one instantiated body.
    pub fn instance(&mut self, signature: &Signature, body: &Body) {
        if self.program.has_dependent_type_queries {
            self.signature(signature);
            self.body(body);
        }
    }

    fn signature(&mut self, signature: &Signature) {
        for ty in signature
            .parameters
            .iter()
            .map(|p| &p.ty)
            .chain(&signature.result)
        {
            self.ty(ty);
        }
    }

    fn body(&mut self, body: &Body) {
        for local in &body.locals {
            self.ty(&local.ty);
        }
        walk_statements(&body.statements, &mut |expr| self.expr(expr));
    }

    fn expr(&mut self, expr: &Expr) {
        self.ty(&expr.ty);
        let callee = match &expr.kind {
            ExprKind::Call { callee, .. } => callee,
            ExprKind::StaticCallable(callee) => callee.as_ref(),
            ExprKind::Literal(Value::Type(ty)) => return self.ty(ty),
            _ => return,
        };
        // A callable owns its anonymous body outside the ordinary expression
        // walk. Its layout can first become concrete in the enclosing instance.
        if let Callee::Anonymous(function) = callee {
            for ty in function.template.types() {
                self.ty(ty);
            }
            self.body(&function.body);
        }
        for argument in callee.generics() {
            match argument {
                Argument::Type(ty) => self.ty(ty),
                Argument::Constant(c) => {
                    if let ConstantValue::Type(ty) = &c.value {
                        self.ty(ty);
                    }
                }
                Argument::NoResult => {}
            }
        }
        for pack in callee.type_packs() {
            for ty in &pack.types {
                self.ty(ty);
            }
        }
    }

    fn first_application(&mut self, id: (usize, usize), ty: &Type, arguments: &[Argument]) -> bool {
        if let Some(key) = ty.identity() {
            return self.concrete.insert(key);
        }
        let reached = self.open.entry(id).or_default();
        if reached.iter().any(|known| known == arguments) {
            return false;
        }
        reached.push(arguments.to_vec());
        true
    }

    fn ty(&mut self, root: &Type) {
        let mut pending = vec![(root.clone(), Vec::new())];
        while let Some((ty, ancestors)) = pending.pop() {
            let applications = RefCell::new(Vec::new());
            ty.any_part(
                &|part| {
                    if matches!(
                        part,
                        Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                    ) {
                        applications.borrow_mut().push(part.clone());
                    }
                    false
                },
                &|_| false,
            );
            for application in applications.into_inner() {
                let Type::Named {
                    module,
                    declaration,
                    ref arguments,
                    ..
                } = application
                else {
                    unreachable!()
                };
                let id = (module, declaration);
                let recursive = ancestors.contains(&id);
                if !recursive && !self.first_application(id, &application, arguments) {
                    continue;
                }
                let Detail::Struct(read) = &self.program.modules[module].details[declaration]
                else {
                    continue;
                };
                let mut path = ancestors.clone();
                path.push(id);
                let map: Vec<_> = read.generics.iter().map(|p| p.at).zip(arguments).collect();
                for field in read
                    .members
                    .iter()
                    .map(|member| &member.ty)
                    .chain(&read.parents)
                {
                    let applied = substitute_within(field, &map);
                    let applied = self.program.resolve_query_types(applied, &|at, args| {
                        let key = query_key(at, args)?;
                        let table = self.program.layout_queries.0.get_or_init(Default::default);
                        if let Some(known) = table.read().unwrap().get(&key) {
                            return Some(known.clone());
                        }
                        let result = self.program.instance_query_type(at, args)?;
                        table.write().unwrap().insert(key, result.clone());
                        Some(result)
                    });
                    // A recursive layout may name itself with growing arguments.
                    // Resolve its immediate fields but bound further expansion.
                    if !recursive {
                        pending.push((applied, path.clone()));
                    }
                }
            }
        }
    }
}
