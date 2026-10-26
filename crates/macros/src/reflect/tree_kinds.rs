// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The kinds of trees the type tests of `quotes.reflect` tell apart.

use crate::host::Host;
use tasty::Type;
use typer::tpd::{TNodeId, Tree as T};

/// The kinds of trees, as the overlay's `TreeKinds` numbers them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Other = 0,
    Ident = 1,
    Select = 2,
    Literal = 3,
    Typed = 4,
    Apply = 5,
    TypeApply = 6,
    Block = 7,
    Inlined = 8,
    Repeated = 9,
    Inferred = 11,
    ValDef = 12,
    NamedArg = 13,
    Term = 14,
    DefDef = 15,
    TypeDef = 16,
    Import = 17,
    This = 18,
    ClassDef = 19,
    Closure = 20,
    Super = 21,
    New = 22,
    Assign = 23,
    If = 24,
    Match = 25,
    SummonFrom = 26,
    Return = 27,
    While = 28,
    Try = 29,
    CaseDef = 30,
    TypeCaseDef = 31,
    Bind = 32,
    TypeBind = 33,
    Unapply = 34,
    Alternatives = 35,
    TypedOrTest = 36,
    TypeIdent = 37,
    TypeSelect = 38,
    TypeProjection = 39,
    Singleton = 40,
    Refined = 41,
    Applied = 42,
    Annotated = 43,
    ByName = 44,
    LambdaTypeTree = 45,
    TypeBoundsTree = 46,
    WildcardTypeTree = 47,
    Export = 48,
    PackageClause = 49,
    TypeBlock = 50,
}

impl Kind {
    /// Whether trees of the kind are dotty's terms (`Tree.isTerm` but for patterns).
    pub fn is_term(self) -> bool {
        use Kind::*;
        matches!(
            self,
            Ident
                | Select
                | Literal
                | Typed
                | Apply
                | TypeApply
                | Block
                | Inlined
                | Repeated
                | NamedArg
                | Term
                | This
                | Closure
                | Super
                | New
                | Assign
                | If
                | Match
                | SummonFrom
                | Return
                | While
                | Try
        )
    }

    /// Whether trees of the kind are type trees (`Tree.isType`), bounds included.
    pub fn is_type(self) -> bool {
        use Kind::*;
        matches!(
            self,
            Inferred
                | TypeIdent
                | TypeSelect
                | TypeProjection
                | Singleton
                | Refined
                | Applied
                | Annotated
                | ByName
                | LambdaTypeTree
                | TypeBoundsTree
                | WildcardTypeTree
                | TypeBind
                | TypeBlock
        )
    }
}

impl Host {
    /// Whether the type of `t` is a bounds or an alias type.
    fn has_bounds(&self, t: TNodeId) -> bool {
        let tpe = self.tpd.get(t).tpe;
        tpe.is_some_and(|tpe| matches!(self.u.types().get(tpe), Type::Bounds(..) | Type::Alias(_)))
    }

    /// The kind of `t`; every `TypeTree` is dotty's `Inferred` but for one of bounds.
    pub fn kind(&self, t: TNodeId) -> Kind {
        let is_type = |name: &syntax::ast::TName| {
            name.is_type || self.tpd.sym(t).is_some_and(|s| self.u.symbol(s).kind.is_type())
        };
        match *self.tpd.tree(t) {
            T::Ident { ref name } if is_type(name) && self.names.text(name.name) == "_" => {
                Kind::WildcardTypeTree
            }
            T::Ident { ref name } if is_type(name) => Kind::TypeIdent,
            T::Select { qualifier, ref name } if is_type(name) => match self.kind(qualifier).is_type() {
                true => Kind::TypeProjection,
                false => Kind::TypeSelect,
            },
            T::Ident { .. } => Kind::Ident,
            T::Select { .. } => Kind::Select,
            T::Literal { .. } | T::ClassLiteral { .. } => Kind::Literal,
            T::Typed { expr, .. } if self.kind(expr).is_term() => Kind::Typed,
            T::Typed { .. } => Kind::TypedOrTest,
            T::Apply { .. } => Kind::Apply,
            T::TypeApply { .. } => Kind::TypeApply,
            T::Block { expr, .. } if self.kind(expr).is_type() => Kind::TypeBlock,
            T::Block { .. } => Kind::Block,
            T::Inlined { .. } => Kind::Inlined,
            T::SeqLiteral { .. } => Kind::Repeated,
            T::TypeTree { .. } if self.has_bounds(t) => Kind::TypeBoundsTree,
            T::TypeTree { .. } => Kind::Inferred,
            T::SingletonTypeTree { .. } => Kind::Singleton,
            T::ByNameTypeTree { .. } => Kind::ByName,
            T::AppliedTypeTree { .. } => Kind::Applied,
            T::LambdaTypeTree { .. } => Kind::LambdaTypeTree,
            T::RefinedTypeTree { .. } => Kind::Refined,
            T::TypeBoundsTree { .. } => Kind::TypeBoundsTree,
            T::Annotated { .. } => Kind::Annotated,
            T::ValDef { .. } => Kind::ValDef,
            T::NamedArg { .. } => Kind::NamedArg,
            T::This { .. } => Kind::This,
            T::Closure { .. } => Kind::Closure,
            T::Super { .. } => Kind::Super,
            T::New { .. } => Kind::New,
            T::Assign { .. } => Kind::Assign,
            T::If { .. } => Kind::If,
            T::Return { .. } => Kind::Return,
            T::WhileDo { .. } => Kind::While,
            T::Try { .. } => Kind::Try,
            T::Match { selector: None, .. } => Kind::SummonFrom,
            T::Match { .. } => Kind::Match,
            T::Labeled { .. } => Kind::Term,
            T::CaseDef { body, .. } if self.kind(body).is_type() => Kind::TypeCaseDef,
            T::CaseDef { .. } => Kind::CaseDef,
            T::Bind { name, .. } if name.is_type => Kind::TypeBind,
            T::Bind { .. } => Kind::Bind,
            T::UnApply { .. } => Kind::Unapply,
            T::Alternative { .. } => Kind::Alternatives,
            T::DefDef { .. } => Kind::DefDef,
            T::TypeDef { rhs, .. } if matches!(self.tpd.tree(rhs), T::Template { .. }) => Kind::ClassDef,
            T::TypeDef { .. } => Kind::TypeDef,
            T::Import { export: true, .. } => Kind::Export,
            T::Import { .. } => Kind::Import,
            T::PackageDef { .. } => Kind::PackageClause,
            _ => Kind::Other,
        }
    }
}
