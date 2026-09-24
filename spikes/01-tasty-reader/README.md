# Spike 01 — TASTy reader in Rust

A dependency-light (`zip` only) Rust crate that decodes TASTy files: header, name
table (all name kinds), section table, Attributes, and a full structural decode of
the `ASTs` section into a preorder node arena, with named views for the tags an
interpreter cares about and a pretty printer that resolves names and addresses.

Format source: `TastyFormat.scala`, `TastyReader.scala`, `TastyHeaderUnpickler.scala`
at tag **3.9.0** of scala/scala3 (TASTy **28.9**, experimental 0). Scala 3.9.0 is
published, so no fallback was needed. `TreeUnpickler.scala`, `TastyUnpickler.scala`
and `NameBuffer.scala` (same tag) were read for details the grammar leaves out.

```
cargo build --release
target/release/tasty-dump [--names] artifacts/Main.tasty         # dump one file
target/release/tasty-dump <jar> [--repeat N] [--tags]              # parse all .tasty, time it
target/release/tasty-dump <jar> --dump scala/Predef.tasty          # dump one jar entry
cargo test --release                                               # sample files + Nat encoding
```

Layout: `cursor.rs` (Nat/LongInt/UTF-8), `file.rs` (header, sections, attributes),
`names.rs`, `tags.rs` (tag table), `tree.rs` (arena + category-driven parser),
`view.rs` (named nodes: PACKAGE, CLASSDEF/TYPEDEF, TEMPLATE, DEFDEF, VALDEF, PARAM,
IDENT, SELECT, APPLY, TYPEAPPLY, BLOCK, IF, MATCH, CASEDEF, INLINED, LAMBDA,
constants, SHARED; everything else stays generic), `print/`.
`sample/Sample.scala` is compiled into `artifacts/*.tasty`
(`scala-cli compile sample/Sample.scala -d <dir>`).

## Results

Apple M4 Pro, release build, best of 20, input already in memory unless stated.

| jar | .tasty files | uncompressed | parse errors | dangling refs | AST nodes |
|---|---|---|---|---|---|
| scala-library-3.9.0 (the stdlib) | 941 | 7.56 MB | 0 | 0 | 925 447 |
| scala3-compiler_3-3.9.0 | 1148 | 17.82 MB | 0 | 0 | 2 416 635 |
| scala3-compiler_3-3.3.1 (TASTy 28.3) | 950 | 13.78 MB | 0 | 0 | 1 925 949 |
| cats-core_native0.5_3-2.13.0 | 949 | 4.37 MB | 0 | 0 | 659 252 |

`scala3-library_3-3.9.0.jar` is an empty stub (0 .tasty); since 3.8 the stdlib is
`scala-library-3.9.0.jar`, compiled by Scala 3.
"Dangling refs" counts address payloads (SHARED*, *REFdirect, *REFsymbol, RETURN,
PARAMtype, RECthis) that do not land exactly on a decoded node start — a strong
consistency check of the structural decode.

stdlib timings:

| step | time |
|---|---|
| open jar + read central directory (4649 entries, `zip` crate) | 9.6 ms |
| open + inflate all 941 .tasty | 37.1 ms (≈ 27 ms of it inflate) |
| header + name table + section table, all files | **2.3 ms** |
| full decode incl. ASTs, all files | **12.7 ms** (595 MB/s of TASTy, ≈ 14 ns/node) |

Stdlib bytes by part: ASTs 2.32 MB, Comments (Scaladoc) 2.08 MB, Positions 2.07 MB,
header+names 1.08 MB. Only ASTs + names are needed to run code.

Lazy-index feasibility (50 ms startup budget): decoding is not the bottleneck —
headers+names for the whole stdlib cost 2.3 ms and a full AST decode 12.7 ms. Zip
handling is: the `zip` crate's central-directory read alone is ~10 ms and inflating
everything ~27 ms, because Positions/Comments are compressed together with the ASTs.
A lazy index therefore wants (a) a cheap own central-directory scan (the class→entry
map is just the zip entry names), inflating per class on demand, or better (b) an
uncompressed, mmap-able cache of ASTs+names only (≈ 3.4 MB for the stdlib), which
would make an eager full decode ~13 ms and a lazy one near zero.

Memory: the generic arena uses 32 B/node + 4 B/child edge, ≈ 33 MB for the stdlib
(14× the 2.3 MB ASTs payload). An interpreter should decode per method on demand, not
materialise the whole generic tree. 32 % of stdlib nodes are SHAREDtype
(298 350 of 925 447): types are heavily hash-consed by address.

## Format gotchas

- **Nat/LongInt are not LEB128**: big-endian base-128, and the *last* byte has the
  0x80 bit set (the stop digit). LongInt sign-extends from bit 6 of the first byte.
- **NameRefs are 0-based.** The grammar comment says "starting from 1"; `NameBuffer`
  assigns `NameRef(nameRefs.size)`. Section names are NameRefs too.
- Name entries: `UTF8` is `tag Nat bytes`; every other kind is `tag Length …`.
  `SIGNED`/`TARGETSIGNED` param sigs are *Int*s: negative = size of a type-param
  clause, positive = NameRef of the erased param type. Result type is a NameRef to a
  fully qualified (QUALIFIED) name, e.g. `<init>(scala.Double)sample.Circle`.
  Unique names: `separator num underlying?`, printed `underlying + separator + num`.
  DefaultGetter index is 0-based (`copy$default$1` has index 0).
- Category 2 payloads (tags 60–89) are LongInt-shaped on the wire; for `BYTE/SHORT/
  INT/LONG/FLOAT/DOUBLEconst` they must be read as *signed* LongInt (FLOAT/DOUBLE are
  raw IEEE bits), for the rest as Nat.
- Category 5 trees are not always "Nats then trees". Leading Nats come from
  `TastyFormat.numRefs` (1 for VALDEF, DEFDEF, TYPEDEF, TYPEPARAM, PARAM, RETURN,
  BIND, REFINEDtype, TERMREFin, TYPEREFin, SELECTin, HOLE; 2 for PARAMtype), **plus**
  `SELECTouter`'s `levels` Nat which `numRefs` omits. `POLYtype`/`METHODtype`/
  `TYPELAMBDAtype` are `result (type NameRef)* modifier*` — Nats interleaved with
  trees; modifiers (tags 6–59) are recognised by tag, and a parameter type can itself
  be a cat-1 constant (literal type), so the test is "is the *next* item a name".
- Optional children are disambiguated by tag only: VALDEF `rhs?` vs trailing
  modifiers (modifier set includes ANNOTATION, PRIVATEqualified, PROTECTEDqualified),
  INLINED `call?` vs following VALDEF/DEFDEF bindings, CASEDEF `guard?`, MATCH/IF
  leading `IMPLICIT`/`INLINE`/`SUBMATCH` pseudo-children.
- `BLOCK` stores `expr` *first*, then the statements. DEFDEF parameter clauses are
  separated by `SPLITCLAUSE`; `EMPTYCLAUSE` is `()`.
- Addresses (ASTRef) are byte offsets relative to the start of the ASTs section
  payload; the target always precedes the reference, so a preorder arena sorted by
  address resolves them by binary search, no map needed.
- Header: 4-byte magic, three Nats, UTF-8 tooling string (`"Scala 3.9.0"`; the
  stdlib says `"Scala 3.9.0-bin-nonbootstrapped"`), 16-byte UUID. In every file
  looked at, byte 0 and byte 8 of the UUID are 0x00.
- Attributes section: stdlib files carry `SCALA2STANDARDLIBRARY`, `EXPLICITNULLS`,
  `CAPTURECHECKED`, `WITHPUREFUNS` and `SOURCEFILE`; the sample only `SOURCEFILE`.
- 3.9.0's `MinorVersion` is 9 (the architecture doc quotes 10 for `main`).

## Observations relevant to interpretation

Excerpts are from `tasty-dump artifacts/<file>.tasty`. Notation: `name@N` = the
definition at AST address N (local symbol, resolved), `a.b.c` = TERMREF/TYPEREF
path (non-local symbol, by name), `SELECTin name` = selection with signed name and
owner class.

**Ordinary `inline def` calls are not expanded; `transparent inline` calls are.**
`twice(inc(20))` (plain `inline def`) is a normal call; `pick(true)` (transparent)
is already reduced to an `INLINED` node whose `expr` is the constant `1` and whose
`call` is trimmed to the enclosing top-level class:

```
APPLY
  scala.Predef.println
  APPLY
    sample.Sample$package.twice
    APPLY
      SELECTin apply inc@585 scala.Function1
      20
APPLY
  scala.Predef.println
  INLINED 1 Sample$package$
```

The inline def itself keeps its body and flags (`INLINE`, `TRANSPARENT` on the def,
`INLINE` on inline params) so an interpreter can expand it:

```
DEFDEF twice@371 inline
  PARAM x@374: Int inline
  Int
  TYPED
    APPLY
      SELECTin + x@374 scala.Int
      x@374
```

In the stdlib, `INLINED` appears 140 times (e.g. Predef's `ensuring` bodies contain
the expanded transparent `assert`); references to inline params inside the
expansion are wrapped as call-less `INLINED cond@2505` nodes. 4 `MACRO` defs,
78 `QUOTE`, 538 `SPLICE` nodes exist in the stdlib (scala.quoted / compiletime).

**Pattern matching is unlowered**: `MATCH` with `CASEDEF(pat, rhs, guard?)`; patterns
are `UNAPPLY` (extractor = `SELECTin unapply` on the companion, plus the scrutinee
type), `BIND name type pat`, typed wildcards `IDENT _: T`, `TYPED`. Case bodies
appear wrapped in a `BLOCK` with no statements.

```
CASEDEF
  TYPED
    UNAPPLY
      SELECTin unapply sample.Circle sample.Circle$
      sample.Circle
      BIND r@132
        scala.Double
        IDENT _: scala.Double
    sample.Circle
  BLOCK "big circle"
  APPLY
    SELECTin > r@132 scala.Double
    10
```

Case classes: all synthetic members (`hashCode`, `equals`, `toString`, `canEqual`,
`productArity`, `productElement`, `copy`, `copy$default$1`, `_1`, companion
`apply`/`unapply`/`fromProduct`) are present as ordinary DEFDEFs with bodies.

**For-comprehensions are desugared** into `flatMap`/`withFilter`/`map` with
lambdas; a **lambda** is `BLOCK(LAMBDA $anonfun@N, DEFDEF $anonfun@N …)` — closure
over a local method, no lambda lifting:

```
APPLY
  TYPEAPPLY
    SELECTin flatMap shapes@456 scala.collection.immutable.List
    scala.Tuple2[java.lang.String, scala.Double]
  BLOCK
    LAMBDA $anonfun@687
    DEFDEF $anonfun@687 synthetic artifact
      PARAM s@691: sample.Shape
      ...
          SELECTin withFilter
```

**Givens / using**: `given Show[Int] with …` is an ordinary object
(`VALDEF given_Show_Int … object given` + `CLASSDEF given_Show_Int$`); `using`
params are `PARAM s: Show[A] given` in their own clause. At the call
site implicit resolution is done: the given instance is an explicit argument in a
second `APPLY`. Implicit conversions are explicit too (`scala.Int.int2double k`),
and so is `Numeric.DoubleIsFractional` for `.sum`.

```
APPLY
  APPLY
    TYPEAPPLY sample.Sample$package.describe scala.Int
    42
  sample.given_Show_Int
```

**Extension methods** are plain methods on the package object with an `EXTENSION`
flag and a `SPLITCLAUSE` between the receiver and the other params; calls are
`sample.Sample$package.scaled(s)(k)`. **String interpolation** is not expanded:
`StringContext.apply(REPEATED "pairs=" " total=" "%").s(REPEATED …)`.

**Semantic information present**:
- Local symbols are referenced by address (`TERMREFdirect`, `TYPEREFdirect`,
  `TERMREFsymbol qual`), so locals/params/local classes resolve without lookup.
- Non-local members are referenced by *name + prefix type* (`TERMREF name qual`,
  `TYPEREF`), and selections as `SELECTin signedName qual owner` (50 392 in the
  stdlib) or `SELECT signedName qual` (33 227). Overloads are already resolved:
  the name is a SIGNED name carrying the erased signature, e.g.
  `mix(scala.Int,scala.Int)scala.Int`. An interpreter still needs a cross-file
  symbol table (class → members by name+signature) to bind these.
- Types are *not* stored on every node. `IDENT` carries its type, definitions carry
  their declared types (`PARAM r: scala.Double`, DEFDEF result type), `TYPEAPPLY`
  carries type args, `TYPED` ascriptions, `REPEATED` element types, `SELECTin` the
  owner. The type of `APPLY`/`SELECT` is recomputed by dotty's unpickler from symbol
  signatures; types are pre-erasure.
- Stdlib TASTy is capture-checked: types like `scala.ImpureFunction1[A, Boolean]`
  and `ANNOTATEDtpt(Any, new scala.annotation.retainsByName…)` appear in signatures,
  which erasure in the interpreter has to see through.
- Constructors are `DEFDEF <init>`; the class template's first statement is the
  primary constructor, parent constructor calls are `APPLY(SELECTin <init> (NEW T))`
  trees in the TEMPLATE parents; objects are `VALDEF X` + `CLASSDEF X$` + a
  synthetic `writeReplace`.
