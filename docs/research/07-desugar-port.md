# 07 — Porting dotty's desugaring to Rust

Date: 2026-09-29. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths are relative to `ref/dotty` (tag 3.9.0); `A/` = `compiler/src/dotty/tools/dotc/ast/`, `T/` =
`.../dotc/typer/`, `D` = `A/Desugar.scala` (2624 lines), `E` = `A/DesugarEnums.scala` (311).
Issue #27; ROADMAP phase 3, step 1, second half. Follows [research 06](06-parser-port.md).

**Recommendation in one paragraph.** dotty has no desugaring pass: Namer calls `desugar.defTree/
packageDef/extMethods` while entering symbols and Typer calls a dozen `desugar.*` helpers while
typing. By weight about three quarters of `D`+`E` is tree-to-tree and needs from the Context only the
*kind* of the owner (package, class with flags and companion, term), the enum's type parameters,
the source version, and a fresh-name counter [V]. Port that part as one pass over the parsed tree,
running in Namer's order (statement lists first, class bodies in source order), into
`crates/syntax/src/desugar/`; leave the parts that read an expected type or a symbol (lambdas,
`{ case … }` blocks, named tuple patterns, assignments, `+=`, `@main`) to the future typer. Test
it differentially with the prototype oracle `tmp/desugar/oracle/Oracle.scala`, which replays
dotty's own `desugar` calls on parsed trees and prints S-expressions: all 5893 corpus files, no
crash, about 2 s per corpus plus 2.5 s JVM start [V].

## 1. Entry points and their callers

- **Namer** (`T/Namer.scala`): `expand` :392-404 → `desugar.defTree` (D:1740), `packageDef`
  (D:1954), `extMethods` (D:1266); the result hangs on the original as `ExpandedTree` and is read by
  `indexExpanded` :439-467 and `Typer.typedStats` (T/Typer.scala:4060-4111, `removeAttachment`
  :4071, :4101). `index(stats)` :577-795 expands a whole statement list (`expandTopLevel` :779
  keeps language imports in the context), then **merges a synthetic companion into an explicit
  object of the same name** (`mergeCompanionDefs` :646-685: synthetic body minus `toString` first,
  explicit body after, `derives` clause carried over), links companions :693-724. Class bodies are
  expanded when the class completes: `completeConstructor` :1626-1658 does `index(constr);
  index(rest)`. `invalidateIfClashingSynthetic` :976-1014 later drops synthetic `apply/unapply/
  copy` that a user member matches (`isRetractableCaseClassMethodName` D:89) [V].
- **`defTree`** dispatches to `valDef` D:222-241 (setter `x_=` for `var`s of classes, traits and
  package objects), `classDef` D:667-1142, `typeDef` D:656 (context bounds on type members),
  `defDef` D:266 = `elimContextBounds` D:331-380 + `addDefaultGetters` D:382-442, `moduleDef`
  D:1174-1206 (+ `packageModuleDef` D:1154, `Checking.checkWellFormedModule`), `patDef` D:1444 →
  `makePatDef` D:1505-1679 with `getVariables` D:2561-2623, after `checkModifiers` D:1724 (opaque,
  tracked) [V].
- **Enums** (`E`): `enumClass`/`enumCompanion` = `ctx.owner` or its `linkedClass` :43-51,
  `addEnumFlags` :90, `expandEnumModule`/`expandSimpleEnumCase` :283-310, `nextOrdinal` :252-267
  (a counter kept as attachment `EnumCaseCount` on `ctx.tree`, the module class def), lookup
  members `$values`/`values`/`valueOf`/`$new`/`fromOrdinal` :115-192 emitted with the last case,
  `interpolatedEnumParent` :66-83 (needs the type parameters' *bounds*), `typeParamIsReferenced`
  :201-245 [V].
- **Typer** (`T/Typer.scala`): `typedUnadapted` :3833-3923 falls back to `desugar(tree, pt)`
  (D:2099-2451) for `SymbolLit`, `InterpolatedString` D:2406-2420, `PostfixOp`, `PrefixOp`,
  `ForDo`/`ForYield` (`makeFor` D:2178-2388), `PatDef`, `ExtMethods`, `Parens`; `typedBlock` :3877
  → `block` D:1758; `typedInfixOp` :3729-3759 → `binop` D:1771 (types: `AppliedTypeTree(op, l,
  r)`, patterns: `Apply(op, l, r)`, `throws` D:1801); `typedTuple` :3762 → `tuple` D:1842-1889;
  `typedIdent` :637 and `typedAppliedTypeTree` :2777 → `patternVar` D:1682; `typedFunctionValue`
  :1857-2090 → `makeTupledFunction` D:2031 (:2020), `patternsToParams` D:2066 (:2032),
  `makeCaseLambda` D:2004 (:2037, and `typedMatch` :2196 with the arity from the prototype),
  `makeClosure` D:1984 (:2086, :2120, :2128); `typedPolyFunction` :2092 → `normalizePolyFunction`,
  `makePolyFunctionType` D:1358, `elimContextBounds`; `typedTry` :2578 → `makeTryCase` D:2464;
  `typedRefinedTypeTree` :2706 → `refinedTypeToClass` D:2507; anonymous given names :2947 →
  `inventGivenName` D:1380-1428; `typedPackageDef` :3634-3659 → `packageObjectName`; `adapt` :4024
  → `makeContextualFunction` D:2079. `T/Applications.scala`: `adaptPatternArgs`/
  `checkWellFormedTupleElems` :226-249 (named tuple patterns), `typedOpAssign` :1514-1535
  (`x += y` → `x = x + y` only after `+=` fails to type), `makeTupledFunction` :2614; `typedAssign`
  :1437-1549 (`f(i) = v` → `update`, `x = v` → setter `x_=`); `T/QuotesAndSplices.scala:259`
  → `quotedPattern`; `core/SymbolLoaders.scala:164-169` → `packageDef` for sources on the class
  path; `MainProxies` (`@main`) runs after typing the top-level stats (:3652) [V].
- **Attachments** the expansions leave for later phases (D:33-79: `ForArtifact`, `PatternVar`,
  `CheckIrrefutable`, `UntupledParam`, `TrailingForMap`, `WasTypedInfix`, `ContextBoundParam`,
  `DerivingCompanion`; `KindOfApply` Using/InfixTuple, `SyntheticUnit`, `ExtendsSingletonMirror`,
  `DefinesEnumLookupMethods`) are read by Typer, PostTyper:655, CheckUnused, DropForMap [V]; in
  Rust they become node flags. Placeholder lambdas, `new C(a)`, givens, `A with B`, `GenCheckMode`
  are already the parser's (06 §3, Parsers:2513-2534, :3164-3167) [V].

## 2. What each part needs

- **Tree to tree** [V]: `block`, `binop` (spans of `left`/`op` decide `MultiLineInfix`; named
  args from `Assign`), prefix/postfix (mode decides `T*` vs `e.op`), `tuple` for unnamed elements
  (`scala.TupleN` up to 22, `()`), interpolated strings, symbol literals, `makeFor`, `patDef`/
  `makePatDef`/`getVariables`, `valDef`, `elimContextBounds`, `addDefaultGetters`, `typeDef`,
  `extMethod(s)`, `packageDef` (needs the file name), `normalizeName`/`inventGivenName`,
  `checkModifiers`, `makeSelector`, `makeCaseLambda` for arity 1, `patternVar`.
- **Owner kind and flags** [V]: `isSetterNeeded` (owner is class, trait, package object),
  `makePatDef`'s `PrivateLocal` on the temp (owner is class), implicit-class errors (owner is
  package), `classDef` (`ctx.owner eq ScalaPackageClass` only for the library), `moduleDef`,
  enums (owner's `linkedClass`, its `typeParams` with variance, the counter on the class tree).
  Also: source version (`enablesBetterFors` ≥ 3.8, `Given` vs `Implicit` evidence ≥ 3.6,
  `migrateTo3`, `shouldBehaveAsScala2`), language features (`modularity` → `WitnessNamesAnnot`,
  `saferExceptions`), `ctx.compilationUnit.isJava`, fresh names per unit (`$N$`, `_$N`,
  `evidence$N` is per method, `contextual$N`, `x$N` is positional).
- **Typing information** [V]: `makeClosure` (inferred parameter types), `makeTupledFunction`/
  `patternsToParams`/n-ary `makeCaseLambda` (arity and product-ness of the expected type),
  `tuple`/`adaptPatternArgs` for named tuple patterns, `makeTryCase` (is the handler a
  `PartialFunction`), `refinedTypeToClass` (typed parent), `makeContextualFunction` (expected
  context function type), `typedAssign`/`typedOpAssign` (symbol of the lhs, failed typing),
  `interpolatedEnumParent` (ground bounds of the enum's type parameters), `derivedTree` of
  `DerivedTypeTree`s (D:101-178, types of watched parameters), `invalidateIfClashingSynthetic`,
  `MainProxies`, `quotedPattern`.
- **Our frontend**: everything in the first two groups runs as a pass before typing, in the order
  the oracle's `Expander` fixes (§4); the third group is ported later, inside the typer, tree for
  tree like dotty.

## 3. Normative rules and where dotty differs

- **For comprehensions**: spec `docs/_spec/06-expressions.md:718-782` gives the Scala 2 scheme
  (four methods, `case p <- e` → `withFilter`, `p = e` → tupled generator via `map`). dotty
  implements it in `makeFor` with, since 3.8 (`enablesBetterFors`), leading aliases as block
  `val`s, aliases before a generator without a tuple, and the trailing `map(x => x)` elided
  (`TrailingForMap`), rules 1, 6, 7, 9 in the comment D:2101-2177; irrefutable patterns are
  *checked* (`IrrefutableGenFrom`) rather than filtered since 3.2 (`reference/changed-features/
  pattern-bindings.md`); patterns of more than 21 variables get `@unchecked` [V]. The spec text is
  pre-3.8; `reference/other-new-features/better-fors.md` describes the change.
- **Pattern definitions**: spec `04-basic-definitions.md:78-130` (one temp + `_i` selectors,
  single variable, no variables, comma lists). dotty `makePatDef` adds: `val (a, b) = (1, 2)` →
  two `val`s (`SimpleTuple`), `Bind(tmp, pat)` when the rhs is a tuple-shaped expression
  (`MatchableTuple`), `lazy` → `def`s, `_` → fresh `_$N`, temp is `private[this] synthetic`, a
  declared type wraps the pattern in `Typed`, `given` patterns only in `for` [V].
- **Variables**: spec `04:142-179` setters for template members; dotty D:208-213 also for all
  trait and package-object members, not for private class members [V].
- **Case classes**: spec `05-classes-and-objects.md:596-653`. dotty D:904-1047: `unapply(x$1) =
  x$1` (name-based; `Option`/`Some` only under `shouldBehaveAsScala2`), `_1.._N` selectors unless
  user-defined, companion `toString` override, `apply` keeps `HasDefault`, `unapplySeq` for a
  repeated element, no `apply` if abstract, no `copy` if abstract or repeated, parents `Product
  with Serializable`, missing parameter list is an error. `equals`/`hashCode`/`toString`/
  `productElement` come from `transform/SyntheticMembers` after typing [V].
- **Objects**: spec `05:842-875` sketches `lazy val m = new …`; dotty D:1165-1205 emits
  `<module> val m: m$ = new m$` and `final class m$ { this: m.type => }` [V].
- **Enums**: spec `05:967-1160` and `reference/enums/desugarEnums.md` state the same nine rules
  and match `E` [V]. Beyond them: `fromOrdinal` (always), `ordinal` literal per class case,
  `import E.{cases}` (rule 1), `Final` on cases, `Sealed | Abstract` on the class; `values`/
  `valueOf` only when *no* class case exists (spec §1152; the reference page's "one or more
  singleton cases" is stale) [V].
- **Context bounds**: spec `07-implicits.md:323-372` (evidence appended to one implicit section)
  vs `reference/contextual/context-bounds.md:68-130`: three placement rules, `as` names, `using`
  clause from 3.6, `evidence$N` per method = `elimContextBounds`/`addEvidenceParams` D:513-607;
  type members → deferred givens (§163-183, `typeDef`); poly functions → context function types
  (D:368-379). `WitnessNamesAnnot` only under experimental `modularity` [V].
- **Default arguments**: spec `04:578-587` names no scheme; dotty `f$default$N`, N counting value
  parameters over all clauses from 1, each getter keeping the type parameters and the preceding
  value clauses without defaults (D:382-442) [V].
- **Extension methods**: `reference/contextual/extension-methods.md`, `right-associative-
  extension-methods.md:37-59` (clause swap) = `extMethod` D:1208-1263, flag `ExtensionMethod` [V].
- **Givens**: `reference/contextual/givens.md:55` names `given_Ord_Int` = `inventGivenName`
  D:1380-1428 (`_type` for term refs, `Function`, tuples, infix types) [V].
- **Top-level definitions**: `reference/other-new-features/toplevel-definitions.md` = `packageDef`
  D:1927-1976; wrapped also: objects named like a wrapped *type* (the page says opaque aliases) [V].
- **Operators**: spec `06:493-578`: dotty `binop` swaps right-associative operands and lifts impure
  arguments only at typing (Typer:3741-3752), `l op (a, b)` passes two arguments, `l op (x = e)` a
  named argument; assignment operators are typer fallbacks [V].
- **Tuples**: spec `06:386-398`; dotty `scala.TupleN(…)` ≤ 22, `TupleXXL` at typing, named
  tuples → `NamedTuple.build[(names)]()(tuple)` (`reference/other-new-features/named-tuples.md`) [V].
- **Anonymous and pattern-matching functions**: spec `06:922-995`, `08-pattern-matching.md:
  577-644`, `reference/other-new-features/parameter-untupling-spec.md` (spec says `def p_i`,
  dotty makes `val`s D:2056); dotty's form is `Block(DefDef($anonfun), Closure)` [V].
- **Interpolated strings**: `_root_.scala.StringContext(parts).id(args)` (rooted on purpose,
  D:2419); `${}` → `()` [V].

## 4. Differential testing

- **Oracle** (`tmp/desugar/oracle/Oracle.scala`, a scala-cli program on `scala3-compiler` 3.9.0,
  same `--list` protocol and S-expression format as `scripts/syntax-oracle`) [V]: one initialized
  `ContextBase` (`Yusejavacp`, `defn` needed for `ref(defn.X)`), per file parse, then `Expander`
  (an `UntypedTreeMap`) replays dotty's calls: `transformStats` = `Namer.index(stats)` (`defTree`/
  `packageDef`/`extMethods`, companion merge, then recursion into the *expansions*); owners are
  `EmptyPackageClass`, a `newCompleteClassSymbol` per class def (module classes linked to their
  companion with `registerCompanion`, enum type parameters entered with their variance and empty
  bounds), a term symbol for `def`/`val` bodies; `ctx.tree` is the class def (enum ordinals);
  `Mode.Type`/`Mode.Pattern` follow the typer's `typed*` methods; `desugar(tree)`, `binop`,
  `tuple` (unnamed), `block` are applied where `typedUnadapted` would. Printed additions:
  `TypedSplice` as the referenced symbol's full name (`t:scala.Tuple2`, `T:scala.Product`),
  `DerivedTypeTree ~name` of the watched definition, attachments as `@infix @patternvar @check=…
  @apply=Using`. Runner `tmp/desugar/run.py pos|neg`.
- **Result** [V]: `tests/pos` 3967 files, 0 crashes, 0 desugar errors (one file has parser
  errors), 4.7 s wall of which 2.5 s JVM start and context init; `tests/neg` 1926 files, 0 crashes,
  281 files with errors (about 130 from desugar, the expected kinds: modifiers on enums/cases/
  opaque/tracked, case class without parameter list, empty enum, implicit class arity, given
  patterns in `val`, right-associative extension forward references), 3.9 s. Output 33 + 7.8 MB.
- **Coverage** (files whose expansion shows the feature, pos/neg): infix `binop` 752/275, case
  class synthetics 561/224, default getters 571/203, `$package` wrapper 958/641, extension methods
  230/71, anonymous given names 204/86, context bounds 147/57, `for` 142/25, pattern definitions
  159/44, setters 135/61, enums 105/73, tuples 246/74, interpolated strings 77/47, prefix ops 64/10;
  no synthetic reference at all in 1832/852 files [V].
- **Hand-written checks** (`tmp/desugar/samples/`) come out as dotty documents them: a case
  class merged into its explicit companion (`apply`, `unapply`, `<init>$default$2`, then the
  user's members), `copy$default$N`, `_1`/`_2`, `val (a, b) = (1, 2)` → two vals, `lazy val
  (u, _)` → `$1$` + `def u`, `for` with alias and guard, enums with `$new`/`$values`/`valueOf`/
  `fromOrdinal` and `Leaf extends Tree[Nothing]`, `given_Ordering_String`, the `+:` clause swap,
  package object, implicit class wrapper [V].
- **Not covered by this oracle**: the typer-driven group of §2. Alternatives rejected: running
  dotc to `typer` and dumping `ExpandedTree` (needs typing of every file with its class path,
  minutes instead of seconds, and `typedStats` removes the attachment); `-Vprint:typer` (text,
  no synthetic detail; and `-Vprint:parser` prints before desugaring) [V].
- **Names are order-dependent**: `UniqueName.fresh` counts per unit and prefix
  (`util/FreshNameCreator.scala`), so `$N$`, `_$N` numbers follow expansion order. dotc expands
  class bodies at completion, on demand; the oracle in source order. The oracle's order is the
  contract for the Rust pass; it differs from a dotc run only in those numbers [I].

## 5. Order of work

1. Infrastructure (~600 lines): a tree copier with dotty's span rules (`Positioned.withSpan`
   clones source-derived nodes, mutates synthetic ones, `envelope` fills children — the parser
   port has `envelope`), a `Mode` bit set, per-unit fresh names and the derived name kinds
   (`$N$`, `_$N`, `x$N`, `evidence$N`, `f$default$N`, `given_…`, `<file>$package`, `$anonfun`,
   `$new`, `$values`), a `SymRef(full name)` node standing for `TypedSplice(ref(defn.X))`,
   attachments as flags, `Thicket` flattening, the `desugar` mode of `syntax-dump`, and
   `scripts/desugar-oracle` + `scripts/desugar-tests.py` (from `syntax-tests.py`) with Justfile
   recipes `desugar-oracle`/`desugar-report`.
2. Expressions (~700): `block`, `binop`, prefix/postfix, `tuple`, interpolated strings, symbol
   literals, `makeSelector`, `makeCaseLambda`(1), `patternVar`, `makeFor` + `makePatDef` +
   `getVariables`.
3. Definitions without an owner model beyond the kind (~800): `valDef`, `patDef`,
   `elimContextBounds`, `addDefaultGetters`, `typeDef`, `extMethods`, `normalizeName`/
   `inventGivenName`, `checkModifiers`, `packageDef`.
4. Classes and modules (~900): `classDef` (parameter accessors, `DerivedTypeTree`s, case class
   members, companions, implicit wrappers, self type), `moduleDef`, companion merge and links.
5. Enums (~450): `DesugarEnums` with the ordinal counter on the module class node.
6. In the typer, later: `makeClosure`, `makeTupledFunction`, `patternsToParams`, named tuples,
   `makeTryCase`, `refinedTypeToClass`, `makeContextualFunction`, `makePolyFunctionType`,
   assignments, `@main`.

Modules under `crates/syntax/src/desugar/`, one per item above (`mod.rs` drives owners, modes and
statement lists; `names`, `copy`, `refs`, `exprs`, `for_expr`, `pat_def`, `variables`, `val_def`,
`def_def`, `defaults`, `class_def`, `class_params`, `case_class`, `companion`, `module_def`,
`ext_methods`, `package_def`, `enums`, `enum_lookups`, `modifiers`), 4.5–5.5k lines Rust [I].
Owner model: `Package { is_package_object }`, `Class { trait, module, enum, case, companion:
Option<NodeId>, type_params: [(Name, Variance)] }`, `Term`; per class node an enum-case counter;
per unit the source version, language features, file name, `is_java`. Stacked MRs: steps 1+2
(with the harness), 3, 4, 5, then neg-corpus parity of diagnostics.

## 6. Risks and open questions

- Span fidelity of synthetic trees rests on `Positioned` side effects (D uses `withSpan`,
  `startPos`, `union`, `Span(op.start, right.end)`); the arena copier must reproduce clone-vs-
  mutate semantics or the oracle's spans will differ [I].
- Companion merge and `invalidateIfClashingSynthetic` are Namer logic entangled with symbols; the
  pass does the merge syntactically (as the oracle does) and leaves clash invalidation to the
  typer [I].
- Enum parents for simple cases of parameterized enums need the bounds' *types* (`isGround`); a
  syntactic approximation on bound trees will diverge when bounds mention other parameters [I].
- Language imports (`modularity`, `saferExceptions`) change expansions; the oracle honours only
  `-language:` options, not `import language.experimental.*` [V]. Experimental features stay out:
  `modularity` proxies, capture checking (`CapturesAndResult`, `UseRef`), `erased`, `into`.
- The typer-side group (§5 step 6) cannot be tested until typed trees can be compared.
- Open: one pass over the whole file versus dotty's lazy expansion per class. A pass is simpler and
  fast (the oracle desugars a corpus in ~2 s in Scala); laziness only matters for fresh-name
  numbers and error order [I].
