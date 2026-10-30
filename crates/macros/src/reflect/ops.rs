//! The `quotes.reflect` operations by name: the kinds of their arguments, the `Host` method that
//! runs each and its result.

use super::decode::{Args, Bits, Decode, Expr, Name, Num, OptSym, OptTree, Rest, Sym, Tree, Ty, TyShape};
use super::encode::Encode;
use super::{Answer, Fallible, UNKNOWN};
use crate::host::Host;
use tasty::{SymbolId, TypeId};
use typer::tpd::TNodeId;
use vm::Reflected;

macro_rules! ops {
    ($($op:literal => $method:ident($($arg:ident: $kind:ty),*) -> $ret:ty;)*) => {
        impl Host {
            /// Runs `quotes.reflect` operation `op` on `args` and `text`.
            pub fn run_reflect(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
                let mut args = Args::new(args, text);
                let mut out = (Vec::new(), None);
                match op {
                    $($op => {
                        $(let $arg = <$kind as Decode>::decode(self, &mut args)?;)*
                        let result: $ret = self.$method($($arg),*);
                        result.put(self, &mut out)?;
                    })*
                    _ => return Err(UNKNOWN),
                }
                Ok(out)
            }
        }
    };
}

ops! {
    // reports
    "error" => error(at: OptTree, message: Name) -> ();
    "errorAt" => error_at(file: Num, start: Num, end: Num, message: Name) -> Fallible<()>;
    // type members
    "typeMembers" => type_members(s: Sym) -> Fallible<Vec<SymbolId>>;
    "fieldMembers" => field_members(s: Sym, name: Name) -> Fallible<Vec<SymbolId>>;
    "isAliasType" => is_alias_type(s: Sym) -> bool;
    "requiredModule" => required_module(name: Name) -> Fallible<SymbolId>;
    // copies
    "copy" => copy(original: Expr, parts: Rest<OptTree>, name: Name) -> Fallible<i32>;
    // type kinds
    "typeKind" => type_kind(t: TyShape) -> i32;
    "typeParts" => type_parts(t: TyShape) -> Fallible<Vec<TypeId>>;
    "paramNum" => param_num(t: TyShape) -> Fallible<i32>;
    "lambdaParts" => lambda_parts(t: TyShape) -> Answer;
    // definitions
    "annotations" => annotations(s: Sym, cls: OptSym) -> Fallible<Vec<TNodeId>>;
    "symbolTree" => symbol_tree(s: Sym) -> Fallible<Option<TNodeId>>;
    "classBody" => class_body(t: Tree) -> Fallible<Vec<TNodeId>>;
    "defClauses" => def_clauses(t: Tree) -> Fallible<Vec<i32>>;
    "classParts" => class_parts(t: Tree) -> Answer;
    "copyClass" => copy_class(t: Tree, constr: Tree, self_: OptTree, n: Num, rest: Rest<Tree>, name: Name)
        -> Fallible<TNodeId>;
    "definitions" => set_definitions(defs: Rest<Expr>) -> ();
    // made definitions
    "freshName" => fresh_name(name: Name) -> String;
    "newVal" => new_val(owner: Sym, info: Ty, flags: Bits, name: Name) -> Fallible<SymbolId>;
    "newMethod" => new_method(owner: Sym, info: Ty, flags: Bits, name: Name) -> Fallible<SymbolId>;
    "newBind" => new_bind(owner: Sym, info: Ty, flags: Bits, name: Name) -> SymbolId;
    "bind" => bind(s: Sym, pattern: Tree) -> Fallible<TNodeId>;
    "valDef" => val_def(s: Sym, rhs: OptTree) -> Fallible<TNodeId>;
    "defParams" => def_params(s: Sym) -> Fallible<Vec<i32>>;
    "defDef" => def_def(s: Sym, rhs: OptTree) -> Fallible<TNodeId>;
    "closure" => closure(def: Tree) -> Fallible<TNodeId>;
    "etaExpand" => eta_expand(t: Expr, owner: Sym) -> Fallible<i32>;
    "lambdaOf" => lambda_of(t: Tree) -> Vec<TNodeId>;
    "copyDef" => copy_def(original: Tree, tpt: Tree, rhs: OptTree, params: Rest<Tree>, name: Name)
        -> Fallible<TNodeId>;
    // classes
    "newClass" => new_class(owner: Sym, flags: Bits, name: Name) -> Fallible<SymbolId>;
    "newModule" => new_module(owner: Sym, name: Name) -> [SymbolId; 2];
    "classParents" => class_parents(cls: Sym, parents: Rest<Ty>) -> Fallible<()>;
    "classConstructor" => class_constructor(cls: Sym, info: Ty) -> Fallible<Vec<i32>>;
    // polymorphic types
    "polyBinder" => poly_binder() -> TypeId;
    "polyParam" => poly_param(t: Ty, idx: Num) -> Fallible<TypeId>;
    "polyType" => poly_type(placeholder: Ty, types: Rest<Ty>, names: Name) -> Fallible<TypeId>;
    "typeBounds" => type_bounds(low: Num, hi: Num) -> Fallible<TypeId>;
    "enterDecls" => enter_decls(cls: Sym, decls: Rest<Sym>) -> Fallible<()>;
    "classDef" => class_def(cls: Sym, n: Num, trees: Rest<Tree>) -> Fallible<TNodeId>;
    "typeIdent" => type_ident(s: Sym) -> Fallible<TNodeId>;
    "applied" => applied(tpt: Tree, args: Rest<Tree>) -> TNodeId;
    "this" => this(cls: Sym) -> Fallible<TNodeId>;
    "fieldMember" => field_member(cls: Sym, name: Name) -> Option<SymbolId>;
    "overridingSymbol" => overriding_symbol(m: Sym, cls: Sym) -> Option<SymbolId>;
    "new" => new_(tpt: Tree) -> TNodeId;
    // declarations
    "declared" => declared(s: Sym, what: Name) -> Vec<SymbolId>;
    "requiredClass" => required_class(name: Name) -> Fallible<SymbolId>;
    "requiredMethod" => required_method(name: Name) -> Fallible<SymbolId>;
    "requiredPackage" => required_package(name: Name) -> Fallible<SymbolId>;
    "typeMember" => type_member(s: Sym, name: Name) -> Option<SymbolId>;
    // implicits
    "isWhileTyping" => is_while_typing() -> bool;
    "macroSettings" => macro_settings() -> String;
    "search" => search(pt: Ty) -> Fallible<Vec<i32>>;
    // type patterns
    "matchType" => match_type(scrutinee: Ty, pattern: Num) -> Fallible<i32>;
    // type reprs
    "showType" => show_type(t: Ty) -> Fallible<String>;
    "showTypeStructure" => show_type_structure(t: Ty) -> Fallible<String>;
    "typeName" => type_name(t: Ty) -> Fallible<String>;
    "appliedTo" => applied_to(tycon: Ty, targs: Rest<Ty>) -> TypeId;
    "dealias" => dealias(t: Ty) -> TypeId;
    "methodType" => method_type(kind: Num, types: Rest<Ty>, names: Name) -> Fallible<TypeId>;
    "byName" => by_name(t: Ty) -> TypeId;
    "isSingleton" => is_singleton(t: Ty) -> bool;
    "baseClasses" => base_classes(t: Ty) -> Fallible<Vec<SymbolId>>;
    "memberType" => member_type(t: Ty, s: Sym) -> Fallible<TypeId>;
    "termRef" => term_ref(qual: Ty, name: Name) -> Fallible<TypeId>;
    "annotation" => annotation(t: Ty) -> Fallible<TNodeId>;
    "symbolRef" => symbol_ref(s: Sym) -> TypeId;
    // members
    "primaryConstructor" => primary_constructor(s: Sym) -> Option<SymbolId>;
    "paramSymss" => param_symss(s: Sym) -> Fallible<Vec<i32>>;
    "caseFields" => case_fields(s: Sym) -> Vec<SymbolId>;
    "companionClass" => companion_class(s: Sym) -> Option<SymbolId>;
    "methodMembers" => method_members(s: Sym, name: Name) -> Fallible<Vec<SymbolId>>;
    "children" => children(s: Sym) -> Fallible<Vec<SymbolId>>;
    "symbolPos" => symbol_pos(s: Sym) -> Answer;
    // trees
    "treeKind" => tree_kind(t: Tree) -> i32;
    "tree" => shape(t: Tree) -> Answer;
    "showTree" => show_tree(t: Tree) -> Fallible<String>;
    "showStructure" => show_structure(t: Tree) -> Fallible<String>;
    "treeSymbol" => tree_symbol(t: Tree) -> Fallible<Option<SymbolId>>;
    "changeOwner" => change_owner(t: Expr, owner: Sym) -> Expr;
    "if" => if_(trees: Rest<Tree>) -> Fallible<TNodeId>;
    // terms
    "underlying" => underlying(t: Tree) -> TNodeId;
    "underlyingArgument" => underlying_argument(t: Tree) -> TNodeId;
    "tpe" => tpe(t: Tree) -> Fallible<TypeId>;
    "isExprOf" => is_expr_of(t: Tree, of: Ty) -> Fallible<bool>;
    "typeTree" => type_tree(t: Ty) -> TNodeId;
    "repeated" => repeated(trees: Rest<Tree>) -> Fallible<TNodeId>;
    "ref" => ref_(s: Sym) -> Fallible<TNodeId>;
    "identRef" => ident_ref(t: Ty) -> Fallible<TNodeId>;
    "selectUnique" => select_unique(qual: Tree, name: Name) -> Fallible<TNodeId>;
    "selectOverloaded" => select_overloaded(qual: Tree, n: Num, rest: Rest<Num>, name: Name)
        -> Fallible<TNodeId>;
    "select" => select(qual: Tree, s: Sym) -> Fallible<TNodeId>;
    "appliedToTypes" => applied_to_types(fun: Tree, targs: Rest<Ty>) -> Fallible<TNodeId>;
    "match" => match_(selector: Tree, cases: Rest<Tree>) -> TNodeId;
    "apply" => apply(fun: Tree, args: Rest<Tree>) -> Fallible<TNodeId>;
    "typed" => typed(expr: Tree, tpts: Rest<Tree>) -> Fallible<TNodeId>;
    "caseDef" => case_def(pat: Tree, guard: OptTree, body: Tree) -> TNodeId;
    "wildcard" => wildcard() -> TNodeId;
    "assign" => assign(lhs: Tree, rhs: Tree) -> TNodeId;
    "namedArg" => named_arg(arg: Tree, name: Name) -> TNodeId;
    "let" => let_(owner: Sym, rhs: Tree, flags: Bits, name: Name) -> Fallible<[TNodeId; 2]>;
    "betaReduce" => beta_reduce(t: Tree) -> Fallible<Option<TNodeId>>;
    "block" => block(trees: Rest<Tree>) -> Fallible<TNodeId>;
    // types
    "typeSymbol" => type_symbol(t: Ty) -> Option<SymbolId>;
    "termSymbol" => term_symbol(t: Ty) -> Fallible<Option<SymbolId>>;
    "typeArgs" => type_args(t: Ty) -> Vec<TypeId>;
    "widenTermRefByName" => widen_term_ref_by_name(t: Ty) -> TypeId;
    "classSymbol" => class_symbol(t: Ty) -> Option<SymbolId>;
    "baseType" => base_type(t: Ty, base: Sym) -> Fallible<TypeId>;
    "selectType" => select_type(t: Ty, s: Sym) -> TypeId;
    "widen" => widen(t: Ty) -> TypeId;
    "isSubtype" => is_subtype(a: Ty, b: Ty) -> Fallible<bool>;
    "isSameType" => is_same_type(a: Ty, b: Ty) -> Fallible<bool>;
    "constantType" => constant_type(t: Ty) -> Fallible<Reflected>;
    "typeConstructorOf" => type_constructor_of(name: Name) -> Fallible<TypeId>;
    "substituteTypes" => substitute_types(t: Ty, pairs: Rest<(Sym, Ty)>) -> TypeId;
    // constants
    "constant" => constant(t: Tree) -> Fallible<Reflected>;
    "literal" => literal(kind: Num, bits: Bits, text: Name) -> Fallible<TNodeId>;
    // symbols
    "companionModule" => companion_module(s: OptSym) -> Option<SymbolId>;
    "declaredMethod" => declared_method(s: OptSym, name: Name) -> Vec<SymbolId>;
    "spliceOwner" => splice_owner() -> SymbolId;
    "root" => root() -> SymbolId;
    "emptyPackage" => empty_package() -> SymbolId;
    "anyRefAlias" => any_ref_alias() -> Fallible<SymbolId>;
    "predefClassOf" => predef_class_of() -> Fallible<SymbolId>;
    "owner" => owner_of(s: Sym) -> Option<SymbolId>;
    "name" => name(s: Sym) -> String;
    "fullName" => full_name(s: Sym) -> String;
    "info" => info(s: Sym) -> TypeId;
    "flags" => flags(s: Sym) -> u64;
    "kind" => symbol_kind(s: Sym) -> i32;
    "moduleClass" => module_class(s: Sym) -> Option<SymbolId>;
    // positions
    "expansionPos" => expansion_pos() -> Answer;
    "pos" => pos(t: Tree) -> Answer;
    "position" => position(file: Num, start: Num, end: Num) -> Answer;
    "sourcePath" => source_path(file: Num) -> Fallible<String>;
    "sourceContent" => source_content(file: Num) -> Fallible<String>;
}
