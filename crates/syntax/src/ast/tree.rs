//! The untyped trees the parser builds: dotty's `untpd` trees, with fields in the order of
//! the Scala case classes. `None` stands for `EmptyTree` (and `EmptyValDef` for `Template.self`).

use super::constant::Constant;
use crate::names::Name;

use std::num::NonZeroU32;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// The id of the node at `index` in the arena's node pool.
    pub fn new(index: usize) -> NodeId {
        NodeId(NonZeroU32::new(index as u32 + 1).expect("a node index"))
    }

    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

pub type Opt = Option<NodeId>;

/// A list of trees, a slice of the arena's list pool.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct List {
    start: u32,
    len: u32,
}

/// A list of lists of trees (a method's parameter clauses), a slice of the clause pool.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Clauses {
    start: u32,
    len: u32,
}

impl List {
    pub fn new(start: usize, len: usize) -> List {
        List { start: start as u32, len: len as u32 }
    }

    /// The indices of the list in the list pool.
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

impl Clauses {
    pub fn new(start: usize, len: usize) -> Clauses {
        Clauses { start: start as u32, len: len as u32 }
    }

    /// The indices of the clauses in the clause pool.
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ModsId(u32);

impl ModsId {
    pub fn new(index: usize) -> ModsId {
        ModsId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A term or type name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TName {
    pub name: Name,
    pub is_type: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumberKind {
    Whole(u8),
    Decimal,
    Floating,
}

/// `Match`, `InlineMatch` or `SubMatch`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatchKind {
    Plain,
    Inline,
    Sub,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenCheckMode {
    Ignore,
    Filtered,
    Check,
    CheckAndFilter,
    FilterNow,
    FilterAlways,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Tree {
    Ident {
        name: TName,
    },
    Select {
        qualifier: NodeId,
        name: TName,
    },
    /// `qual` is `EmptyTypeIdent` when absent.
    This {
        qual: Opt,
    },
    Super {
        qual: NodeId,
        mix: Opt,
    },
    Apply {
        fun: NodeId,
        args: List,
    },
    TypeApply {
        fun: NodeId,
        args: List,
    },
    Literal {
        value: Constant,
    },
    New {
        tpt: NodeId,
    },
    Typed {
        expr: NodeId,
        tpt: NodeId,
    },
    NamedArg {
        name: TName,
        arg: NodeId,
    },
    Assign {
        lhs: NodeId,
        rhs: NodeId,
    },
    Block {
        stats: List,
        expr: Opt,
    },
    If {
        cond: NodeId,
        thenp: NodeId,
        elsep: Opt,
        inline: bool,
    },
    Match {
        selector: Opt,
        cases: List,
        kind: MatchKind,
    },
    CaseDef {
        pat: NodeId,
        guard: Opt,
        body: NodeId,
    },
    Return {
        expr: Opt,
    },
    WhileDo {
        cond: NodeId,
        body: NodeId,
    },
    Quote {
        body: NodeId,
    },
    Splice {
        expr: NodeId,
    },
    SplicePattern {
        body: NodeId,
    },
    TypeTree,
    SingletonTypeTree {
        reference: NodeId,
    },
    RefinedTypeTree {
        tpt: Opt,
        refinements: List,
    },
    AppliedTypeTree {
        tpt: NodeId,
        args: List,
    },
    LambdaTypeTree {
        tparams: List,
        body: NodeId,
    },
    TermLambdaTypeTree {
        params: List,
        body: NodeId,
    },
    MatchTypeTree {
        bound: Opt,
        selector: NodeId,
        cases: List,
    },
    ByNameTypeTree {
        result: NodeId,
    },
    TypeBoundsTree {
        lo: Opt,
        hi: Opt,
        alias: Opt,
    },
    Bind {
        name: TName,
        body: NodeId,
        mods: ModsId,
    },
    Alternative {
        trees: List,
    },
    ValDef {
        name: TName,
        tpt: NodeId,
        rhs: Opt,
        mods: ModsId,
    },
    DefDef {
        name: TName,
        paramss: Clauses,
        tpt: NodeId,
        rhs: Opt,
        mods: ModsId,
    },
    TypeDef {
        name: TName,
        rhs: NodeId,
        mods: ModsId,
    },
    /// `parents_or_derived` ends with the `derived` classes of a `derives` clause.
    Template {
        constr: NodeId,
        parents_or_derived: List,
        self_: Opt,
        body: List,
        derived: Option<u32>,
    },
    Import {
        expr: NodeId,
        selectors: List,
    },
    Export {
        expr: NodeId,
        selectors: List,
    },
    PackageDef {
        pid: NodeId,
        stats: List,
    },
    Annotated {
        arg: NodeId,
        annot: NodeId,
    },
    Thicket {
        trees: List,
    },
    ModuleDef {
        name: TName,
        imp: NodeId,
        mods: ModsId,
    },
    ParsedTry {
        expr: NodeId,
        handler: Opt,
        finalizer: Opt,
    },
    SymbolLit {
        value: Box<[u16]>,
    },
    InterpolatedString {
        id: TName,
        segments: List,
    },
    /// A closure or function type; `mods` for `FunctionWithMods`, `wildcard` for a
    /// `WildcardFunction` made of placeholder parameters.
    Function {
        args: List,
        body: NodeId,
        mods: Option<(ModsId, Box<[bool]>)>,
        wildcard: bool,
    },
    PolyFunction {
        targs: List,
        body: NodeId,
    },
    InfixOp {
        left: NodeId,
        op: NodeId,
        right: NodeId,
    },
    PostfixOp {
        od: NodeId,
        op: NodeId,
    },
    PrefixOp {
        op: NodeId,
        od: NodeId,
    },
    Parens {
        t: NodeId,
    },
    Tuple {
        trees: List,
    },
    Throw {
        expr: NodeId,
    },
    ForYield {
        enums: List,
        expr: NodeId,
    },
    ForDo {
        enums: List,
        body: NodeId,
    },
    GenFrom {
        pat: NodeId,
        expr: NodeId,
        check_mode: GenCheckMode,
    },
    GenAlias {
        pat: NodeId,
        expr: NodeId,
    },
    ContextBounds {
        bounds: NodeId,
        cx_bounds: List,
    },
    PatDef {
        mods: ModsId,
        pats: List,
        tpt: NodeId,
        rhs: Opt,
    },
    ExtMethods {
        paramss: Clauses,
        methods: List,
    },
    ContextBoundTypeTree {
        tycon: NodeId,
        param_name: TName,
        own_name: TName,
    },
    MacroTree {
        expr: NodeId,
    },
    ImportSelector {
        imported: NodeId,
        renamed: Opt,
        bound: Opt,
    },
    Number {
        digits: Box<str>,
        kind: NumberKind,
    },
    /// A reference to a compiler-known type the parser inserts: `scala.&` for `A with B`.
    TypedSplice {
        what: &'static str,
    },
    /// `EmptyTree` where a list or result holds it; never positioned.
    Empty,
}
