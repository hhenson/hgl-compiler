//! The typed form of a body: what a function does, said without its
//! syntax.
//!
//! The syntax tree says how a body is written. The stages after the
//! analyser need what it means: which local each name is, the type of
//! each expression, where a number is widened. This module is that
//! description. It names no target: a transpiler reads it and writes
//! its own target's code.
//!
//! The analyser fills it in pass by pass. What a pass does not read yet
//! is [`ExprKind::Unread`] or [`Stmt::Unread`]: it is there, its type is
//! not known, and nothing is reported of it (ERR-9).

use hgl_diagnostics::Location;

use crate::program::LocalKind;
use crate::types::Type;

/// The body of one function.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// Its parameters and locals, parameters first, each in the order
    /// it is declared. A [`LocalId`] is a position in this list.
    pub locals: Vec<Local>,
    /// Its statements, in order. An arrow body, `=> value`, is one
    /// [`Stmt::Return`].
    pub statements: Vec<Stmt>,
}

/// The position of a local in [`Body::locals`].
pub type LocalId = usize;

/// A parameter or a local of a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Its name.
    pub name: String,
    /// Where the name is declared.
    pub at: Location,
    /// What declares it.
    pub kind: LocalKind,
    /// Its type; [`Type::Unknown`] when nothing says.
    pub ty: Type,
}

/// One statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let name = value`, `var name: T = value`, `var name: T`: the
    /// local begins, with its first value when one is written.
    Declare {
        /// The local.
        local: LocalId,
        /// Its first value.
        value: Option<Expr>,
    },
    /// `name = value` or `name += value`.
    Assign {
        /// The local assigned.
        local: LocalId,
        /// The operator of a compound assignment: `+` for `+=`. The
        /// local's value is its left operand.
        operator: Option<BinaryOperator>,
        /// The value, or the right operand.
        value: Expr,
        /// Where the statement is written.
        at: Location,
    },
    /// `return`, `return value`, or the value of an arrow body.
    Return {
        /// The value returned.
        value: Option<Expr>,
        /// Where the statement is written.
        at: Location,
    },
    /// An expression evaluated for what it does.
    Expr(Expr),
    /// A statement no pass reads yet.
    Unread {
        /// Where it is written.
        at: Location,
    },
}

/// One expression, with its type.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What it is.
    pub kind: ExprKind,
    /// Its type; [`Type::Unknown`] when it is not known.
    pub ty: Type,
    /// Where it is written.
    pub at: Location,
}

/// What an expression is.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A literal.
    Literal(Value),
    /// A parameter or a local.
    Local(LocalId),
    /// A generic value parameter of the declaration: the `N` of
    /// `const fn width<const N: i64>() -> i64 => N`.
    GenericValue {
        /// Its name.
        name: String,
        /// Where it is declared.
        at: Location,
    },
    /// `-value` or `!value`.
    Unary {
        /// The operator.
        operator: UnaryOperator,
        /// The operand.
        operand: Box<Expr>,
    },
    /// `left + right`, and each other form with two operands.
    Binary {
        /// The operator.
        operator: BinaryOperator,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// An `i64` taken as the `f64` of the same number, where an `f64`
    /// is wanted (EXPR-5, NUM-1). The source writes nothing for it.
    Widen(Box<Expr>),
    /// A call: `scale(value, factor: 2.0)`.
    Call {
        /// What is called.
        callee: Callee,
        /// The arguments, as they are written.
        arguments: Vec<Argument>,
    },
    /// A struct made from its fields: `Quote(bid: 1.0, ask: 2.0)`. The
    /// expression's type is the struct, with its generic arguments.
    Construct {
        /// The position of the struct's module in `Program::modules`.
        module: usize,
        /// The position of the struct in that module's `declarations`.
        declaration: usize,
        /// The fields given, as they are written. A field's position
        /// is its place among the struct's `members`.
        arguments: Vec<Argument>,
    },
    /// A member of an enum: `Side.buy`, or `.buy` where a `Side` is
    /// expected (TYPE-7).
    EnumMember {
        /// The position of the enum's module in `Program::modules`.
        module: usize,
        /// The position of the enum in that module's `declarations`.
        declaration: usize,
        /// The member's position among the enum's members.
        member: usize,
    },
    /// A field of a struct value: `quote.bid`.
    Field {
        /// The struct value.
        base: Box<Expr>,
        /// The field's position among the struct's `members`.
        member: usize,
    },
    /// `base[index]`.
    Index {
        /// What is indexed.
        base: Box<Expr>,
        /// The index or key.
        index: Box<Expr>,
    },
    /// `(a, b)`.
    Tuple(Vec<Expr>),
    /// `[a, b]`: a sequence of values. Its type says what it makes.
    Sequence(Vec<Expr>),
    /// An expression no pass reads yet, or one that has been reported.
    Unread,
}

/// What a call calls.
#[derive(Debug, Clone, PartialEq)]
pub enum Callee {
    /// A value function.
    Function {
        /// The position of its module in `Program::modules`.
        module: usize,
        /// Its position in that module's `declarations`.
        declaration: usize,
    },
    /// The value candidate selected for a call of an operator (GEN-5).
    Candidate {
        /// The operator called: the position of its module in
        /// `Program::modules`, and its position in that module's
        /// `declarations`.
        operator: (usize, usize),
        /// The position of the candidate's module in
        /// `Program::modules`.
        module: usize,
        /// The candidate's position in that module's `declarations`.
        declaration: usize,
    },
    /// What no pass reads yet: a name left for the prelude, an
    /// operator whose candidates are generic, a function that is
    /// itself an expression.
    Unread,
}

/// One argument of a call or a constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    /// The parameter or field it is given to: its index among the
    /// function's parameters, or among the struct's fields, a parent's
    /// first. `None` when it is given to a pack, or to nothing.
    pub parameter: Option<usize>,
    /// Its value.
    pub value: Expr,
}

/// The value of a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// `true` or `false`.
    Bool(bool),
    /// A string: its contents, the escapes read.
    Str(String),
    /// `null`.
    Null,
    /// A duration, in microseconds.
    Duration(i64),
    /// A date, a time or another temporal literal, as it is written. Its
    /// type says which.
    Temporal(String),
}

/// An operator with one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    /// `-`.
    Negate,
    /// `!`.
    Not,
}

/// An operator with two operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/`: true division.
    Divide,
    /// `//`: floor division.
    FloorDivide,
    /// `%`.
    Modulo,
    /// `==`.
    Equal,
    /// `!=`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,
    /// `<=>`: full comparison.
    Compare,
    /// `&&`.
    And,
    /// `||`.
    Or,
}

impl BinaryOperator {
    /// The operator as HGL writes it.
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::FloorDivide => "//",
            BinaryOperator::Modulo => "%",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::Less => "<",
            BinaryOperator::LessEqual => "<=",
            BinaryOperator::Greater => ">",
            BinaryOperator::GreaterEqual => ">=",
            BinaryOperator::Compare => "<=>",
            BinaryOperator::And => "&&",
            BinaryOperator::Or => "||",
        }
    }
}
