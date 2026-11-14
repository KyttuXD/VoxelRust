//! What the parser makes of a script: items, expressions, patterns.

use crate::script::value::Value;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

/// A name (variable, function, field, type), cheap to copy around.
pub type Name = Rc<str>;

/// A type as written. Types are parsed and mostly ignored: only `f32`/`f64`
/// (to turn whole numbers into floats) and `String`/`Vec` (for `collect`)
/// are ever looked at.
#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    /// The last word of the path (`Vec`, `i32`), `()` for a tuple, `[]` for
    /// an array or slice, `_` for anything else (functions, closures).
    pub name: Name,
    pub args: Vec<Type>,
}

impl Type {
    pub fn is_float(&self) -> bool {
        &*self.name == "f32" || &*self.name == "f64"
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
    Deref,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

/// Which macro a `Format` expression is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FmtMac {
    /// `format!`: gives the string.
    Format,
    /// `print!` / `println!`: log it (no newline needed).
    Print,
    /// `panic!`: stop the script with it as the error.
    Panic,
}

/// A piece of a format string.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Lit(String),
    /// Argument number `arg`, shown as `spec` says.
    Arg { arg: usize, spec: Spec },
}

/// What follows the colon in `{:>8.2}`.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub fill: char,
    /// `<`, `>` or `^`; `None` means the type's own side.
    pub align: Option<char>,
    pub plus: bool,
    pub alt: bool,
    pub zero: bool,
    pub width: Option<usize>,
    pub prec: Option<usize>,
    /// `' '` for Display, `?` for Debug, or `x`, `X`, `b`, `o`, `e`.
    pub ty: char,
}

impl Spec {
    pub const DISPLAY: Spec = Spec { fill: ' ', align: None, plus: false, alt: false, zero: false, width: None, prec: None, ty: ' ' };
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct Closure {
    pub params: Vec<Pat>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Lit(Value),
    /// A name or a path: `x`, `Shape::Circle`, `Self::NEW`, `i32::MAX`.
    Path(Vec<Name>),
    Unary(UnOp, Box<Expr>),
    /// `&e` (`false`) or `&mut e` (`true`).
    Ref(bool, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    AssignOp(BinOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    MethodCall { recv: Box<Expr>, name: Name, turbofish: Option<Type>, args: Vec<Expr> },
    Field(Box<Expr>, Name),
    TupleField(Box<Expr>, usize),
    Index(Box<Expr>, Box<Expr>),
    Range(Option<Box<Expr>>, Option<Box<Expr>>, bool),
    Cast(Box<Expr>, Type),
    Try(Box<Expr>),
    Block(Block),
    /// `let PAT = EXPR` inside an `if` or `while` condition.
    Let(Pat, Box<Expr>),
    If { cond: Box<Expr>, then: Block, els: Option<Box<Expr>> },
    Match { scrut: Box<Expr>, arms: Vec<Arm> },
    While { label: Option<Name>, cond: Box<Expr>, body: Block },
    Loop { label: Option<Name>, body: Block },
    For { label: Option<Name>, pat: Pat, iter: Box<Expr>, body: Block },
    Break(Option<Name>, Option<Box<Expr>>),
    Continue(Option<Name>),
    Return(Option<Box<Expr>>),
    Closure(Rc<Closure>),
    Tuple(Vec<Expr>),
    /// `[a, b]` and `vec![a, b]`.
    Array(Vec<Expr>),
    /// `[x; n]` and `vec![x; n]`.
    Repeat(Box<Expr>, Box<Expr>),
    StructLit { path: Vec<Name>, fields: Vec<(Name, Expr)>, base: Option<Box<Expr>> },
    /// `format!`, `println!`, `panic!`: the pieces and the arguments.
    Format { mac: FmtMac, pieces: Vec<Piece>, args: Vec<Expr> },
    /// `assert!(cond, ...)`; the message is a `Format` expression.
    Assert(Box<Expr>, Option<Box<Expr>>),
    /// `assert_eq!(a, b)` (`true`) and `assert_ne!(a, b)` (`false`).
    AssertEq(bool, Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    /// `matches!(e, PAT if guard)`.
    Matches(Box<Expr>, Pat, Option<Box<Expr>>),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// The last expression, when it has no `;`: the block's value.
    pub tail: Option<Box<Expr>>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let { pat: Pat, ty: Option<Type>, init: Option<Expr>, els: Option<Block>, line: u32 },
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub enum Pat {
    Wild,
    /// A name: binds, unless it is `None` or a constant.
    Bind(Name, Option<Box<Pat>>),
    Lit(Value),
    /// `1..=5`, `'a'..='z'`, `..9`, `3..`.
    Range(Option<Value>, Option<Value>, bool),
    Tuple(Vec<Pat>),
    /// A unit variant or constant by path: `Dir::North`, `Self::X`.
    Path(Vec<Name>),
    /// `Some(x)`, `Shape::Circle(r)`, `Point(x, y)`.
    TupleStruct(Vec<Name>, Vec<Pat>),
    /// `Point { x, y: 0, .. }`, `Shape::Rect { w, h }`.
    Struct(Vec<Name>, Vec<(Name, Pat)>),
    Or(Vec<Pat>),
    /// `[a, b]`, `[first, ..]`, `[.., last]`.
    Slice(Vec<Pat>),
    /// `..` inside a tuple or slice pattern.
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelfKind {
    Value,
    Ref,
    RefMut,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub pat: Pat,
    pub ty: Type,
}

#[derive(Debug)]
pub struct FnDef {
    pub name: Name,
    pub self_kind: Option<SelfKind>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
    /// The type of the `impl` block this is in.
    pub impl_ty: Option<Name>,
    pub line: u32,
}

#[derive(Debug)]
pub struct StructDef {
    pub name: Name,
    /// Field names (`0`, `1`... for a tuple struct).
    pub fields: Vec<Name>,
    pub tuple: bool,
}

#[derive(Debug)]
pub enum VariantKind {
    Unit,
    Tuple(usize),
    Struct(Rc<Vec<Name>>),
}

#[derive(Debug)]
pub struct VariantDef {
    pub name: Name,
    pub kind: VariantKind,
    /// The number after `=`, or the variant's position.
    pub disc: i64,
}

#[derive(Debug)]
pub struct EnumDef {
    pub name: Name,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug)]
pub struct ConstDef {
    pub name: Name,
    pub ty: Option<Type>,
    pub init: Expr,
    /// The type of the `impl` block it is in.
    pub impl_ty: Option<Name>,
}

/// A whole file.
#[derive(Debug, Default)]
pub struct Program {
    pub file: String,
    pub fns: BTreeMap<Name, Rc<FnDef>>,
    /// Methods and associated functions, by (type, name).
    pub methods: BTreeMap<(Name, Name), Rc<FnDef>>,
    pub structs: BTreeMap<Name, StructDef>,
    pub enums: BTreeMap<Name, EnumDef>,
    /// In the order they were written (later ones may use earlier ones).
    pub consts: Vec<ConstDef>,
}

impl Program {
    pub fn has_fn(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }
}
