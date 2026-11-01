# Research 43 — Compiling quotes outside splices (PickleQuotes) with NIR-identical output

`D/` = `ref/scala3/compiler/src/dotty/tools/dotc/`, `Y/` = `crates/typer/src/`, `T/` =
`crates/tasty/src/`, `L/` = `crates/lower/src/`. Follows ADR 0010, 0020, 0029; research 26 §3.2.

## 1 Problem

A macro implementation `def impl(x: Expr[Int])(using Quotes): Expr[T] = '{ f($x) }` is ordinary
code: dotty compiles the quote into a call `unpickleExprV2(<TASTy as Base64 strings>, types,
termHoles)` and Scala Native emits that call (`UnsafePackageCompat$.nir`: `as[QuoteUnpickler]`,
`unpickleExprV2(java.io.Serializable, Seq, Function3)`, hole closure `Lambda$1` →
`$anonfun$1(size$1, idx$1, contents$1, quotes$1)` with a `MatchError` default). Our typer only
types quotes inside a splice of an inline method (`Y/typer/source_splices.rs:16-30`,
`Mode::SPLICE` `Y/ctx.rs:30`); everything else is "the typer does not support Quote". Blocked:
sourcecode `Macros.scala` (16 quotes, 10 of them `'{new X(${Expr(s)})}`, `:266` with a type
parameter `T`), nativelib `UnsafePackageCompat.scala:125-133` (2 quotes), every library of #174.
We unpickle quotes (`Y/unpickle/quotes.rs`, `T/positions.rs`, `T/names.rs`, `T/tags.rs`,
`crates/macros/src/tasty_string.rs`) but have no writer: `T/` has no `write` at all.

Decoded `UnsafePackageCompat$`'s strings: header `28.8-0`, tooling `Scala 3.8.3`, UUID, a name
table (26 names, `SIGNED`, `QUALIFIED`, `OBJECTCLASS` kinds), section `ASTs` (54 bytes) and
section `Positions` (186 bytes: the line table of the *whole* source file, spans, and the path
`nativelib/src/main/scala-3/.../UnsafePackageCompat.scala` relative to `-sourceroot`). NIR
identity means every byte of this.

## 2 What dotty does

1. **Typer** `D/typer/QuotesAndSplices.scala:41-70`: `'{ e }` summons `Quotes` (`:48`), types
   `e` at level+1 as `Expr.quote(e)` and rewrites to `Quote(e, Nil).select(apply).appliedTo(quotes)`
   (`:66-69`); `${ e }` (`:78-105`) types `Expr.splice(e)` at level−1 into `Splice(e)`; at level
   0 it marks the enclosing inline method `Macro` (`:88-96`). Cancelled `'{ ${x} }` warns.
2. **Phase order** `D/Compiler.scala:50-56`: Pickler → Inlining → PostInlining → Staging →
   Splicing → PickleQuotes. Inline method bodies are left alone by all three
   (`D/staging/CrossStageSafety.scala:105`, `D/transform/Splicing.scala:92-94`,
   `D/transform/PickleQuotes.scala:99-100`); their quotes are pickled at the call site after
   inlining, their bodies are dropped by Erasure.
3. **Staging** (`D/transform/Staging.scala:82-85` → `CrossStageSafety.scala:59-163`): tracks
   levels (`D/staging/StagingLevel.scala`), cancels `'{ ${x} }`/`${ 'x }` (`:63-64,81-82`), heals
   the quote's body type and every `TypeTree`/type `Ident` in the body (`:65-79,125-133`):
   `D/staging/HealType.scala:46-60,87-105` replaces a level-0 type `T` by `t.Underlying` where `t`
   is the summoned given `Type[T]` (`inferImplicitArg`), recorded as a quote *tag*
   (`D/staging/QuoteTypeTags.scala:17-24`); `Type.of[T]` becomes a type quote with tags
   (`:90-104`); `Quote(body, tags)`.
4. **Splicing** (`Splicing.scala:85-131`): each level-1 `Splice` becomes
   `Hole(isTerm, idx, args, closure, tpe)` (`:168-183`): references to definitions *of the quote*
   inside the splice are replaced by `${x$1}` of fresh lambda parameters `x$1: Expr[T1]`
   (`:195-203,291-305`), captured types by `Type[X]` parameters (`:315-337`), calls and
   assignments of captured symbols by reflect-built trees (`:240-289`); nested quotes inside the
   splice are transformed again at level 0 (`:118,223-235`).
5. **PickleQuotes** (`PickleQuotes.scala:91-103`) on `Apply(Select(quote, apply), quotes)`:
   - `extractHolesContents` (`:105-151`) moves each hole's closure out, leaves
     `Inlined(EmptyTree, Nil, Hole(idx, args))` with a type avoiding non-static symbols (`:201-205`).
   - `encodeTypeArgs` (`:167-199`): tags `t_i` become `@SplicedType type T$i = {{ i }}`
     `TypeDef`s in a `Block` before the body; `t_i.Underlying` → `T$i`.
   - `pickle` (`:214-392`): a literal body → `ToExpr.IntToExpr.apply(lit)(quotes)` (`:269-289`),
     `null`/`()` → `reflect.Literal(Constant.Null())` (`:233-259`); a primitive type quote →
     `TypeRepr.typeConstructorOf(classOf[Int]).asType` (`:372-377`); else `pickleAsTasty`
     (`:303-360`): body wrapped in `Inlined(inlineCallTrace(ctx.owner, pos), Nil, body)`
     (`D/inlines/Inlines.scala:343-348`: `Ident(topLevelClass.typeRef)`), pickled to strings
     (`D/quoted/TastyString.scala:13-16`, split at 65535 chars), `types` = `null` or
     `SeqLiteral(tags)`, `termHoles` = `null` or `Lambda((idx, contents, quotes) => idx match
     { case i => BetaReduce(content_i(contents(j).asInstanceOf[..])).apply(quotes) })`
     (`:318-345`), then `quotes.asInstanceOf[QuoteUnpickler].unpickleExprV2[T](strs, types,
     termHoles)` (`:347-359`). Hole contents are transformed recursively (`:97`).
6. **Pickling** (`D/quoted/PickledQuotes.scala:219-235`): `TastyPickler(RootClass)`,
   `TreePickler.pickle(tree :: Nil)`, `compactify`, `PositionPickler.picklePositions` if the
   span exists, `assembleParts` (`D/core/tasty/TastyPickler.scala:27-74`: magic, `28 9 0`,
   `"Scala 3.9.0"`, UUID = `pjwHash64(names) ^ pjwHash64(ASTs)` / xor of the other sections,
   name table, sections).
7. **Later phases** see plain trees: the `Function3` lambda is lifted and adapted as any closure,
   `BetaReduce` removes the applied closures (`:231,312`).

## 3 The minimal TASTy pickler subset

- `TastyBuffer` (`ref/scala3` lacks dotty's `tasty/src`; `just dotty-sources` must add it):
  `writeByte/Nat/Int/LongInt/UncompressedLong`, `reserveAddr`/`fillAddr` (4-byte refs, right
  adjusted), `natSize`, `skipZeroes`. `TreeBuffer.compactify` (`TreeBuffer.scala:85-198`) must
  be ported as is: the iterated delta adjustment (`length / saved < 100`) decides the bytes.
- `NameBuffer` (`NameBuffer.scala:21-50,80-117`): insertion-ordered table; `SignedName` registers
  original, target, result and type-parameter names first; kinds needed: UTF8, QUALIFIED,
  UNIQUE (`x$1`), OBJECTCLASS, SIGNED/TARGETSIGNED, EXPANDED (private members). Signatures are
  erased parameter types with type-parameter counts (`T/names.rs:12-15` reads them; `T/erasure`
  computes them).
- `TreePickler.pickleType` (`TreePickler.scala:171-329`): `SHAREDtype` dedup (`:174-182`),
  `NamedType` (`:201-240`: package → `TERMREFpkg` full name; quote-local → `TERMREFdirect` +
  symbol address; external → `TERMREF`/`TYPEREF` name+signature+prefix, private →
  `TERMREFin`), `THIS`, `APPLIEDtype`, `TYPEBOUNDS`, `ANNOTATEDtype`, `AND/ORtype`, `BYNAMEtype`,
  `TYPELAMBDAtype`, `METHOD/POLYtype` with `richTypes` (hole types), `PARAMtype`, constants.
- `pickleTree` (`:425-830`): `Ident` as its `TermRef`, `Select`/`SELECTin` (`:466-505`),
  `APPLY`, `TYPEAPPLY`, `Literal`, `NEW`, `TYPED`, `NAMEDARG`, `ASSIGN`, `BLOCK`, `IF`, `LAMBDA`,
  `MATCH`/`CASEDEF`, `RETURN`, `WHILE`, `TRY`, `REPEATED`, `INLINED`, `BIND`, `ALTERNATIVE`,
  `UNAPPLY`, `VALDEF`/`DEFDEF`/`TYPEDEF`/`TEMPLATE` through `pickleDef` (`:369-405`) with
  `pickleModifiers`/`pickleFlags` (`:851-916`) and annotations (`:928-941`), `TypeTree` as its
  type, `HOLE` (`:804-815`), `THROW`. Not needed: `IMPORT`, `PACKAGE`, patterns beyond the above,
  `QUOTE`/`SPLICE` (never left in a pickled quote), best-effort branches.
- `PositionPickler` (`PositionPickler.scala:32-143`): line sizes of the file (`:47-58`), per
  tree `header(addrDelta, start?, end?, point?)` + deltas (`:62-78`) for trees whose span is
  not their children's envelope or that always need one — `DefTree`, `Hole`, lazy-field trees
  (`:88-105,107-139`); `SOURCE` + path relative to `-sourceroot` (`:80-84`,
  `D/util/SourceFile.scala:274-290`). Needs dotty-identical spans, points and synthetic flags
  of every tree in the body — the typer oracle checks text without spans (research 09).

## 4 Options

- **A. Faithful port, byte-identical** — Staging + Splicing + PickleQuotes as passes after
  `Inlining::run` inside `with_inlining` (`Y/lib.rs:140-148`; healing needs `Host::summon`,
  `Y/inlining/host.rs:28`), a TASTy writer in `T/`, a tree pickler in `Y/pickle/`. ~2.6k Rust.
  Risks: span fidelity (positions), fresh-name order (`x$1`, `idx$1`, `$anonfun$1` are drawn
  after inlining; PostInlining's names come before), signature computation for signed names.
- **B. A without positions first** — the `ASTs` section identical, the `Positions` section and
  therefore the string differ; `nir_compare.py` decodes `unpickleExprV2` strings and compares
  name table + `ASTs` structurally, reporting `quote-pos`. Same code, byte identity deferred;
  risk: a tolerated difference that hides real ones, and macro `Position`s off.
- **C. Own quote encoding** — `unpickleExprV2("scivolo:<n>", …)` into a frontend table. Not
  NIR-identical, breaks cached library images and `--dep` jars, still needs a serializer. Rejected.

**Recommendation: A, delivered through B's comparison as the intermediate gate** — the
structural compare lands first and stays as a diagnostic; the `Positions` step closes it.
Accepted differences from dotty: none; the tooling string is `Scala 3.9.0` (ADR 0046 oracle).
One ADR (next free number): quotes are pickled by an own TASTy writer, byte-identical to dotty's.

## 5 Steps

1. **Typer: staging levels** (~300 lines, 3 MRs). `Ctx` gets a level instead of `Mode::SPLICE`;
   `'{ e }` at any level summons `Quotes` and types `Expr.quote(e).apply(quotes)`; `${ e }` at
   level ≥ 1 types `Expr.splice(e)`; `Type.of[T]`; cancelled-quote/splice warnings
   (`QuotesAndSplices.scala:44-45,83-85`). Tests: `tests/typer` goldens of
   `tests/macros/lib/Macros.scala` (`scripts/typer-oracle`), sourcecode `Macros.scala` typed.
2. **TASTy writer** in `T/write/` (~500, 3 MRs): buffer, name table, `compactify`, header and
   `pjwHash64`. Tests: our reader round-trips; the name table and header of
   `UnsafePackageCompat$`'s decoded quotes re-encode byte-identically (fixture in
   `tests/pickle/`); `tmp/research-43/quote_names.py` shows the decoding.
3. **Tree pickler and simple quotes** (~900, 8 MRs): `pickleType`/`pickleTree` subset (§3) over
   `tpd`, `pickleDef` and modifiers; `PickleQuotes` for quotes whose holes have no captured
   references and no tags (`extractHolesContents`, `pickleAsValue`/`Literal`, `pickleAsTasty`,
   the `termHoles` lambda through `Y/inlining/beta.rs`); `nir_compare.py` quote-aware
   (`quote-pos`); `scivolo-aot` and `lib-source.py` get `--sourceroot` (as `scivolo-lower`
   `L/bin/scivolo-lower.rs:34`; scala-cli oracle `-sourceroot` as `Justfile:539`). Acceptance:
   `just lib-compile nativelib UnsafePackageCompat` `same` up to `quote-pos`;
   `just lib-source sourcecode` all files but `Macros.scala`'s `text`/`args` quotes `same`.
4. **Staging and Splicing** (~700, 6 MRs): `CrossStageSafety` + `HealType` with tags
   (`encodeTypeArgs`, `types = SeqLiteral(tags)`), `Splicing`'s captured terms/types (hole args,
   `x$1` parameters), captured applications/assignments, nested quotes in splices, type quotes,
   `taggedType`. Acceptance: `tests/macros/lib/Macros.scala` compiled with `--sourceroot .`
   equals `macros.jar`'s NIR up to `quote-pos` (33 quotes); sourcecode `Macros.scala` whole.
5. **Positions** (~250 + span fixes, 4 MRs): `PositionPickler` port; typer spans fixed where
   the pickled bytes differ. Acceptance: steps 3–4's comparisons `same`, `quote-pos` gone;
   `just lib-source sourcecode` 38/38 `same`, `tests/aot/sourcecode` passes on our NIR;
   `UnsafePackageCompat$` byte-identical to a 3.9.0 oracle build.

Order and dependencies: 1 → 2 → 3 → {4, 5} (4 and 5 independent); 2 needs dotty's `tasty/src`
in `ref/` (`scripts/dotty_sources.py`); 3 reuses `Y/inlining/beta.rs`, `L/phases/closures.rs`
and lambda lifting unchanged; 4 reuses `Host::summon` (ADR 0020) and the quote-type machinery of
`Y/unpickle/quotes.rs:98-132` in reverse. Out of scope: quote patterns in pickled form
(`QUOTEPATTERN`, matched by the typer per ADR 0029), `-Xcheck-macros`, best-effort TASTy.
