//! Resolved, typed per-method IR produced by `lower` and consumed by `closure` and `walk`.
use crate::program::{ClassId, MethodId};
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Unit,
    Bool,
    Int,
    Long,
    Double,
    Str,
    Obj(ClassId),
    Any,
}

impl Ty {
    pub fn numeric_rank(self) -> Option<u8> {
        match self {
            Ty::Int => Some(0),
            Ty::Long => Some(1),
            Ty::Double => Some(2),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    UShr,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn from_name(name: &str) -> Option<BinOp> {
        use BinOp::*;
        Some(match name {
            "+" => Add,
            "-" => Sub,
            "*" => Mul,
            "/" => Div,
            "%" => Rem,
            "<<" => Shl,
            ">>" => Shr,
            ">>>" => UShr,
            "&" => And,
            "|" => Or,
            "^" => Xor,
            "==" => Eq,
            "!=" => Ne,
            "<" => Lt,
            "<=" => Le,
            ">" => Gt,
            ">=" => Ge,
            _ => return None,
        })
    }

    pub fn is_compare(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }

    pub fn is_shift(self) -> bool {
        matches!(self, BinOp::Shl | BinOp::Shr | BinOp::UShr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
    Abs,
    ToInt,
    ToLong,
    ToDouble,
}

/// Operands of `Bin` and `Un` have already been converted to the operation type.
#[derive(Debug)]
pub enum Expr {
    Const(Value),
    Local(u32),
    SetLocal(u32, Box<Expr>),
    Block(Vec<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    While(Box<Expr>, Box<Expr>),
    AndAlso(Box<Expr>, Box<Expr>),
    OrElse(Box<Expr>, Box<Expr>),
    Bin(BinOp, Ty, Box<Expr>, Box<Expr>),
    Un(UnOp, Ty, Box<Expr>),
    Concat(Vec<Expr>),
    Println(Box<Expr>),
    Call(MethodId, Vec<Expr>),
    New(ClassId, Vec<Expr>),
    Field(Box<Expr>, u32),
    ModuleField(ClassId, u32),
    SetModuleField(ClassId, u32, Box<Expr>),
    Switch(Box<Expr>, Vec<(i32, Expr)>, Option<Box<Expr>>),
}

/// A lowered method body: `slots` locals, of which the first `params` are the arguments.
#[derive(Debug)]
pub struct Lowered {
    pub body: Expr,
    pub params: u32,
    pub slots: u32,
}
