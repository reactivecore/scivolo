# 35 — Local classes, types and imports in inline bodies

Date: 2026-10-23. Claims are [V] verified by running or reading, [I] inferred. Paths: `D/` =
`ref/scala3/compiler/src/dotty/tools/dotc/`, `Yi/` = `crates/typer/src/inlining/`, `Yu/` =
`crates/typer/src/unpickle/`, `Y/` = `crates/typer/src/typer/`, `L/` = `crates/lower/src/phases/`,
`S/` = `crates/tasty/src/symbols/`. Measured with `just dotty-lower-report` on
`origin/integration-2026-10-23` (1232 of dotty's `tests/run` pass, 246 stop at the typer) [V].

**Recommendation.** Four steps of about an hour: fresh class symbols in the inliner's copy (§3 S1,
5 tests), inline calls inside copied members (S2, 8 more), the lowering checked against Scala Native
names with the probe `tmp/l35/InlineLocal.scala` (S3), then class definitions unpickled from library
inline bodies (S4: 25333, upickle's `macroR`/`macroW`, #144). Imports are dropped, not copied (§2.3).

## 1. The two refusals and the 14 tests

| site | message | tests (fresh `lower-results.tsv`) [V] |
|---|---|---|
| `Yi/copy.rs:89-91` | "inline body with a local type, class or import" | i11050, i11174, i11174local, i12328, i13334, i24357, implicitFuns2, t5665, toplevel-mixed, typeclass-derivation1, typeclass-derivation2b, typelevel-numeric (12) |
| `Yu/locals.rs:37-38` | "local type or class in a library inline body", "import in a library inline body" | 25333 (a quote: `new Callback: class Worker …`, through `Yu/quotes.rs` → `block`); upickle `macroR`/`macroW` (research 31 §3.2, `new CaseClassReader3[T](…) { override … }`) [I: sources missing from the jar] |

What the 12 contain [V sources]:

- An anonymous class in an inline *argument*, copied by `fresh_copy` (`copy.rs:46`): `assert(new
  O.T{}.f(0) == 42)` (t5665), `assert(caughtE1orE2(throw new E1 {}))` (i24357), `(new x("abc"){}).s`
  (toplevel-mixed), `new Foo3{}.foo` (implicitFuns2).
- An anonymous class as the inline body: `inline given (using DFC): TC = new TC: foo()` (i13334),
  `new Schema[T] { }` (i12328), `new MathLib[N] { import n.*; def dotProduct … }` (typelevel-numeric,
  an import *inside* the template).
- Anonymous or named local classes whose members contain inline matches or inline calls, expanded
  only after the copy: `class InlinedShow extends Show[T] { def show(x: T) = inline ev match … }`
  (i11050), `new EnumerateNames[T] { … inline ev match … }` (i11174, i11174local), `new { def
  equals(x: T, y: T) = deriveForSum[Alts](x, y) }` (typeclass-derivation1, -2b).

Research 34 §3 I1 counted 7; the fresh report lists 12 at the same site. Each may stop later
elsewhere (typeclass derivation, mirrors, `summonInline` in members): the step order takes the
simple shapes first.

## 2. Design

### 2.1 What dotty does

`Inliner.scala:775-776` [V] runs an `InlinerMap` (a `TreeTypeMap`) over the body with `oldOwners =
inlinedMethod`, `newOwners = ctx.owner`. `TreeTypeMap.transform` (`D/ast/TreeTypeMap.scala:100-
108`) handles `Template` by `withMappedSyms(localSyms(impl :: self :: Nil))`: the local dummy and
the self symbol get copies, the constructor and body are transformed with the mapped owner; every
`Block` and `Inlined` goes through `transformDefs` (`:163-166`), which maps *all* local symbols of
its statements (`localSyms`: vals, defs, classes, type aliases) through `mapSymbols`
(`D/core/Symbols.scala:920-988`). `mapSymbols` copies each symbol with the mapped owner, then
completes the copies with a lazy type: for a class a `ClassInfo` with mapped parents, cloned
`decls`, mapped type parameters (`mapSymbols(original.typeParams, …, mapAlways = true)`), the self
info substituted; cross references are substituted with `ttmap.withSubstitution(originals, copies)`
(`This(old)` → `This(new)` by `mapOwnerThis`, `:59-73`). Members of the class are *not* copied by
`mapSymbols` (the scope is cloned); they are copied when the `Template`'s `transformDefs` runs. An
`Import` is kept as a tree and later dropped as an elideable statement (`Inliner.scala:41-44` [V]).

Naming: a lifted local class is `owner.flatName $ name.freshened`, the counter per compilation
unit and base name in `LambdaLift` traversal order (`D/transform/LambdaLift.scala:66-71`,
`D/core/NameOps.scala:184-189` [V]). The probe `tmp/l35/InlineLocal.scala` compiled by Scala
Native 0.5.12 / Scala 3.9.0 (`tmp/sn/3.9.0/InlineLocal`) gives [V]: `Lib.mk` (a local `class
Foo`) inlined into `Test.main` → `Test$Foo$1`, into `Other.go` → `Other$Foo$2`; `Lib.anon` (`new
Runnable {…}`) → `Test$$anon$1`, `Other$$anon$2`; the caller's own `class Local` → `Test$Local$1`.
So an inlined local class is named as a local class *of the caller*, numbered in the order it is
met in the caller's unit, no trace of `Lib`. `L/local_lift.rs:76-94 name_class` already builds
`outer$base$N` with `l.fresh` per unit [V], and `L/drop_inlined.rs:48-56` gives trees from another
source the call's span, so `LocalClass.at` (`L/lift_names.rs:29-33`) orders inlined classes at
their call [V code, I for the order].

### 2.2 The inliner's copy (`Yi/copy.rs`)

Today `TreeTypeMap { refs, syms, old_owner, new_owner, this }` maps `Sym(None, s)` through `syms`
and `This(c)` only through the proxies in `this` (`:31-43`); `fresh_symbols` makes copies of vals,
defs, binds and type parameters with the owner mapped, then maps their infos (`:139-175`, dotty's
two phases). Port of `mapSymbols` for classes:

1. `Block` (`:63-72`): collect `TypeDef` statements too (classes and aliases) — dotty's `localSyms`.
2. `declared_fresh` for `Kind::Class`: after the class copy `n`, copy its `decls` recursively with
   owner `n` (type parameters, the primary constructor and its parameters, members, the local dummy
   `<local C>`; `S/define.rs:12-30 define` enters a symbol into its owner's `decls`, so no `enter`
   call); then, in the second phase, `set_parents(n, parents(s).map(map_type))` and
   `set_self_type` — `S/classes.rs:17-26 parents` decodes from a tree only, and the copy has
   `tree: None` [V]. `Kind::Type`: like a val, its info (the alias) mapped.
3. `map_type`: `This(c)` with `class_symbol(c) ∈ syms` → `This(Sym(None, new))`; `Sym(Some(pre),
   s)` with `s ∈ syms` → `Sym(Some(pre), new)` (members selected through `this` have a prefix;
   today only `Sym(None, _)` is mapped, `:33-36`).
4. `copy` (`:89-91`): `TypeDef` and `Template` fall into the general path: the node's `sym` (class
   or local dummy, `Y/classes.rs:139`) maps through `syms` (`:95`), children are copied. A `TypeDef`
   met outside a `Block` (a member class of a local class) is already fresh through its owner.
5. `Import`: dropped from the copied statements (dotty keeps and elides it; nothing reads imports
   after typing, `L/first_transform.rs:140`, `L/erasure/members.rs:36` only pass them through).
6. `no_inits` (`Yi/no_inits.rs:14-27`) is keyed by class symbol: `note_inits` on the copied
   `TypeDef`, else `is_no_inits_class(new)` is wrong for module elision.

`fresh_copy` of an argument (`old_owner: None`) keeps the class's owner, the caller's method —
right, the argument was typed there. `drop_unused` already treats an unused `TypeDef` as elideable
(`Yi/unused.rs:186`), as dotty does.

### 2.3 Inline calls inside copied members (`Yi/retype.rs`)

`retype_node` lists the trees it descends into (`:98-117`) and switches the owner for `ValDef`/
`DefDef` only (`:118-121`); `TypeDef` and `Template` are missing, so an inline match inside a
copied member body (i11050, i11174, typeclass-derivation*) would stay unexpanded and the lowering
would refuse it. Add both to the list, with the owner switched to the class (`TypeDef`) and the
local dummy or class (`Template`), member bodies then switch to the member. `this` proxies
(`Yi/this_proxy.rs`) of a call inside a member refer to the local class's `this`, which must not be
proxied: `enclosing_class` (`Yi/inline_call.rs:43-47`) walks owners, so with the owner switched it
finds the local class [I].

### 2.4 The lowering

After the inliner, the class definition is an ordinary local class of the caller: `drop_inlined`
makes the `Inlined` a block whose statements are the argument bindings, `walk.rs:132 class_defs`
collects every `TypeDef` with a `Template`, `L/local_classes.rs:26-60` sees `owner` = the caller's
method, a val or a local dummy (all terms, `local_free.rs:54`) and proxies the free values — the
bindings `x$proxy` and `C_this` of the expansion — like any capture. The lowering tells unit classes
from library classes by `symbol.tree.is_some()` (`L/outer_args.rs:34`, `L/mixin.rs:21,61,149`,
`L/bridges.rs:131`) [V]: the copy has `tree: None`, so a class unpickled from a library body (§2.5)
must also go through the copy, which every expansion does (`Yi/expand.rs:77-80`). Risks, to be
checked with `just nir-cmp tmp/l35/InlineLocal.scala 'Test?Foo?1'` etc. once S1 types the probe
[I]: a `same-file` inline body keeps its own spans (`reposition` only changes spans from another
file or source), so two copies of one class in one method share `at` and sort by definition order;
`L/local_classes.rs` requires a `<init>` in `decls` (`L/constructors.rs:149`); `lambda_lift.rs:78`
"lifting into a class of another unit" must not fire for the copy's owner chain.

### 2.5 Library inline bodies (`Yu/locals.rs`, `Yu/defs.rs`)

dotty's `TreeUnpickler` creates the symbols of a `TYPEDEF` with a `TEMPLATE` as members
(`createMemberSymbol`, `D/core/tasty/TreeUnpickler.scala:636`), the template's local dummy
(`:609-612`) and reads it with `readTemplate` (`:1095-1200`: `indexStats` of the body before the
parents, parents, `SELFDEF`, the constructor as the first `DEFDEF`). Our `S/entries.rs:49-88 walk`
already enters library classes this way (class, its template's params and members, `tree:
Some((file, id))`), but stops at method bodies (`:36`). Port:

1. `define_locals` (`locals.rs:32-43`): `TYPEDEF` → `define_at(id, owner, false)` extended with
   `View::TypeDef { is_class: true, rhs }` → `Kind::Class`, then `define_at(k, class, true)` for the
   template's `TYPEPARAM`/`PARAM`/`VALDEF`/`DEFDEF`/`TYPEDEF` kids, and a `Kind::Val` `<local C>` as
   `Y/classes.rs:178`. `parents(class)` and `info(member)` then decode from the tree as for library
   classes [V `S/classes.rs:21`, `S/info.rs:16-20`]; `admitted.rs::check` each member's info.
   `is_class: false` → `Kind::Type` (alias; `info` decodes it). `IMPORT`/`EXPORT`: skipped.
2. `local_def` (`defs.rs:71`): `TYPEDEF` → `class_def`: `T::TypeDef { rhs: T::Template { constr:
   def_def(ctor, ctor_sym), parents: terms (`APPLY`/`NEW`) or `tpt`s, self_: `SELFDEF` as a
   `ValDef`, body: members through `local_def`/`class_def`, statements through `term` with
   `owner` = the local dummy } }`, the `Template` node's `sym` as `Y/classes.rs:139`, positioned.
3. `avoiding` (`locals.rs:100-117`): the block's type `$anon` (a local class) is not avoided by
   `widen`; use the typer's avoidance (`Y/avoid.rs`, parents intersection) as `Yi/blocks.rs:68`
   does, or refuse only when that fails.

25333 reaches this through `quotes.rs` (same `block`). upickle's `macroR`/`macroW` also need
research 31's other items (`m.MirroredElemTypes`, local implicits in `Yi/host.rs:38-40`).

Local objects (a module val plus module class, `S/companions.rs:21-35` finds the class by name
in the owner's decls), companions of local classes and local enums are out of scope: no test in
the list uses them; a probe decides whether the copy of two symbols under one owner suffices.

## 3. Steps

| step | what | tests | lines | files | dotty |
|---|---|---|---:|---|---|
| S1 | fresh class symbols, aliases, imports dropped (§2.2 1-6) | i13334, i24357, t5665, toplevel-mixed, i12328, typelevel-numeric, implicitFuns2 | 110 | `Yi/copy.rs` (split: `copy_classes.rs` for `declared_fresh` of classes), `Yi/no_inits.rs` | `D/ast/TreeTypeMap.scala:59-73,100-108,163-166`, `D/core/Symbols.scala:920-988` |
| S2 | retype into `TypeDef`/`Template`, owners of members (§2.3) | i11050, i11174, i11174local, typeclass-derivation1, typeclass-derivation2b | 40 | `Yi/retype.rs`, `Yi/inline_call.rs` | `D/inlines/Inliner.scala:775-800` |
| S3 | lowering: `tests/programs/inline-local-classes` from the probe, `just nir-cmp` of `Test?Foo?1`, `Test??anon?1`, `Other?Foo?2`; fixes in `L/lift_names.rs`/`L/drop_inlined.rs` as found (§2.4) | the S1/S2 tests through the VM | 40 | `L/lift_names.rs`, `L/local_classes.rs`, `tests/programs/` | `D/transform/LambdaLift.scala:66-71`, `D/core/NameOps.scala:184-189` |
| S4 | unpickle local classes and aliases, skip imports, avoid `$anon` (§2.5) | 25333; `tmp/eval-lihaoyi` upickle `derives` one blocker further (#144) | 130 | `Yu/locals.rs`, `Yu/defs.rs` (new `Yu/class_defs.rs`), `S/entries.rs` (share `walk`'s member kinds) | `D/core/tasty/TreeUnpickler.scala:604-660,824-864,1095-1200` |

Each step: `just gates`, `just dotty-lower-report FILTER` for its tests, `just snapshot-diff programs
lower` without worsened rows. S1 before S2 before S3; S4 after S1 (its trees go through the copy).
A step whose tests stop at a new site records that site in the MR instead of chasing it.
