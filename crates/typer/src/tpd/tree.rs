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
    /// `mix` is `None` for dotty's empty identifier.
    Super {
        qual: TNodeId,
        mix: Option<TName>,
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
    /// `from` is an identifier of the method returned from.
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
    /// Its symbol is the class's; `self_` is `None` for `EmptyValDef`.
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
}
