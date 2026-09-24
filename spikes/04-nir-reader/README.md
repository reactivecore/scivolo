# Spike 04: NIR reader

A Rust deserializer for Scala Native 0.5.12 NIR (compat 6, revision 11), plus the `nir-dump` CLI. The spec was
`nir/src/main/scala/scala/scalanative/nir/{serialization/*, Show, Mangle, Unmangle, Sig, ...}.scala` at tag
`v0.5.12`. About 2.3 kLOC, with `rustc-hash` and `zip` as the only dependencies.

```
cargo build --release
nir-dump [--dump | --parse | --index | --stats] [--mangled] [--externs] [--filter TEXT] [--repeat N] <file.nir | dir | jar>
```

- `--dump` is the default for a file or a directory. It prints definitions in SN `Show` syntax. Globals and
  signatures are demangled by default (`@Employee::hashCode(): int`). With `--mangled` the output uses SN's exact
  form (`@"M8EmployeeD8hashCodeiEO"`).
- `--parse` is the default for a jar. It fully decodes every definition of every `.nir` and reports how many
  succeeded, the byte count and the best-of-N time.
- `--index` reads only the prelude and the offsets table. That gives the definition names and each definition's
  byte offset, which is the lazy-loading path.
- `--stats` prints op histograms, raw-memory and extern usage, and `--externs` adds the list of extern symbols.

Samples: `samples/features.scala` covers try/catch/finally, a lazy val, an Int match, a closure, case-class `==` and
an `s""` interpolation. The NIR comes from `scala-cli --power compile --native --native-version 0.5.12 F.scala` and
ends up in `.scala-build/<proj>/classes/main/*.nir`.

## Correctness

- **Every `.nir` in every stdlib jar decodes, with no errors**: scalalib (4982 files), javalib (3503), nativelib
  (721), auxlib (86), clib (115) and posixlib (270). That is 9677 files and 27.9 MB.
- **Byte-exact against Scala Native.** `RefDump.scala` (JVM, `org.scala-native::nir:0.5.12`) runs SN's own
  `deserializeBinary` + `defn.show` over the NIR of `moderate.scala`. Every one of its 281 definitions matches our
  `--mangled` output line for line. We print 5 more definitions than SN does; see the gotchas.
- Malformed input returns an `Err` and never panics. Every read is bounds-checked, every interned reference is
  range-checked, sequence lengths are capped by the file size, and nesting is capped at 256.

## Numbers (M4 Pro, release build, best of 5, warm page cache)

| input | files | bytes (.nir) | unzip+read | full decode | index only | defns |
|---|---:|---:|---:|---:|---:|---:|
| moderate.scala classes | 35 | 54 964 | – | **0.53 ms** | – | 286 |
| scalalib 3.9.0+0.5.12 | 4982 | 13.7 MB | 74 ms | **107 ms** | **19.5 ms** | 61 551 |
| javalib 0.5.12 | 3503 | 10.2 MB | 57 ms | **84 ms** | **11.5 ms** | 38 369 |
| nativelib 0.5.12 | 721 | 2.7 MB | 17 ms | 19 ms | 3.3 ms | 10 963 |
| clib / posixlib / auxlib | 115 / 270 / 86 | 0.5 / 0.7 / 0.13 MB | 3 / 6 / 1 ms | 3.0 / 4.2 / 0.9 ms | 0.75 / 0.92 / 0.18 ms | |

Full decode runs at about 130 MB/s, and it interns everything into a pool that the files share. The strings,
globals and types of the whole scalalib come to 36 k, 62 k and 42 k entries. **Eagerly decoding the stdlib
(≈190 ms, plus ≈130 ms of inflate) does not fit the 50 ms budget.** A per-file index of all of scalalib + javalib
takes 31 ms, which is still too slow for every run, and the inflate cost comes on top of it. Some workable
designs:

- a prebuilt global symbol index (`Global` → jar entry + defn offset, like a CDS archive), with bodies decoded
  lazily on the first call (about 0.1 µs to 10 µs per method at the rates above);
- or storing the stdlib NIR uncompressed and mmapped, so no inflate is needed.

A moderate script's own NIR decodes in 0.5 ms.

## Format gotchas

- The layout is a 45-byte big-endian prelude (magic `.NIR`, compat, revision, 8 section starts, `hasEntryPoints`),
  followed by the sections **Offsets, Strings, Positions, Globals, Types, Defns, Vals, Insts**, in that order.
  Nothing records section lengths; each section ends where the next one starts.
- Strings, Positions, Globals, Types and Vals are **interned**. A reference is a ULEB offset *relative to the
  section start* (SN's `in(start){…}`), so a decoder needs a per-file offset→id cache. We use dense `Vec<u32>`
  caches, reused across files. Vals include every `Val.Local(id, ty)`, so locals are interned too.
- Each `Define` stores a ULEB offset into Insts, followed inline by its debug info: local names, then lexical
  scopes. The instruction lists themselves are not interned.
- Every instruction carries a position reference. **The scope id of a `Let` is serialized *after* the op and
  unwind** (in SN it is an implicit constructor argument), and `Label`/`Next.Unwind` parameters are inline
  `(local, type)` pairs, not `Val` references.
- The order of `Op.Store` fields is `(ty, ptr, value)`. The variable names in the serializer read `value, ptr`,
  but they are positional.
- Strings are **UTF-16 code units, each one encoded as a ULEB**. Lone surrogates can occur, and we decode them
  lossily. A real VM should store Java strings as UTF-16 or WTF-8. The lengths in mangled identifiers also count
  **UTF-16 units**, not bytes.
- Signatures are stored as **mangled strings** (`D3mapL15scala.Function1L…EO`). Method identity is the mangled
  sig. For demangling, see `demangle.rs` and `docs/contrib/mangling.md`.
- The offsets table ends with a `Global.None` entry (offset −1, SLEB). **The table can list the same global
  twice.** Examples are `Circle$.fromProduct`: the Product version and the covariant-return bridge get the same
  erased name `fromProduct(Product): Object`. SN reads the table into a `Map`, so the last one wins and SN drops 5
  of the 286 definitions in moderate. An interpreter has to replicate that behavior or pick by return type.
- `Type.Virtual`, `Int128` and `Val.Virtual` have tags, but the serializer refuses to write them. The
  `LinktimeCondition` tags (1 = simple, 2 = complex) are separate from `Val.LinktimeCondition` (22), which is
  never written.

## What Scala looks like in NIR (from `samples/features.scala` and `moderate.scala`; demangled)

NIR is erased, and in debug output it is unoptimized SSA with block parameters. Calls are always a two-step
`method` (a virtual lookup to a function pointer) followed by `call`, or a direct `call` on a `Val.Global`.

**try / catch / finally.** Inside the try, every `Let` carries `unwind %e to handler(%e)`. The handler is an
`is`/`as` chain, and the finally body is **duplicated** on every exit edge:
```
%5:
  1: %20 = call[(@StringOps$, @String) => int] %18 : ptr(%10, %16) unwind %19 : @java.lang.Throwable to %4(%19)
  jump %36
%4(%7 : @java.lang.Throwable):
  2: %21 = is[@java.lang.NumberFormatException] %7
  if %21 : bool then %22 else %23
...
%48:                                  // no case matched: finally, then rethrow
  8: %51 = call[...] @scala.Predef$::println(...)(%49, "parsed")
  throw %7 : @java.lang.Throwable
```
**Lazy val.** This is Scala 3's `LazyVals` protocol, fully lowered. There is a `size$lzy1: Object` field and the
`LazyVals$NullValue$` sentinel, and the initialization lives in a separate `size$lzyINIT1` (we did not dump it;
we expect the LazyVals CAS protocol there):
```
alwaysinline def @Config::size(): int {
  %3 <result> = fieldload[@java.lang.Object] %1 <this>, @Config::size$lzy1
  %8 = is[@java.lang.Integer] %3
  if %8 then %4 else %5
%4: %9 = unbox[@java.lang.Integer] %3 ...
%5: %14 = module @scala.runtime.LazyVals$NullValue$ ; %15 = ieq[@Object] %3, %14 ...
%11: %17 = method %1 <this>, private[Config] size$lzyINIT1(): java.lang.Object ...
```
**Closure.** `x => x + k` becomes an ordinary class, `features$package$$$Lambda$1`, whose constructor takes the
capture `k`. No invokedynamic remains:
```
def @features$package$::adder(int): scala.Function1 {
  %4 = classalloc @features$package$$$Lambda$1
  %5 = call[(@…Lambda$1, int) => unit] @features$package$$$Lambda$1::<init>(int) : ptr(%4, %1 <k>)
  ret %4
```
**List.map** is a plain virtual call on the erased signature,
`%17 = method %14 : @scala.collection.immutable.List, map(scala.Function1): scala.collection.immutable.List`
(List's own override), followed by a `call`. The `_ * 2` lambda is a `$$Lambda$N` class. `List(1, 2, 3)` boxes
its elements (`box[@java.lang.Integer] int 1`).
**Pattern match.** The `Int` match in `classify` is a `switch %n { case int 0 => … case int 1 => … case int 2 => … }`.
Type patterns (`describe`, `area`) turn into `is[...]`/`as[...]` chains and calls to `_1()`/`unapply`. Guards
become ordinary `if`s.
**Case class `equals`/`hashCode`** are generated methods. `hashCode` is a `var[int]` accumulator with `varload`/
`varstore` and calls to `scala.runtime.Statics::static mix(int, int)` (see `Employee.nir`). `Point.equals` is
`ieq[@java.lang.Object] this, x$0`, then `is[@Point]`, then `ieq[int]` on each field, then
`method …, canEqual(java.lang.Object): bool`.
**String interpolation `s"…"`** is already lowered to `classalloc @java.lang.StringBuilder`, a chain of
`StringBuilder::append(java.lang.String|java.lang.Object)` calls, and `toString()`. No `StringContext` remains.
**Modules.** Every `object` access is `module @X$`, a lazily initialized singleton. Static forwarders are
`@X::static m(...)` wrappers that call `module` and then the method.

## Raw memory, intrinsics and externs (unlowered NIR, `--stats`)

| jar | methods | with load/store/elem/stackalloc/ptr-conv | with stackalloc | calling an extern | files with raw-mem methods | extern decls | distinct externs called (`scalanative_*`) |
|---|---:|---:|---:|---:|---:|---:|---:|
| scalalib | 47 466 | **0 (0 %)** | 0 | 1 | 0 / 4982 | 0 | 1 (1) |
| javalib | 27 975 | **169 (0.6 %)** | 140 | 595 (2.1 %) | 55 / 3503 | 285 | 534 (268) |
| nativelib | 8 729 | **975 (11.2 %)** | 13 | 129 (1.5 %) | 107 / 721 | 171 | 57 (40) |

- **scalalib is pure object code.** It uses no pointer ops at all. The only extern it touches is
  `scalanative_atomic_memory_order_release`, for `LazyVals`/volatile.
- Op mix in scalalib: `call` 91 k, `method` 57 k, `module` 17 k, `fieldload`/`fieldstore` 11 k/7 k, `as` 7.5 k,
  `classalloc` 6.7 k, `varload`/`varstore` 6 k/3.6 k, `box`/`unbox` 1.5 k each, `dynmethod` 1. An interpreter can
  run the whole Scala stdlib with ~25 op kinds and **no pointer model**.
- **Caveat.** Unlowered NIR still expresses most `unsafe` use as **calls**: `Ptr.unary_!`, `Intrinsics.loadInt`,
  `fromRawPtr`, `CFuncPtr.apply`, and `sizeOf[T]` through `Tag`. The raw-op counts above are a lower bound on
  pointer use. javalib's real exposure is the 534 extern symbols it calls and the classes that wrap them. We did
  not measure the call-based intrinsics in this spike.
- **The C runtime surface javalib needs** (`scalanative_*`; the full list comes from `--stats --externs`): errno
  and constant getters (`scalanative_eintr`, `scalanative_af_inet`, `scalanative_clock_monotonic`, …, most of the
  268 are *constants* exposed as functions); zlib (`scalanative_deflate*`, `inflate*`, `crc32`, `adler32`); atomics
  (`scalanative_atomic_*`); GC hooks (`scalanative_GC_collect`, `GC_*_heapsize`, weak-reference slots,
  `GC_pthread_create`); continuations (`scalanative_continuation_*`); file mapping, epoll and fcntl wrappers;
  `scalanative_current_time_millis`. nativelib adds `scalanative_GC_alloc(_array)`, the thread info
  (`scalanative_currentThread`, `assignCurrentThread`), stack-overflow guards, libunwind wrappers
  (`scalanative_unwind_*`) and embedded resources. The other 266 javalib externs are plain libc/POSIX (`open`,
  `read`, `stat`, `getenv`, `pthread_*`, …).

## Assessment: an NIR interpreter vs a TASTy interpreter

**Reading NIR is easy.** The decoder took about 1 kLOC of Rust and a few hours. It is byte-exact with SN on the
first diff, and the only documentation needed was the Scala source. **Executing NIR is a "JVM-like"
interpreter**: SSA with block params, ~30 ops, explicit `unwind` edges, and explicit `module`, `classalloc`,
`box`, `is`/`as` and `method`/virtual dispatch. Everything TASTy would still leave to the interpreter has already
been done by the compiler before NIR is written:

- pattern matching is lowered to `is`/`as`/`switch`;
- lazy vals use the LazyVals protocol;
- closures are classes and `var` captures are boxed refs;
- traits are mixed in (forwarders are generated), and default arguments, inline, extension methods, given
  resolution, erasure and bridges are all resolved.

The remaining work:

1. **Semantics that live in `Lower`, not in NIR.** Null checks, array bounds checks, division by zero, class
   init, and `is`/`as` on arrays and boxes have to be re-derived from `tools/.../codegen/Lower.scala`.
2. **Method resolution.** `method obj, sig` needs vtables/itables built from `Class`/`Trait` defns
   (closed-world). Lazy loading means building a class's dispatch table the first time it is used.
3. **The native boundary is confined.** Scala stdlib code needs about 1 extern, while javalib needs a few hundred
   `scalanative_*` + libc functions. The realistic plan is to compile SN's C runtime (`nativelib/src/main/resources`)
   into the interpreter and reach it via FFI. Pointer/Intrinsic calls are a small, enumerable set (`runtime.Intrinsics`,
   `unsafe.Ptr`) that can be special-cased. Objects need SN's layout (rtti + fields) only where javalib takes the
   address of fields (`fromRawPtr`, `Intrinsics.classFieldRawPtr` in LazyVals/atomics).
4. **Startup.** Eager decoding costs ≈190 ms + inflate, so we need a prebuilt index and lazy per-method decode
   (see the numbers above).
5. **Debug NIR is verbose.** It has `var`/`varload`/`varstore` for every `var`/accumulator, redundant
   `copy`/`box`, and `method`+`call` pairs. A small pre-pass (var → SSA, direct-call devirtualisation for `final` /
   module methods) is cheap and worth doing.

A TASTy interpreter, by comparison, needs typed-tree evaluation plus reimplementations of pattern-match lowering,
mixin composition, lazy vals, erasure/bridges, inline expansion and by-name/closure conversion. It also still has
no code for `java.lang.*` (no TASTy exists for the JDK). **NIR removes that entire lowering layer and brings
javalib for free.** The costs are pinning to the SN 0.5 format (compat 6) and needing `nscplugin`-compiled user
code. For user scripts that means a JVM-hosted `dotc` today, and that is the real tension with the ≤ 50 ms goal.
It is the TASTy/front-end spikes' problem, not this one's.

## Not done

- The call-based unsafe intrinsics are not counted.
- `show/op.rs` (358 lines) should be split into ops and values/types so it meets the 200-line rule.
- `Defn`/`Inst` positions and lexical scopes are decoded but not printed.
