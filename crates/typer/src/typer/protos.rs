// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! What the context expects of a tree: a type, or an application to arguments not typed yet.

use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;
use tasty::TypeId;

/// What the context expects of a tree (dotty's prototypes).
#[derive(Clone, Copy, Debug)]
pub enum Pt {
    Any,
    Type(TypeId),
    /// A function applied to the arguments of a `FunProto`, by its index in `Typer::protos`.
    Fun(usize),
    /// A polymorphic function applied to type arguments, by the index of its `PolyProto` in
    /// `Typer::poly_protos`.
    PolyFun(usize),
    /// The method of `m _`, to be eta-expanded (dotty's `AnyFunctionProto`).
    AnyFunction,
}

/// The arguments of an application, typed once when overloading needs their types, but for
/// functions left for the chosen alternative (dotty's `FunProto`).
pub struct FunProto {
    pub args: Vec<NodeId>,
    pub ctx: Ctx,
    pub typed: Option<Vec<Option<TNodeId>>>,
    /// What the application's result is expected to be (dotty's `resultType`).
    pub pt: Pt,
    /// Whether an expected result type takes part in the applicability of alternatives, not
    /// only ignored (dotty's `deepenProto` of an `IgnoredProto`).
    pub deep: bool,
    /// The elements of an infix operation's tuple argument, until it is untupled.
    pub dual: Option<Vec<NodeId>>,
    /// Whether the arguments are a `using` clause.
    pub using: bool,
}

/// The type arguments of a type application, typed once when overloading needs their types, and
/// what the application is expected to be (dotty's `PolyProto`).
pub struct PolyProto {
    pub args: Vec<NodeId>,
    pub ctx: Ctx,
    pub pt: Pt,
    pub typed: Option<Vec<TNodeId>>,
}
