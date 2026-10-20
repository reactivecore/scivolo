# 29 — Named tuples (#139)

Date: 2026-10-20. Claims are [V] verified by reading the code or running it, [I] inferred. Paths
as in research 28: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `C/` = `.../dotc/core/`,
`D/` = `.../dotc/ast/`, `P/` = `.../dotc/printing/`, `Y/` = `crates/typer/src/typer/`, `S/` =
`crates/tasty/src/`, `X/` = `crates/syntax/src/`. Method: `just typer-dump` on the eight tests of §3
(branch `139-named-tuples`), reading dotty 3.9.

**Recommendation.** Named tuples are a thin layer over plain tuples: a type alias
`NamedTuple.NamedTuple[Names, Values]`, a desugaring of literals to `NamedTuple.build`, and a
selection fallback in `typedSelect`. The parser already produces `Tree::Tuple` with `NamedArg`
elements [V `X/parse/trees.rs:14-25`, `X/parse/types/args.rs:10`], the desugarer leaves such tuples
alone [V `X/desugar/walk.rs:44`], the erasure expands opaque type lambdas [V `S/erasure/applied.rs:36-46`],
and the typer refuses in exactly two places [V `Y/tuple_xxl.rs:18,65`]. MR A (§4, ~150 lines)
gives the issue's REPL line; patterns and conversions follow in two more slices.

## 1. dotty 3.9

- Type: `(a: Int, b: String)` is `AppliedTypeTree(NamedTupleTypeRef, [("a", "b"), (Int, String)])`,
  i.e. `scala.NamedTuple.NamedTuple[("a", "b"), (Int, String)]` with the names as string singleton
  types [V `D/Desugar.scala:1843-1891`, `C/Definitions.scala:1007-1008`]. There is no
  `typedNamedTuple`; `typedTuple` (`T/Typer.scala:3762-3766`) calls `desugar.tuple` and retypes
  the result, and only arities over 22 stay a `Tuple` tree (`:3768-3790`, our `tuple_xxl.rs`).
- Literal: `(a = 42, b = "Hello")` becomes `NamedTuple.build[("a", "b")]()(Tuple2(42, "Hello"))`
  [V `:1883-1891`]; `build` is `inline def build[N <: Tuple]()[V <: Tuple](x: V): NamedTuple[N, V]`
  in the library, and `NamedTuple` is `opaque type NamedTuple[N <: Tuple, +V <: Tuple] >: V = V`
  [I from `C/Definitions.scala:1435-1450` and `mirror_sources.rs:151`; the library sources are not
  in `ref/`]. So a named tuple value *is* the values tuple; a plain tuple conforms to the named one
  by the lower bound `>: V`; the upper bound is `AnyNamedTuple`, not `Tuple`, which is why `adapt`
  inserts `.toTuple` (below) [V `NamedTuple` extractor `:1442-1450`, `:5066`].
- Element access: `typedSelect` tries members first, then `tryNamedTupleSelection`
  (`T/Typer.scala:860-875`): `qual.tpe.widenDealias.namedTupleElementTypes(true)` gives
  `[(name, type)]` (`C/TypeUtils.scala:128-146`: the names tuple's elements as `ConstantType`
  strings, zipped with the values tuple's); for index `i` it types
  `untpd.Apply(Select(qual, apply), Literal(i))`, i.e. the library's `extension (x: NamedTuple[N,V])
  inline def apply(n: Int): Elem[V, n]` on the values [V]. `isNamedTupleType`, `stripNamedTuple`
  (`:159-170`) are the two predicates every later phase uses.
- Patterns `case (a = x, b = y)`: `desugar.tuple` in Pattern mode calls `adaptPatternArgs`
  (`:1893-1925`): named patterns are reordered by the scrutinee's `namedTupleElementTypes` into a
  positional `TupleN(…)` pattern with wildcards for missing names; `pt.stripNamedTuple` gives the
  element types (`:3768`, `:2928-2960`); `PatternMatcher.dropNamedTuple` casts the scrutinee to the
  values tuple (`transform/PatternMatcher.scala:118-124, 382-404`) [V].
- Conversions: `adapt` inserts `.toTuple` when a named tuple meets a `Tuple` expected type
  (`T/Typer.scala:5066-5067`); `.toTuple`, `.names`, `withNames`, `++` (`Concat`), `From`, `Names`,
  `DropNames` are library extensions and match types; `From` is special-cased in
  `C/Definitions.scala:1468, 1515` (reduced by the compiler like `Tuple.Concat`) [V].
- Erasure: the opaque alias erases as its bound `V`, so `(a: Int, b: String)` erases to `Tuple2`
  [I from `C/TypeErasure` treating opaque aliases by their bounds; mirrors our `expand_opaque`].
- Printing: `RefinedPrinter.appliedText` (`P/RefinedPrinter.scala:276-283`) asks
  `namedTupleElementTypesUpTo(200, false, normalize = false)` on every `AppliedType` and prints
  `(a : Int, b : String)` (`toTextNamedTuple`, `:239-242`, name, space, colon, space) [V]. That is
  the `val res0: (a : Int, b : String) = (42, "Hello")` of the issue; the value is `Tuple2.toString`.

## 2. Ours

- Refusals: `Y/tuple_xxl.rs:18` (literal with `NamedArg` elements) and `:65` (type with
  `NamedArg` elements), reached from `Y/dispatch.rs:32` and `type_trees.rs` because
  `X/desugar/walk.rs:44` skips tuples with named arguments [V]. All eight tests of §3 but i16213
  stop at the first named type or literal [V dumps].
- Library: `scala.NamedTuple` is in the image: `type NT = scala.NamedTuple.NamedTuple[("a", "b"),
  (Int, String)]` types [V probe `tmp/nt-a.scala`]; `Y/implicits/mirror_sources.rs:151` already
  names `scala.NamedTuple$.NamedTuple`.
- Probes (`tmp/nt-{a,b,c,d}.scala`, `typer-dump` with absolute paths) [V]:
  (a) `val y: NT = (42, "Hello")` → "unsupported adaptation": the comparer does not use the
  opaque alias's lower bound `>: V` for an *applied* opaque type lambda from outside its owner
  (`S/subtype/meaning.rs:31-48`: `opaque_bounds` (`S/types/decode/defs.rs:110-117`) reads only a
  bare `TYPEBOUNDStpt`, a parameterised opaque's rhs is a `LAMBDAtpt`, so the fallback is the
  symbol's info; `S/subtype/applied.rs:40` then has the `Abstract` bounds) [V refusal, I cause];
  (b) `NamedTuple.build[("a", "b")]()[(Int, String)]((42, "Hello"))` → the same refusal at the
  argument (the inline body `x: V` as `NamedTuple[N, V]`, same cause);
  (c) `x.apply(0)` without an import → "member not found": the extension in object `NamedTuple`
  is not in the implicit scope of `NamedTuple.NamedTuple[…]` (`Y/implicits/scope.rs:74-78` adds the
  prefix's static module only for `parts.anchors`; `parts.rs:64,108` anchors an applied abstract
  tycon? — check);
  (d) with `import scala.NamedTuple.*`, `y.apply(0)` → "unsupported extension method among other
  definitions" (the refusal site in `Y/extension_calls.rs`).
- Reuse: `small_tuples.rs:46-83` (`tuple_elements`, `*:` chains and `TupleN`), `tuple_xxl.rs:62-85`
  (building type trees), `pattern_args.rs:35-95` (named patterns of case classes: `is_named_pattern`,
  `typed_pattern_arg`, the reordering with wildcards — dotty's `adaptPatternArgs` for case classes),
  `select.rs:55-95` (the fallback chain of `typed_select`, where `small_generic_tuple` sits),
  `S/types/show_user/applied.rs:39-47, 164-178` (tuple printing), `S/erasure/applied.rs:45`
  (`expand_opaque`), match-type reduction (ADR on match types; `Tuple.Concat` and friends) [V].
- Consequence: MR A must not depend on `NamedTuple.build`, the extension `apply`, or
  `V <: NamedTuple[N, V]` in the comparer. It casts instead (`type_cast`, as `tuple_xxl.rs:56`):
  the literal is the typed `TupleN(…)` cast to `NamedTuple[N, V]`, selection `x.a` is `x` cast to
  `V` then `._i` — what dotty's inlined `apply` reduces to [I]. The comparer's lower bound (a),
  the implicit scope (c) and the overloaded extension (d) move to MR C.

## 3. Tests (`ref/scala3/tests/run/`)

| test | needs | MR |
|---|---|---|
| i16213 | types now (plain XXL tuple pattern) [V dump] | — |
| named-tuples (`.check`) | types, literals, `.age` selection, `++`/`Concat`, patterns, `toTuple` | A, B, C |
| named-tuples-xxl | the same over 22 elements | A (XXL names), C |
| i23131 | `Some(name = a)` named pattern on a one-element named tuple | B |
| i23440 | named tuple in `List(…)`, `case List((a, b))`, lambdas `(a, b) =>` over named tuples | A, B, C |
| named-patmatch, named-patterns | `val (x = x, y = y) = …`, `case (x = x, y = y)`; named-patterns also case classes | B |
| named-tuple-ops, named-tuple-ordering, named-tuples-mirror, -strawman-2 | `Names`/`DropNames`/`From`, `Ordering`, `Mirror` of named tuples | C |

## 4. MRs

**A — types, literals, selection, printing (~150 lines, 45 min).**
1. `Y/tuple_xxl.rs:62` `typed_tuple_type`: for `NamedArg` elements build the names tuple
   (`Tree::SingletonTypeTree` of string literals, each with the tuple's span) and the values tuple
   as `Tree::Tuple` without names, then `AppliedTypeTree(TypedSplice(scala.NamedTuple$.NamedTuple),
   [names, values])` and `typed_type` it (dotty `:1880-1881`); refuse mixed named/unnamed (`:1829`).
   New file `Y/named_tuples.rs` (≤200 lines) holds this and the rest; `tuple_xxl.rs` only dispatches.
2. `Y/tuple_xxl.rs:15` `typed_tuple_xxl`: for named elements type the values tuple
   (`Tree::Tuple` without names, which the desugarer's `tuple` turns into `TupleN(…)` — call
   `typed` on a fresh `Tree::Tuple` node or build the `Apply(TypedSplice(scala.TupleN), values)` as
   `infix_tuples.rs:41-44` does) against `pt` stripped of its names, then `type_cast` it to
   `NamedTuple[names, widened values]` (names as string `Const` types in a `TupleN`/`*:` chain,
   `nested_pairs` above 22). dotty's `build` (`:1883-1891`) is a no-op cast after inlining [I].
3. `Y/named_tuples.rs`: `named_tuple_elements(t) -> Option<Vec<(String, TypeId)>>` — dealias to
   `Applied(NamedTuple, [names, values])` (full name as in `mirror_sources.rs:151`), names via
   `tuple_elements` as string `Const` types, values via `tuple_elements`; and `strip_named_tuple`.
4. `Y/select.rs:73`: before `small_generic_tuple`, if `named_tuple_elements(qual_tpe)` has `name`
   at index `i`, `type_cast` `qual` to the values tuple and select `_{i+1}` on it (`TupleN` only;
   over 22 `unsupported`); dotty types `qual.apply(i)` (`:860-875`), which inlines to the same.
5. `S/types/show_user/applied.rs:40`: before `tuple_elems`, a named tuple prints
   `(a : Int, b : String)` (`P/RefinedPrinter.scala:239-242`); the REPL uses `show_user`
   (`crates/repl/src/render.rs:78`) so `val res0: (a : Int, b : String) = (42, "Hello")` follows.
6. Tests: a `tests/repl` check with the issue's two lines; a `tests/programs` program with
   `named-tuples.scala`'s first 25 lines minus `++` and the match (type, ascription, `.age`,
   `println`). Risks: (i) `type_cast` to an applied opaque type in the lowering (`elim_opaque`
   must expand it; `S/erasure/applied.rs:45` does for the erasure) — test with `just run`, not
   only `typer-dump`; (ii) `val person2: (name: String, age: Int) = bob` is the same type on both
   sides, but `bob: (name: String, age: Int)` ascribed on a literal typed against that `pt` must
   not pass through the comparer's lower bound (type the literal with the stripped `pt`, cast to
   `pt` itself); (iii) the dumps of `named-tuples.scala` after MR A stop at `++`/`bob match`,
   expected.

**B — patterns (~120 lines).** `Y/pattern_args.rs` reordering generalised to named tuples
(dotty `adaptPatternArgs`, selector names from `named_tuple_elements`), `pt` stripped of names in
`typed_tuple_xxl`'s pattern branch and in `unapply.rs` for `TupleN.unapply`; the lowering casts the
scrutinee (`PatternMatcher.dropNamedTuple`); `X/desugar/pat_def.rs:149,184` for `val (x = a) = …`.
Tests: i23131, named-patmatch, named-patterns, i23440's matches.

**C — conversions and operators (~150 lines).** The comparer's lower bound of an applied opaque
type lambda (§2 (a); then `NamedTuple.build` and `V <: NamedTuple[N, V]` work as in dotty);
the extension `apply`/`toTuple`/`++` via the implicit scope of the type's prefix object (§2 (c),
(d)); `adapt.rs`: `.toTuple` when a named tuple meets a `Tuple` `pt` (dotty `:5066`);
`isNamedTuple_From` in the match-type reducer; `Concat`/`Names`/`DropNames` then come from the
library's match types; XXL names over 22. Tests: the rest of
named-tuples, named-tuple-ops, named-tuple-ordering, named-tuples-mirror (also needs
`mirror_sources.rs:64`), named-tuples-strawman-2.
