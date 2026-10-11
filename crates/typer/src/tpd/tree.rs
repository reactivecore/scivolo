//! The typed tree classes, with fields in the order of dotty's case classes. `None` stands for
//! `EmptyTree` (and `EmptyValDef` for `Template.self`).

use std::num::NonZeroU32;
use std::ops::Range;
use syntax::ast::TName;
use syntax::ast::constant::Constant;
use tasty::TypeId;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TNodeId(NonZeroU32);

impl TNodeId {
    pub fn new(index: usize) -> TNodeId {
        TNodeId(NonZeroU32::new(index as u32 + 1).expect("a node index"))
    }

    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// A list of trees, a slice of the `Tpd`'s list pool.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct List {
    start: u32,
    len: u32,
}

impl List {
    pub fn new(start: usize, len: usize) -> List {
        List { start: start as u32, len: len as u32 }
    }

    pub fn range(self) -> Range<usize> {
        self.start as usize..(self.start + self.len) as usize
    }

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// A list of lists of trees (a method's parameter clauses), a slice of the clause pool.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Clauses {
    start: u32,
    len: u32,
}

impl Clauses {
    pub fn new(start: usize, len: usize) -> Clauses {
        Clauses { start: start as u32, len: len as u32 }
    }

    pub fn range(self) -> Range<usize> {
        self.start as usize..(self.start + self.len) as usize
    }

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Tree {
    Ident {
        name: TName,
    },
    Select {
        qualifier: TNodeId,
        name: TName,
    },
    This {
        qual: TNodeId,
    },
    /// `mix` is an untyped identifier, empty for none.
    Super {
        qual: TNodeId,
        mix: TNodeId,
    },
    Apply {
        fun: TNodeId,
        args: List,
    },
    TypeApply {
        fun: TNodeId,
        args: List,
    },
    Literal {
        value: Constant,
    },
    /// dotty's `Literal(Constant(tpe))` with a class tag: `classOf[tpe]`.
    ClassLiteral {
        tpe: TypeId,
    },
    New {
        tpt: TNodeId,
    },
    Typed {
        expr: TNodeId,
        tpt: TNodeId,
    },
    NamedArg {
        name: TName,
        arg: TNodeId,
    },
    /// The function value of method `meth`, closing over `env`; `tpt` for a SAM type.
    Closure {
        env: List,
        meth: TNodeId,
        tpt: Option<TNodeId>,
    },
    /// The arguments of a repeated parameter, of element type `elemtpt`.
    SeqLiteral {
        elems: List,
        elemtpt: TNodeId,
    },
    Assign {
        lhs: TNodeId,
        rhs: TNodeId,
    },
    Block {
        stats: List,
        expr: TNodeId,
    },
    If {
        cond: TNodeId,
        thenp: TNodeId,
        elsep: TNodeId,
    },
    CaseDef {
        pat: TNodeId,
        guard: Option<TNodeId>,
        body: TNodeId,
    },
    /// Block `expr` whose symbol is a label, which `Return`s exit (made by the lowering).
    Labeled {
        expr: TNodeId,
    },
    /// `from` is an identifier of the method or label returned from.
    Return {
        expr: TNodeId,
        from: TNodeId,
    },
    WhileDo {
        cond: TNodeId,
        body: TNodeId,
    },
    Try {
        expr: TNodeId,
        cases: List,
        finalizer: Option<TNodeId>,
    },
    Match {
        selector: TNodeId,
        cases: List,
    },
    /// The pattern `p1 | p2 | ...`.
    Alternative {
        trees: List,
    },
    /// An extractor pattern: `fun` is the `unapply` method, `implicits` its implicit arguments.
    UnApply {
        fun: TNodeId,
        implicits: List,
        patterns: List,
    },
    /// `inferred` for dotty's `InferredTypeTree`.
    TypeTree {
        inferred: bool,
    },
    SingletonTypeTree {
        reference: TNodeId,
    },
    ByNameTypeTree {
        result: TNodeId,
    },
    /// Type `arg` with annotation `annot`, a constructor call.
    Annotated {
        arg: TNodeId,
        annot: TNodeId,
    },
    AppliedTypeTree {
        tpt: TNodeId,
        args: List,
    },
    TypeBoundsTree {
        lo: TNodeId,
        hi: TNodeId,
        alias: Option<TNodeId>,
    },
    /// `[tparams] =>> body`.
    LambdaTypeTree {
        tparams: List,
        body: TNodeId,
    },
    Bind {
        name: TName,
        body: TNodeId,
    },
    ValDef {
        name: TName,
        tpt: TNodeId,
        rhs: Option<TNodeId>,
    },
    DefDef {
        name: TName,
        paramss: Clauses,
        tpt: TNodeId,
        rhs: Option<TNodeId>,
    },
    TypeDef {
        name: TName,
        rhs: TNodeId,
    },
    /// Its symbol is the class's local dummy when the body has expression statements, else the
    /// class; `self_` is `None` for `EmptyValDef`.
    Template {
        constr: TNodeId,
        parents: List,
        self_: Option<TNodeId>,
        body: List,
    },
    Import {
        expr: TNodeId,
        selectors: List,
    },
    /// Its identifiers are untyped, as in dotty.
    ImportSelector {
        imported: TNodeId,
        renamed: Option<TNodeId>,
        bound: Option<TNodeId>,
    },
    PackageDef {
        pid: TNodeId,
        stats: List,
    },
    /// The expansion of inline call `call` after its `bindings`; `call` is `None` for an
    /// argument inlined from the outer scope.
    Inlined {
        call: Option<TNodeId>,
        bindings: List,
        expansion: TNodeId,
    },
}

/// A field of a tree, as `Tree::fields` hands it out: the subtrees as slots that can be
/// replaced, the other fields as values.
pub enum Field<'a> {
    Node(&'a mut TNodeId),
    Opt(&'a mut Option<TNodeId>),
    List(&'a mut List),
    Clauses(&'a mut Clauses),
    Name(TName),
    Constant(&'a Constant),
    /// `ClassLiteral.tpe`.
    Class(TypeId),
}

impl Tree {
    /// Calls `f` on each field of the tree's class in dotty, in order, except `TypeTree.inferred`.
    pub fn fields(&mut self, f: &mut dyn FnMut(Field<'_>)) {
        use Field::{Clauses as Ls, List as L, Name as Nm, Node as N, Opt as O};
        match self {
            Tree::Ident { name } => f(Nm(*name)),
            Tree::Select { qualifier, name } => {
                f(N(qualifier));
                f(Nm(*name));
            }
            Tree::This { qual } => f(N(qual)),
            Tree::Super { qual, mix } => {
                f(N(qual));
                f(N(mix));
            }
            Tree::Apply { fun, args } | Tree::TypeApply { fun, args } => {
                f(N(fun));
                f(L(args));
            }
            Tree::Literal { value } => f(Field::Constant(value)),
            Tree::ClassLiteral { tpe } => f(Field::Class(*tpe)),
            Tree::New { tpt } => f(N(tpt)),
            Tree::NamedArg { name, arg } => {
                f(Nm(*name));
                f(N(arg));
            }
            Tree::Closure { env, meth, tpt } => {
                f(L(env));
                f(N(meth));
                f(O(tpt));
            }
            Tree::SeqLiteral { elems, elemtpt } => {
                f(L(elems));
                f(N(elemtpt));
            }
            Tree::Typed { expr: a, tpt: b }
            | Tree::Assign { lhs: a, rhs: b }
            | Tree::Return { expr: a, from: b }
            | Tree::Annotated { arg: a, annot: b }
            | Tree::WhileDo { cond: a, body: b } => {
                f(N(a));
                f(N(b));
            }
            Tree::Block { stats, expr } => {
                f(L(stats));
                f(N(expr));
            }
            Tree::If { cond, thenp, elsep } => {
                f(N(cond));
                f(N(thenp));
                f(N(elsep));
            }
            Tree::CaseDef { pat, guard, body } => {
                f(N(pat));
                f(O(guard));
                f(N(body));
            }
            Tree::Labeled { expr } => f(N(expr)),
            Tree::Try { expr, cases, finalizer } => {
                f(N(expr));
                f(L(cases));
                f(O(finalizer));
            }
            Tree::Match { selector, cases } => {
                f(N(selector));
                f(L(cases));
            }
            Tree::Alternative { trees } => f(L(trees)),
            Tree::UnApply { fun, implicits, patterns } => {
                f(N(fun));
                f(L(implicits));
                f(L(patterns));
            }
            Tree::TypeTree { .. } => {}
            Tree::SingletonTypeTree { reference } => f(N(reference)),
            Tree::ByNameTypeTree { result } => f(N(result)),
            Tree::AppliedTypeTree { tpt, args } => {
                f(N(tpt));
                f(L(args));
            }
            Tree::TypeBoundsTree { lo, hi, alias } => {
                f(N(lo));
                f(N(hi));
                f(O(alias));
            }
            Tree::LambdaTypeTree { tparams, body } => {
                f(L(tparams));
                f(N(body));
            }
            Tree::Bind { name, body } => {
                f(Nm(*name));
                f(N(body));
            }
            Tree::ValDef { name, tpt, rhs } => {
                f(Nm(*name));
                f(N(tpt));
                f(O(rhs));
            }
            Tree::DefDef { name, paramss, tpt, rhs } => {
                f(Nm(*name));
                f(Ls(paramss));
                f(N(tpt));
                f(O(rhs));
            }
            Tree::TypeDef { name, rhs } => {
                f(Nm(*name));
                f(N(rhs));
            }
            Tree::Template { constr, parents, self_, body } => {
                f(N(constr));
                f(L(parents));
                f(O(self_));
                f(L(body));
            }
            Tree::Import { expr, selectors } => {
                f(N(expr));
                f(L(selectors));
            }
            Tree::ImportSelector { imported, renamed, bound } => {
                f(N(imported));
                f(O(renamed));
                f(O(bound));
            }
            Tree::PackageDef { pid, stats } => {
                f(N(pid));
                f(L(stats));
            }
            Tree::Inlined { call, bindings, expansion } => {
                f(O(call));
                f(L(bindings));
                f(N(expansion));
            }
        }
    }
}
