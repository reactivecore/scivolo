# 40 — Tree extractors on `Tpd` and borrowed type views on `Universe` (#172)

Date: 2026-10-29, on `integration-2026-10-29` (6d512f7c). [V] verified by grep/reading, [I] inferred.
Paths: `Y/` = `crates/typer/src/`, `L/` = `crates/lower/src/phases/`, `Lc/` = `crates/lower/src/codegen/`,
`T/` = `crates/tasty/src/`, `D/` = `ref/scala3/compiler/src/dotty/tools/dotc/`. Counts are grep hits
over `crates/*/src` (tests excluded); `crates/aot` has no `Tree::` match and 29 `types().get` [V].

**Recommendation.** One MR (§4): three tree extractors on `Tpd` (`class_template`, `callee`,
`closure_def`) converting ~75 sites, and one borrowed view family on `Universe` (`binding`,
`applied`, `strip_poly`) converting the ~55 Method/Poly/Lambda clone sites that do not mutate
`u` afterwards. Expected −90 to −120 lines, no behaviour change.

## 1. Recurring tree shapes

Existing extractors: `Y/tpd/info.rs` (61 lines) holds `meth_part` (48 uses), `strip_apply` (2),
`strip_typed` (12), `is_type_tree`, `lub_of` [V]. The chains below are what is still hand-written.

| # | Shape (let-else chain) | dotty | Sites | Examples | Saves |
|---|---|---|---|---|---|
| T1 | `TypeDef { rhs }` → `Template { constr, parents, self_, body }` | `ClassDef`/`Template` unapply (`D/ast/Trees.scala`) | 48 (all `L/`, `Lc/`; the typer's 17 `TypeDef { rhs` do not chain) | `L/outer_fields.rs:24,38,159`, `L/mixin.rs:31,47`, `L/getters.rs:22,131`, `Lc/class.rs:92`, `L/beta_reduce.rs:131` | 1 line each (2 lines → 1): ~48 |
| T2 | `Apply { fun }` → (`TypeApply { fun }`)* → `sym(fun)` = called method; three private copies of `callee` | `methPart(t).symbol`, `isSelfConstrCall` | 22 Apply→Select/TypeApply chains + 3 `callee` fns (`L/elideable.rs:90`, `L/repeated.rs:112` (with clause index), `Y/typer/purity.rs:161`) | `L/get_class.rs:41-42`, `L/inline_patterns.rs:65`, `L/interpolators.rs:69-71`, `L/beta_reduce.rs:134-135`, `L/elim_opaque.rs:14` | 1–2 lines each, −15 for the three copies: ~30 |
| T3 | `Block { stats, expr }` → `[def]` + `Closure { meth }` → `DefDef` | `closureDef`, `closureBody` (`D/ast/TreeInfo.scala:918,937`) | 6 of the 19 `Block` sites (`L/drop_for_map.rs:146`, `L/lambda_lift.rs:200`, `L/eta_reduce.rs:29-36`, `L/inline_patterns.rs:61`, `L/context_results.rs:267`, `L/sam_classes.rs:51`) | | 2 lines each: ~10 |
| T4 | `Typed { expr }` → `Apply` → `Select { qualifier }` → `New` (anonymous-class instance) | `AnonClass` unapply (`D/ast/tpd.scala:1444`) | 2 (`L/beta_reduce.rs:133-138`, `L/inline_patterns.rs:63-66`) | | 3 each; too few, skip [I] |
| T5 | `while let Apply\|TypeApply { fun }` loops re-implementing `meth_part` | `methPart` | 5 (`Lc/layout.rs:110`, `Lc/linktime.rs:19`, `L/context_results.rs:227`, `Y/typer/this_calls.rs:16`, `Y/namer/annotations.rs:76` (untyped, keep)) | | 2 each: ~8 |

Other `let Tree::` chains (`DefDef` 54, `ValDef` 25, `Select` 42, `CaseDef` 15) destructure a
definition the caller already knows it holds; no shared shape beyond the variant itself [V].

## 2. Type destructurings that clone

467 `.types().get(t).clone()` sites: typer 247, tasty 147, lower 64, macros 8, repl 1 [V]. Forms:

| Form | Sites | Of which first/only variant |
|---|---|---|
| `let Type::X {..} = ….clone() else` | 170 | Method 56, Applied 48, Poly 17, Lambda 13, Refined 6, Match 4, the rest Copy-only variants (TypeRef/TermRef 6, Sym 3, …) that need no clone at all |
| `match ….clone() { … }` | 217 (typer 119, lower 25, tasty ~70) | first arm Sym 33, Method 22, Poly 20, Applied 16, This 7, Or/And/Lambda 6 each |
| `if let Type::X = ….clone()` | 19 | Method/Applied/Poly |
| Already borrowed (`match *u.types().get(t) {`, `if let … = u.types().get(t)`) | 419 | the pattern to imitate; e.g. `Y/typer/implicit_views.rs:30` borrows `Poly { result }` and clones only the `Method` two lines later |

Variants with a `Vec`/`String`: `Method`, `Poly`, `Lambda` (all `binder, params: Vec<(String, TypeId)>, result`),
`Applied(tycon, Vec<TypeId>)`, `Match { cases }`, `Refined(_, String, _)`, `TypeRef`/`TermRef`/`Str`/`Local`
(a `String`). Only these force a clone; a Copy-field destructuring (`Sym`, `This`, `And`, `Or`, …)
in a cloning `match` clones for nothing [V].

**Borrow pattern.** `Types::all` is append-only (`intern` pushes, never mutates) [V `T/types/mod.rs:115`],
so a `&Type` is stable in principle, but the borrow of `u` blocks `u.intern_type`, `u.define`,
`u.subst`, `self.strip_implicit(..)` etc. in the body. Sample of the 86 Method/Poly/Lambda let-else
sites: only ~1 in 2 lines after the `let` calls `u` mutably [V grep -A1]; by reading, the
sites fall into three groups [I from 15 read sites]:

| Group | Sites (est.) | Example | Borrowed view works? |
|---|---|---|---|
| Reads params/result only, then compares or returns | ~40 | `Y/typer/applicability.rs:164,190`, `Y/typer/eta.rs:24`, `Y/typer/named_overloads.rs:31`, `L/calls.rs:61`, `L/specialize.rs:78` | yes: match on `&Type` or a view |
| Copies `binder`/`result` (Copy) and `params` types, then mutates `u` | ~30 | `Y/typer/implicit_views.rs:33`, `Y/typer/sams.rs:64`, `L/anon_class.rs:64`, `Y/inlining/expand.rs:136` | yes if the view yields `params: &[(String, TypeId)]` and the caller collects `Vec<TypeId>` (or the names) before mutating; the `String` clones go away, the `Vec<TypeId>` stays |
| Needs the names *and* mutates with them (`subst(info, binder, &names, &args)`) | ~15 | `L/anon_class.rs:64-68`, `Y/unpickle/assign.rs:114,140`, `L/context_results.rs:166` | clone of the names unavoidable; keep `.clone()` or collect names first |

`Applied` (48 let-else + 16 match): about half read `args` only (`args.len()`, `args[i]`,
`args.iter()`) → `&[TypeId]` view; the other half pass `args` to `u.intern_type(Type::Applied(..))`
or `map`, which copies a `Vec<TypeId>` anyway (`to_vec()` on the slice, same cost) [I].

## 3. Existing helpers (do not duplicate)

| Helper | Where | Covers |
|---|---|---|
| `meth_part`, `strip_apply`, `strip_typed`, `is_type_tree` | `Y/tpd/info.rs` | T5 loops; `callee` = `tpd.sym(tpd.meth_part(t))` only when `t` has no `Apply` left to count |
| `callee` ×3 (private) | `L/elideable.rs:90`, `L/repeated.rs:112`, `Y/typer/purity.rs:161` | T2 — to be replaced |
| `Universe::result_type(&self, t)` (loops Method/Poly to the final result) | `T/symbols/overrides.rs:132` | dotty `finalResultType`; borrowed already |
| `dealias`, `widen`, `widen_once` | `T/types/widen.rs` | `widenDealias` |
| `Typer::strip_implicit(&self, t)`, `final_result(&mut self, t)` | `Y/typer/adapt_by_result.rs:93`, `Y/typer/avoid.rs:89` | implicit-clause peel (typer only) |
| `function_parts`, `context_function_parts` → `Parts` | `Y/typer/function_types.rs:35,46` | `FunctionN[..]` args as `(params, result)` |
| `repeated_elem`, `repeated_param_elem` | `Y/typer/repeated.rs:22`, `Y/typer/by_name_repeated.rs:11` | `T*` element |
| `class_symbol`, `class_param`, `resolve_ref`, `base_type` | `T/symbols/classes.rs`, `resolve.rs`, `base_types.rs` | symbol of a type (all `&mut self`) |
| `erased_method_type`, `Lower::param_types`/`result_type` | `T/erasure/mangle.rs:105`, `Lc/erased.rs:88,93` | erased signatures |
| 41 `Type::Poly { result, .. }` peels before a `Method` match | spread over `Y/`, `L/`, `T/` | dotty `stripPoly`; no helper yet [V] |

## 4. Plan for one MR (~2 h, pure refactor)

**A. Tree extractors** in `Y/tpd/info.rs` (stays < 200 lines; dotty `TreeInfo` notice already there):

| Fn | Signature | Body | Converts |
|---|---|---|---|
| `class_template` | `fn class_template(&self, td: TNodeId) -> Option<Template>` with `pub struct Template { pub constr: TNodeId, pub parents: List, pub self_: Option<TNodeId>, pub body: List }` in `info.rs` | `TypeDef { rhs }` then `Template {..}`, else `None` | the 48 T1 sites: `let Some(Template { body, .. }) = l.tpd.class_template(td) else { … }` (keep each site's `else` arm: `continue`, `return`, `unreachable!`) |
| `callee` | `fn callee(&self, t: TNodeId) -> Option<SymbolId>` | `self.sym(self.meth_part(t))`, `None` unless `meth_part` ends in `Ident`/`Select` | `Y/typer/purity.rs:161`, `L/elideable.rs:90` (delete both), T2 sites that only need the symbol (`L/elim_opaque.rs:14`, `L/get_class.rs:41-42`, `L/inline_patterns.rs:65`, `Lc/layout.rs:110`, `Lc/linktime.rs:19`, `L/context_results.rs:227`, `Y/typer/this_calls.rs:16`); `L/repeated.rs:112` keeps its clause count |
| `closure_def` | `fn closure_def(&self, t: TNodeId) -> Option<(TNodeId, TNodeId)>` (the `DefDef` and the `Closure`) | `Block { stats: [def], expr }` with `Closure { meth }` whose `meth` symbol is `def`'s | the 6 T3 sites |

**B. Type views** in a new `T/types/parts.rs` (`mod parts;` in `T/types/mod.rs`, re-exported with
`Type`; the view type lives beside `Type`):

```rust
/// The parts a `Method`, `Poly` or `Lambda` type binds.
pub struct Binding<'a> { pub binder: Binder, pub params: &'a [(String, TypeId)], pub result: TypeId,
                         pub implicit: bool, pub contextual: bool }
impl Binding<'_> { pub fn param_types(&self) -> Vec<TypeId>; pub fn names(&self) -> Vec<String> }
impl Universe {
    pub fn method(&self, t: TypeId) -> Option<Binding<'_>>;   // Type::Method only
    pub fn poly(&self, t: TypeId) -> Option<Binding<'_>>;
    pub fn lambda(&self, t: TypeId) -> Option<Binding<'_>>;
    pub fn applied(&self, t: TypeId) -> Option<(TypeId, &[TypeId])>;
    pub fn strip_poly(&self, t: TypeId) -> TypeId;             // dotty stripPoly: result of a Poly, else t
}
```

Conversion rule per site: (1) a `let Type::X {..} = ….clone() else` whose body never touches `u`
mutably → `let Some(m) = u.method(info) else {…}` and `m.params`/`m.result`; (2) a site that
mutates afterwards but needs only Copy parts plus param types → `let (binder, result, params) =
{ let m = u.method(info)?; (m.binder, m.result, m.param_types()) };` (one statement, borrow ends);
(3) a site that needs names and mutates (`subst`) → leave the clone, or `m.names()` under rule 2.
Also replace the 41 `Poly { result, .. }` peels with `u.strip_poly(t)`, and drop `.clone()` where
the destructured variant has only Copy fields (`Sym`, `This`, `And`, `Or`, `TypeRef` with `..`).

Order and budget: A1 `class_template` (48 sites, 30 min, mechanical), A2 `callee` (10 sites + 2
deletions, 15 min), B `Binding`/`applied`/`strip_poly` + the ~40 group-1 sites in `Y/typer/` and
`L/` (60 min), A3 `closure_def` (6 sites, 15 min). Stop at 2 h; the remaining clone sites stay
for a later MR under #172 (tasty's own 147, mostly in `subtype/` and `show_user/`, are a separate
pass: they live inside `&mut self` methods of `Universe` and need the same rule 2).

Expected delta: A −60 lines, B −30 to −50 (plus ~50 new in `info.rs`/`parts.rs`): net −90 to −120 [I].

Verification (pure refactor): `just gates`; `just snapshot-diff programs lower typer` unchanged
(oracles `programs`, `lower`, `typer` in `scripts/snapshot.py:77-84`) [V]; `just lint-msrv` for
the `let … else` chains touched. No new tests: the extractors are covered by every phase run.
