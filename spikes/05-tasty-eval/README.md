# Spike 05 — executing Scala 3.9 from TASTy (incomplete)

**Status: stopped early (session ended).** The evaluator was not built. What
exists: the test programs and their TASTy, a load-and-index driver with
startup timings, the IR and value types the evaluator was going to use, and
the TASTy shapes the lowering must handle, read from the real files. There are
no execution-speed numbers yet.

## What is here

```
programs/   hello.scala, fib.scala (fib 30), sum.scala (while loop summing 1..1e8 as Long),
            collatz.scala (longest Collatz chain below 1e6), features.scala (case class,
            Int match, object val, s"...", Long shift, Double, &&/!)
artifacts/  their .tasty, compiled with `scala-cli compile programs/X.scala -d <dir>` (Scala 3.9.0)
src/        program.rs  loads .tasty files (bytes leaked to 'static), parses them with
                        spike 01, indexes classes, finds entry points (`@main` defs and
                        `main` in an object)
            ir.rs       planned resolved and typed per-method IR (Ty, BinOp, UnOp, Expr, Lowered)
            value.rs    planned Value enum (Rc<str> strings, Rc<Obj> objects), Throw,
                        and a Java-compatible toString (Double formatting)
```

`cargo build --release && target/release/tasty-eval artifacts/fib\$package.tasty`
prints the entry point it found and the phase timings. Spike 01 is used
unchanged as a path dependency.

## Measurements so far (Apple M4 Pro, release)

| step | time |
|---|---|
| read one program's .tasty (≈ 0.5–1.5 KB) | 35–80 µs |
| `Tasty::parse` + `Ast::parse` | 24–29 µs |
| class index + entry-point search | < 1 µs |
| **process end to end** (`bench.py`, 20 runs, hello load-only) | **3.9 ms** mean (the harness's spawn floor is 3.6 ms) |

For user code, loading and decoding TASTy has no measurable cost. Startup is
set by process spawn and, later, by the stdlib: nothing in these programs
needs stdlib TASTy if `println`, `StringContext.s`, `math.abs` and the numeric
conversions are intrinsics. The ≤ 20 ms hello target therefore depends on how
much of the stdlib gets loaded and initialised (spike 01: ~13 ms for a full
eager decode of the stdlib, near zero if lazy). It does not depend on the
evaluator.

## How the needed features appear in TASTy (from `tasty-dump` of `artifacts/`)

- **References to locals and params are bare `TERMREFdirect addr` terms**, not
  `IDENT`. A same-file member is `TERMREFsymbol addr qual`, for example
  `<empty>.this.fib$package@5.fib@87`. Both resolve by address, with no name
  lookup. Reused subtrees show up as `SHAREDterm`, for example the `i` in `i += 1`.
- **Primitive operators are method selections with an owner:** `APPLY(SELECTin
  +[signed] n@90 scala.Int, 1)`. The owner (`scala.Int`, `scala.Long`,
  `java.lang.String`) together with the signed name picks the overload.
  Mixed types stay unconverted: `acc += i` with `acc: Long` and `i: Int` is
  `ASSIGN acc (APPLY (SELECTin + acc scala.Long) i)`, so the lowering applies
  binary numeric promotion itself. Implicit widenings the compiler inserts are
  explicit calls: `APPLY scala.Int.int2long i`. Literals are already constants
  of the target type (`0L`, `2d`).
- **Compound assignment is desugared** to `ASSIGN(x, x.+(1))`.
- **`while`** is `WHILE(cond, BLOCK …)`. `if` without `else` gets a `()` else
  branch.
- **`BLOCK` stores its result expression first**, then the statements.
  `var x = 0` is `VALDEF x: scala.Int mutable 0`.
- **`println`** is `APPLY(TERMREF println (Predef), arg)`, a plain module call
  that is easy to turn into an intrinsic.
- **`s"a${x}b"`** is `APPLY(SELECTin s (APPLY (SELECTin apply StringContext$)
  TYPED(REPEATED String "a" "b")), TYPED(REPEATED Any x))`. It is not
  expanded, so the lowering has to recognise this shape and emit a concat.
- **String `+`** is `SELECTin + "sum = " java.lang.String` with an argument of
  any type, which requires a Java-compatible `toString` for every value (for
  example Double `2.0`, `1.0E7`).
- **`@main def f()`** is a DEFDEF in the `X$package$` object with annotation
  `@new main.<init>()`. The generated `class f` with a static `main` wraps the
  call in a `TRY` over `CommandLineParser.ParseError`, so the evaluator should
  call the annotated def directly. `object O { def main(args) }` is a DEFDEF
  `main` in `CLASSDEF O$`.
- **Case class construction** is `APPLY(SELECTin apply(scala.Int,scala.Int)Point
  <Point> Point$, 3, -4)`, a cross-file call. The companion's synthetic
  `apply` body is `APPLY(SELECTin <init> (NEW Point), x, y)`. Fields are the
  template's `PARAM x caseaccessor` entries, and there is no separate field
  VALDEF. Field access is `SELECT x p`, with no owner, so the lowering needs
  the static type of `p`, taken from the local's declared tpt.
- **`match` on Int literals** is `MATCH sel (CASEDEF 0 (BLOCK "zero")) …
  (CASEDEF (IDENT _: Int) …)`, and each case body is wrapped in an empty
  BLOCK. `math.abs` is `SELECTin abs (SELECT package scala.math)
  scala.math.package$`. `!b` is `SELECT unary_! b`, with no owner and no
  APPLY.
- **Object vals** are VALDEFs in the object template (`VALDEF greeting "hi"`).
  They are accessed as `TERMREFsymbol greeting (THIS Features$)` and need
  lazy module initialisation.

## Planned design (not implemented)

TASTy → **lowering** → typed IR (`src/ir.rs`) → **closure compiler**
(`Box<dyn Fn(&mut Machine) -> Result<Value, Throw>>`), compiled per method on
its first call and cached in a `OnceCell` per method. Locals live in slots of
one shared `Vec<Value>` stack with a base index. A second backend walks the
same IR with a `match`, to measure how much closure compilation gains.

## Lessons so far for the real design

- **A typed IR between TASTy and the executable form is needed.** TASTy
  leaves out expression types: `SELECT x p`, `SELECT unary_!` and `==` on
  mixed types can only be resolved once the qualifier's type is known. That
  type has to be recomputed bottom-up from declared tpts and callee result
  types, the same thing dotty's unpickler does. The lowering has to: number
  locals, resolve addresses and names to slots, methods and fields, apply
  numeric promotion, recognise intrinsic shapes (`println`, `StringContext.s`,
  `int2long`, `math.*`), lower `MATCH` to tests, and pick primitive operations
  by static type. Closures should be generated from that IR, not straight
  from TASTy.
- **Symbol resolution needs a cross-file member table** keyed by class and
  signed name. Definitions carry plain names while use sites carry signed
  names (`apply(scala.Int,scala.Int)Point`), so matching overloads requires
  computing erased signatures on the definition side.
- **Startup cost is not in reading user TASTy** (≈ 60 µs per program). The
  budget goes to process spawn and to the stdlib, which is where laziness and
  caching matter.

## Next steps

Implement `lower/`, `closure/`, `walk.rs` and `machine.rs` as planned. Then
measure hello end to end, fib(30), sum 1e8 and collatz 1e6 against
`java -cp` and Python equivalents, and with and without superinstructions
(`local op const`).
