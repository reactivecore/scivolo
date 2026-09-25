//! The NIR data model (Scala Native 0.5, compat 6). Interned nodes are referenced by id into a `Pool`.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StrId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GlobalId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ValId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PosId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Local(pub u64);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Global {
    None,
    Top(StrId),
    Member(GlobalId, StrId),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Pos {
    pub file: StrId,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    Vararg,
    Bool,
    Ptr,
    Char,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    Size,
    Int128,
    ArrayValue(TypeId, u32),
    StructValue(Vec<TypeId>),
    Function(Vec<TypeId>, TypeId),
    Null,
    Nothing,
    Virtual,
    Var(TypeId),
    Unit,
    Array { elem: TypeId, nullable: bool },
    Ref { name: GlobalId, exact: bool, nullable: bool },
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Val {
    True,
    False,
    Null,
    Unit,
    Zero(TypeId),
    Char(u16),
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    Size(u64),
    Int128(i64, u64),
    StructValue(Vec<ValId>),
    ArrayValue(TypeId, Vec<ValId>),
    ByteString(Vec<u8>),
    Local(Local, TypeId),
    Global(GlobalId, TypeId),
    Const(ValId),
    String(StrId),
    Virtual(u64),
    ClassOf(GlobalId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bin {
    Iadd,
    Fadd,
    Isub,
    Fsub,
    Imul,
    Fmul,
    Sdiv,
    Udiv,
    Fdiv,
    Srem,
    Urem,
    Frem,
    Shl,
    Lshr,
    Ashr,
    And,
    Or,
    Xor,
}

pub const BINS: [Bin; 18] = {
    use Bin::*;
    [Iadd, Fadd, Isub, Fsub, Imul, Fmul, Sdiv, Udiv, Fdiv, Srem, Urem, Frem, Shl, Lshr, Ashr, And, Or, Xor]
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Comp {
    Ieq,
    Ine,
    Ugt,
    Uge,
    Ult,
    Ule,
    Sgt,
    Sge,
    Slt,
    Sle,
    Feq,
    Fne,
    Fgt,
    Fge,
    Flt,
    Fle,
}

pub const COMPS: [Comp; 16] = {
    use Comp::*;
    [Ieq, Ine, Ugt, Uge, Ult, Ule, Sgt, Sge, Slt, Sle, Feq, Fne, Fgt, Fge, Flt, Fle]
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conv {
    Trunc,
    Zext,
    Sext,
    Fptrunc,
    Fpext,
    Fptoui,
    Fptosi,
    Uitofp,
    Sitofp,
    Ptrtoint,
    Inttoptr,
    Bitcast,
    SSizeCast,
    ZSizeCast,
}

pub const CONVS: [Conv; 14] = {
    use Conv::*;
    [
        Trunc, Zext, Sext, Fptrunc, Fpext, Fptoui, Fptosi, Uitofp, Sitofp, Ptrtoint, Inttoptr, Bitcast,
        SSizeCast, ZSizeCast,
    ]
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryOrder {
    Unordered,
    Monotonic,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

pub const ORDERS: [MemoryOrder; 6] = {
    use MemoryOrder::*;
    [Unordered, Monotonic, Acquire, Release, AcqRel, SeqCst]
};

#[derive(Clone, Debug)]
pub enum Op {
    Call { ty: TypeId, ptr: ValId, args: Vec<ValId> },
    Load { ty: TypeId, ptr: ValId, order: Option<MemoryOrder> },
    Store { ty: TypeId, ptr: ValId, value: ValId, order: Option<MemoryOrder> },
    Elem { ty: TypeId, ptr: ValId, indexes: Vec<ValId> },
    Extract { aggr: ValId, indexes: Vec<i32> },
    Insert { aggr: ValId, value: ValId, indexes: Vec<i32> },
    Stackalloc { ty: TypeId, n: ValId },
    Bin { bin: Bin, ty: TypeId, l: ValId, r: ValId },
    Comp { comp: Comp, ty: TypeId, l: ValId, r: ValId },
    Conv { conv: Conv, ty: TypeId, value: ValId },
    Fence(MemoryOrder),
    Classalloc { name: GlobalId, zone: Option<ValId> },
    Fieldload { ty: TypeId, obj: ValId, name: GlobalId },
    Fieldstore { ty: TypeId, obj: ValId, name: GlobalId, value: ValId },
    Field { obj: ValId, name: GlobalId },
    Method { obj: ValId, sig: StrId },
    Dynmethod { obj: ValId, sig: StrId },
    Module(GlobalId),
    As { ty: TypeId, obj: ValId },
    Is { ty: TypeId, obj: ValId },
    Copy(ValId),
    SizeOf(TypeId),
    AlignmentOf(TypeId),
    Box { ty: TypeId, obj: ValId },
    Unbox { ty: TypeId, obj: ValId },
    Var(TypeId),
    Varload(ValId),
    Varstore { slot: ValId, value: ValId },
    Arrayalloc { ty: TypeId, init: ValId, zone: Option<ValId> },
    Arrayload { ty: TypeId, arr: ValId, idx: ValId },
    Arraystore { ty: TypeId, arr: ValId, idx: ValId, value: ValId },
    Arraylength(ValId),
}

#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub id: Local,
    pub ty: TypeId,
}

#[derive(Clone, Debug)]
pub enum Next {
    None,
    Unwind(Param, Box<Next>),
    Case(ValId, Box<Next>),
    Label(Local, Vec<ValId>),
}

#[derive(Clone, Debug)]
pub enum Cond {
    Simple { property: StrId, comp: Comp, value: ValId, pos: PosId },
    Complex { bin: Bin, left: Box<Cond>, right: Box<Cond>, pos: PosId },
}

#[derive(Clone, Debug)]
pub enum Inst {
    Label(Local, Vec<Param>),
    Let { id: Local, op: Op, unwind: Next, scope: u32 },
    Ret(ValId),
    Jump(Next),
    If(ValId, Next, Next),
    Switch(ValId, Next, Vec<Next>),
    Throw(ValId, Next),
    Unreachable(Next),
    LinktimeIf(Box<Cond>, Next, Next),
}

#[derive(Clone, Debug)]
pub enum Attr {
    MayInline,
    InlineHint,
    NoInline,
    AlwaysInline,
    MaySpecialize,
    NoSpecialize,
    UnOpt,
    NoOpt,
    DidOpt,
    BailOpt(StrId),
    Extern { blocking: bool },
    Link(StrId),
    Dyn,
    Stub,
    Abstract,
    Volatile,
    Final,
    SafePublish,
    LinktimeResolved,
    UsesIntrinsic,
    Align { size: i32, group: Option<StrId> },
    Define(StrId),
    LinkCppRuntime,
}

#[derive(Clone, Debug)]
pub struct LexicalScope {
    pub id: u32,
    pub parent: u32,
    pub pos: PosId,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub insts: Vec<Inst>,
    pub positions: Vec<PosId>,
    pub local_names: Vec<(Local, StrId)>,
    pub scopes: Vec<LexicalScope>,
}

#[derive(Clone, Debug)]
pub enum DefnKind {
    Var { ty: TypeId, rhs: ValId },
    Const { ty: TypeId, rhs: ValId },
    Declare { ty: TypeId },
    Define { ty: TypeId, body: Body },
    Trait { traits: Vec<GlobalId> },
    Class { parent: Option<GlobalId>, traits: Vec<GlobalId> },
    Module { parent: Option<GlobalId>, traits: Vec<GlobalId> },
}

#[derive(Clone, Debug)]
pub struct Defn {
    pub name: GlobalId,
    pub attrs: Vec<Attr>,
    pub pos: PosId,
    pub kind: DefnKind,
}

impl Attr {
    pub fn is_extern(&self) -> bool {
        matches!(self, Attr::Extern { .. })
    }
}
