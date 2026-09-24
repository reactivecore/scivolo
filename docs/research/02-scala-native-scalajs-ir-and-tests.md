# Scala Native and Scala.js as reference points: IRs, compatibility subset, test suites, runtime/javalib

Research note for the Reactive Core "fast Scala 3 interpreter in Rust" project.
Date: 2026-09-24. Target: Scala 3.9.0 (LTS, released 2026-09-03), compatibility subset = Scala Native.

Legend: **[V]** = verified against a primary source (file, jar, API listing, docs page) during this research;
**[I]** = inference / judgement; **[U]** = reported by a secondary source, not independently verified.

---

## 0. Executive summary

- Scala Native's current release is **0.5.12 (2026-05-22)**; a 0.5.13 is in development (docs snapshots dated 2026-09-10).
  Maven Central carries `nscplugin_3.9.0/0.5.12` and `scalalib_native0.5_3/3.9.0+0.5.12`, i.e. **0.5.12 supports Scala 3.9.0** (and already publishes 3.10.0-RC1/RC2 artifacts). **[V]**
- **NIR** is a strongly typed, post-erasure, SSA-with-block-parameters IR: ~32 ops, 8 instruction kinds, a closed
  type system (`Ref(name, exact, nullable)`, `Array`, primitives, `Ptr`, structs). It is shipped as one binary
  `.nir` file per top-level class/trait/module inside ordinary jars; the **javalib and scalalib jars contain only
  `.nir` files** (no `.class`). Format is versioned (`compat=6`, `revision=11`), forward-compatible within a
  compat level, no formal spec (the `nir/` Scala sources are the spec; `docs/contrib/nir.md` is a partly stale overview). **[V]**
- An **NIR interpreter is feasible** and is the most direct way to inherit Scala Native's compatibility subset and
  its precompiled stdlib/javalib, but NIR is *low-level in places*: javalib/nativelib use raw pointers
  (`Elem/Load/Store/Stackalloc`), C `@extern` calls into libc/POSIX/zlib, and intrinsics. An interpreter must
  therefore include a "C machine" (byte-addressable memory, C ABI FFI) and re-implement or link the C parts of
  `nativelib`. **[I]**
- **SJSIR** is higher level (typed expression trees with first-class closures, `Match`, `TryCatch`, `Labeled`),
  has a written run-time semantics, a very strong stability policy (all 1.x IR readable by newer linkers; 32
  compatibility shims in the deserializer) and an existing reference interpreter (`sjrd/sjsir-interpreter`).
  But its javalib is JS-host-dependent (no files, sockets, threads) and its subset is not the project's target. **[V]**
- **Test ladders**: Scala Native `unit-tests/shared` (505 files, **7,388 `@Test`**, of which 5,479 javalib and 172
  pure language-semantics tests) runs on both JVM and Native and is the natural conformance suite; `scala-partest-junit-tests`
  runs scala/scala `test/junit` (2,944 `@Test`) minus a ~230-entry denylist; the full Scala 2 partest suite is
  Scala-2-only and nightly. Dotty's `tests/run` has **2,060 `.scala` files in 2,711 entries (172 dirs), 880 `.check` files**,
  182 marked `scalajs: --skip`, 127 `.java` files; Scala.js's shared test-suite has 2,713 `@Test`. **[V]**
- Prior art: **scalino** (Sept 2026) self-hosts dotc on Scala Native and expands macros with its own TASTy tree
  interpreter — the strongest evidence that a JVM-free Scala 3 toolchain is practical. No NIR interpreter exists.
  Rust JVMs (ristretto, rjvm, rusty-jvm) exist but none is documented to run Scala. **[V]/[U]**

---

## 1. Scala Native

### 1.1 Versions and Scala 3 support

| Item | Value | Status |
|---|---|---|
| Latest release | 0.5.12, 2026-05-22 (previous: 0.5.11 2026-04-07, 0.5.10 2026-01-24, 0.5.9 2025-10-12) | [V] GitHub releases |
| In development | 0.5.13 (docs snapshot `0.5.13-20260910-501cd90-SNAPSHOT`) | [V] scala-native.org/en/latest |
| Scala versions listed in 0.5.12 notes | 2.12.17–2.12.21, 2.13.9–2.13.18, 3.1.2–3.1.3, 3.2.0–3.2.2, 3.3.0–3.3.7 LTS, 3.4.0–3.4.3, 3.5.0–3.5.2, 3.6.2–3.6.4, 3.7.0–3.7.4, 3.8.0–3.8.3 | [V] release page |
| Scala 3.9.0 support | `org/scala-native/nscplugin_3.9.0/0.5.12/` and `scalalib_native0.5_3/3.9.0+0.5.12/` exist on Maven Central; also `3.9.0-RC1..RC6+0.5.12`, `3.10.0-RC1/RC2+0.5.12`, `3.8.4+0.5.12` | [V] repo1.maven.org listing |
| Publishing model | Since 0.5.0 (SIP-51) the toolchain publishes stdlib artifacts *per Scala version* (`<scala>+<sn>`); a `publishForScalaRelease.yml` workflow exists, so new Scala releases get artifacts without a new SN release | [V] repo tree + artifact names |
| 0.5.12 highlights | experimental **virtual threads** (Scala 3 only, Linux/macOS x86_64/aarch64, LLVM 16+, via `delimcc` continuations), GC safepoint/yieldpoint fixes for release builds, JDK<17 and LLVM<16 deprecated, javalib additions (SubmissionPublisher, Exchanger, StampedLock, DelayQueue, LinkedBlockingDeque, HexFormat) | [V] release notes |

Artifact naming gotcha (from scalino's findings and confirmed here): for Scala 3.x < 3.8 the real stdlib NIR is in
`scala3lib_native0.5_3` (`3.7.4+0.5.12` = 1.25 MB, 781 `.nir`) while `scalalib_native0.5_3` is a manifest-only jar;
**for Scala 3.8+ ("self-contained stdlib") it is the reverse**: `scala3lib_native0.5_3/3.9.0+0.5.12.jar` is 305 bytes
(manifest only) and `scalalib_native0.5_3/3.9.0+0.5.12.jar` is 7.6 MB with **4,982 `.nir` files** (e.g.
`scala/collection/immutable/List.nir` 43 KB, `scala/runtime/LazyVals.nir`). **[V]** (jar downloads + `unzip -l`)

Dependency jars (all 0.5.12, Scala 3): `javalib_native0.5_3` = 6.3 MB, **3,503 `.nir` + 12 `.c`, zero `.class`**;
`nativelib_native0.5_3` = 3.6 MB, 721 `.nir` + 619 `.class` + 184 `.tasty` + 87 `.h` + 76 `.c` + 9 `.hpp`
(the C/C++ runtime sources are shipped inside the jar and compiled by the toolchain with clang). **[V]**

### 1.2 Architecture

Pipeline (all **[V]** from repo layout, `project/Build.scala`, `nscplugin/`, `tools/`):

1. **`nscplugin`** (compiler plugin; separate Scala 2 and Scala 3 implementations). For Scala 3 it registers three phases:
   `PrepNativeInterop` (after `PostTyper`, before `Pickler`), `PostInlineNativeInterop` (after `Inlining`), and
   **`GenNIR`, which `runsAfter = Set(MoveStatics)` and `runsBefore = Set(GenBCode)`** — i.e. it consumes the same
   fully lowered trees (post-erasure, post-lambda-lift, post-constructors, post-`MoveStatics`) that the JVM backend
   sees, and emits one `.nir` per top-level symbol into the classes output directory (so `.nir` ends up in the jar next
   to `.class`/`.tasty` for user code; the published javalib/scalalib jars strip the class files).
2. **`tools`** (JVM library `tools_3`; also cross-built as `tools_native0.5_3` for self-hosting — that variant only
   works inside a native binary): `build/` (config, clang driver, LLVM), `linker/` (`ClassPath`, `ClassLoader.FromDisk`,
   `Reach`, `LinktimeValueResolver`, `LinktimeIntrinsicCallsResolver`), `interflow/` (optimizer),
   `codegen/` (`Lower`, `MemoryLayout`, `FieldLayout`, `VirtualTable`, `ITable`, `DynamicHashMap`/`PerfectHashMap`,
   `RuntimeTypeInformation`, `GenerateReflectiveProxies`, `ResourceEmbedder`, `llvm/` emitter), `checker/`.
3. **Reachability** (`Reach.scala`): starts from entry points (`main`, classes with `hasEntryPoints` in the NIR
   prelude — e.g. `@EnableReflectiveInstantiation` classes — and configured service providers), lazily `lookup`s
   definitions from the classpath jars, tracks `Op.Method` targets through the class hierarchy and `Op.Dynmethod`
   candidates ("any method that implements a known dynamic signature becomes reachable"), reaches module constructors
   on module allocation, and reports missing/unsupported/stub symbols (`linkStubs`).
4. **Interflow** (Shabalin & Odersky, Scala Symposium 2018): interprocedural flow-sensitive type inference, method
   duplication, inlining, partial evaluation, allocation sinking; produces `Ref(exact=true)`/non-nullable refinements. Skipped in `debug` mode.
5. **Lower + codegen**: high-level ops become memory ops over a fixed object layout (rtti pointer header, vtables,
   itables, dynmap), emitted as LLVM `.ll` and compiled/linked with clang. Build modes `debug`/`release-fast`/
   `release-size`/`release-full`, LTO `none`/`thin`/`full`, GC `immix`/`commix`/`boehm`/`none`.

### 1.3 NIR precisely

Source of truth: `nir/src/main/scala/scala/scalanative/nir/` (`Attrs, Bins, Comps, Convs, Defns, Global, Insts,
Linktime, Local, Mangle, Next, Ops, Rt, Show, Sig, Types, Vals, Versions, serialization/{Prelude, Tags,
BinarySerializer, BinaryDeserializer}`). **[V]**

**File format.** Header (`Prelude`, 45 bytes): `magic:int = 0x2E4E4952 (".NIR")`, `compat:int`, `revision:int`,
eight section offsets (`offsets, strings, positions, globals, types, defns, vals, insts`), `hasEntryPoints:bool`.
Hex dump of `java/lang/String.nir` from javalib 0.5.12: `2e4e4952 00000006 0000000b ...` → compat 6, revision 11,
matching `Versions.scala` on `main`. Payload uses LEB128 varints, a string pool, per-`Global` offset table enabling
**lazy per-definition deserialization** (`in(start){...}` caching), source positions section, and numeric tags
(`Tags.scala`, with deliberate holes for future nodes). One `.nir` per `Global.Top`. **[V]**

**Versioning.** `Versions.scala`: "Revision is increased on any change to the format. Compatibility level is increased
whenever the given change is breaking… NIR emitted via 1.1-compatible compiler should read without any changes in
1.2-based compiler, but not the other way around." Reader accepts `compat == 6 && revision <= 11`. 0.5.0 bumped compat
(binary-incompatible with 0.4.x). So: **stable within the 0.5.x line, will break at 0.6**. No spec document; the
`nir` module (plus `docs/contrib/mangling.md`, which gives a formal name-mangling grammar) is the reference. `Show.scala`
prints a textual form used in docs/tests but there is no textual parser. **[V]**

**Names.** `Global.Top(id)` (class/trait/module; type aliases are not preserved) and `Global.Member(owner: Top, sig: Sig)`.
`Sig` kinds: `Field`, `Ctor(types)`, `Clinit`, `Method(name, types(params ++ ret), scope)`, `Proxy` (reflective proxy
for structural calls), `Extern`, `Generated`, `Duplicate` (Interflow specialisations). Scopes `Public`, `PublicStatic`
(Java statics only; "there is no static member in Scala Native" for Scala code), `Private(in)`, `PrivateStatic`.
Mangled, Itanium-ABI-like (`_SM<name>D<name><types>E<scope>`). Overloads are distinguished purely by erased parameter types. **[V]**

**Definitions (`Defn`).** `Var(attrs, name, ty, rhs)` (global or field), `Const`, `Declare(attrs, name, ty: Function)`
(abstract/extern), `Define(attrs, name, ty, insts, debugInfo{localNames, lexicalScopes})`, `Trait(attrs, name, traits)`,
`Class(attrs, name, parent, traits)`, `Module(attrs, name, parent, traits)`. **[V]**

**Attributes (`Attrs`, 17 fields).** `inlineHint (MayInline|InlineHint|NoInline|AlwaysInline)`, `specialize`, `opt`,
`align`, `isExtern`, `isBlocking`, `isDyn` (callable via structural dispatch), `isStub`, `isAbstract`, `isVolatile`,
`isFinal`, `isSafePublish`, `isLinktimeResolved`, `isUsingIntrinsics`, `links: Seq[Link]`, `preprocessorDefinitions`,
`linkCppRuntime`. (The docs page still lists `pin`, `pin-if`, `override`, `pure` — those are gone.) **[V]**

**Types (`Type`).** Value kinds: `Bool, Char (u16), Byte, Short, Int, Long, Int128, Float, Double, Size (ssize_t), Ptr,
ArrayValue(ty, n), StructValue(tys)`. Reference kinds: `Null, Unit, Array(elemTy, nullable), Ref(name: Global.Top, exact, nullable)`.
Others: `Vararg, Nothing, Virtual` (Interflow-only), `Var(ty)`, `Function(args, ret)`. **Post-erasure: no generics
anywhere; `List[Int]` is `Ref(@scala.collection.immutable.List)`**. `Type.box/unbox` map primitives to
`java.lang.Integer` etc.; unsigned wrappers (`UInt`…) unbox to signed. Primitive class objects are
`scala.scalanative.runtime.PrimitiveInt` etc.; arrays are runtime classes `scala.scalanative.runtime.{Boolean,Char,Byte,Short,Int,Long,Float,Double,Object,Blob}Array`. **[V]**

**Values (`Val`).** `True/False, Null, Unit, Zero(ty), Size, Char, Byte, Short, Int, Long, Int128(hi, lo), Float, Double,
StructValue, ArrayValue(elemTy, values), ByteString, Local(id, ty), Global(name, ty), Const(v), String(s), Virtual(key), ClassOf(name)`. **[V]**

**Instructions (`Inst`).** `Label(id, params: Seq[Val.Local])` (block parameters = phis), `Let(id, op, unwind: Next)`,
control flow `Ret(v)`, `Jump(next)`, `If(cond, then, else)`, `Switch(v, default, cases)`, `Throw(v, unwind)`,
`Unreachable(unwind)`, and link-time `LinktimeIf(cond, then, else)` (resolved by the linker from `LinktimeInfo` properties).
`Next`: `None`, `Unwind(exc: Val.Local, next: Next)`, `Case(value, next)`, `Label(id, args)`. **[V]**

**Ops (32).** Low-level: `Call(ty, ptr, args)`, `Load(ty, ptr, memoryOrder)`, `Store`, `Elem(ty, ptr, indexes)` (GEP),
`Extract`, `Insert`, `Stackalloc(ty, n)`, `Bin(bin, ty, l, r)` with bins `Iadd Fadd Isub Fsub Imul Fmul Sdiv Udiv Fdiv Srem
Urem Frem Shl Lshr Ashr And Or Xor`, `Comp` with `Ieq Ine Ugt Uge Ult Ule Sgt Sge Slt Sle Feq Fne Fgt Fge Flt Fle`,
`Conv` with `ZSizeCast SSizeCast Trunc Zext Sext Fptrunc Fpext Fptoui Fptosi Uitofp Sitofp Ptrtoint Inttoptr Bitcast`,
`Fence(order)`. High-level: `Classalloc(name, zone)`, `Fieldload(ty, obj, name)`, `Fieldstore`, `Field(obj, name)` (pointer),
`Method(obj, sig)` (virtual lookup → fn pointer), `Dynmethod(obj, sig)` (structural), `Module(name)` (lazy singleton),
`As(ty, obj)`, `Is(ty, obj)`, `Copy`, `SizeOf`, `AlignmentOf`, `Box(ty, obj)`, `Unbox`, `Var(ty)`/`Varload`/`Varstore`
(mutable locals, removed by later passes), `Arrayalloc(ty, init, zone)`, `Arrayload`, `Arraystore`, `Arraylength`. **[V]**

**How Scala constructs are encoded** (from `nscplugin/src/main/scala-3/.../NirGenExpr.scala`, **[V]**):

- *Exceptions*: every `Let`/`Throw` carries `unwind`; inside a `try`, ops unwind to a handler block that receives the
  exception in `Next.Unwind(exc, Label(...))`. `genTry` emits an `is`-test chain for `catch` cases; `finally` bodies are
  **duplicated on every exit edge** (`genTryFinally`). At run time the C++ runtime wraps the object:
  `scalanative_throw(obj)` does `throw ExceptionWrapper(obj)` (`nativelib/.../eh.cpp`), LLVM `invoke`/landingpads catch it.
  An interpreter simply propagates to the `unwind` target — no C++ EH needed. **[V]/[I]**
- *Closures*: Scala 3 `Closure` trees become a **synthesised class `Owner$$Lambda$N`** with `capture$i` fields, a
  constructor and the SAM method delegating to the lifted method ("similarly to closure encoding on Scala 2.11 and
  earlier"); no `invokedynamic`/`LambdaMetaFactory`. Closures are therefore ordinary classes in NIR. **[V]**
- *Boxing*: `Op.Box/Unbox` for primitives; value classes are re-constructed by constructor call (`ensureBoxed/ensureUnboxed`). **[V]**
- *Pattern matching*: `Match` on `Int` scrutinee → `Switch`; otherwise if/else chains; the compiler's earlier phases
  have already desugared patterns into labeled blocks/ifs. String concatenation → `java.lang.StringBuilder` appends. `synchronized` → try/finally around `enterMonitor/exitMonitor` (runtime `ObjectMonitor`). `classOf[T]` → `Val.ClassOf`. **[V]**
- *Modules*: `Op.Module(name)` loads (and lazily initialises via `module_load.c`) the singleton whose ctor is `Global.Member(name, Sig.Ctor(Seq()))`. **[V]**
- *Dispatch*: `Op.Method(obj, sig)` is resolved by the toolchain to vtable/itable slots
  (`VirtualTable.scala`, `ITable.scala`, `itable_search.c`) and `Op.Dynmethod` to a perfect-hash `dynmap`
  (`dyndispatch.c`). `Op.Is/As` use class-id ranges (`java.lang.Class` runtime fields: `id, interfacesCount,
  interfaces, name, size, idRangeUntil, refFieldOffsets, itablesCount, itables, superClass`, cf. `Rt.jlClassFields`). **[V]**
- *Strings*: `java.lang.String` has fields `value: CharArray, offset, count, cachedHashCode` (`Rt.jlStringFields`); string literals become `Val.String` / interned `string_constants.c`. **[V]**
- *Native interop*: `@extern` methods → `Declare` with `isExtern` and unmangled names; `Ptr[T]`, `CStructN`, `CArray`,
  `stackalloc`, `Zone` allocations map to `Ptr`/`StructValue`/`Stackalloc`/`Elem`/`Load`/`Store`; `CFuncPtr` wraps raw
  fn pointers; intrinsics live in `scala.scalanative.runtime.{Intrinsics, LLVMIntrinsics, ffi}` and are recognised by `isUsingIntrinsics`. **[V]**

### 1.4 Feasibility of an NIR interpreter in Rust

Assessment **[I]**, based on the facts above:

Favourable:
- NIR is **already what the project wants semantically**: erased, monomorphic, closed-world, with Scala-specific
  constructs (modules, traits, boxing, `is/as`, arrays) still explicit *before* `Lower`. An interpreter should consume
  the **unlowered** `.nir` straight from jars and never run Interflow/Lower.
- SSA with block parameters is a natural register machine; ~32 ops plus 8 instruction kinds is a small core.
  Deserialisation is documented by `BinaryDeserializer.scala`/`Tags.scala` and allows lazy, per-definition loading —
  good for start-up time. A Rust reader for the format is a few thousand lines.
- **Precompiled code exists for every Scala 3 version** (`scalalib_native0.5_3/<v>+0.5.12`, `javalib`, `nativelib`,
  `auxlib`, `clib`, `posixlib`) — exactly the "use precompiled stdlib instead of interpreting it" requirement, with
  no JVM at run time. User code is compiled with the standard `nscplugin` (JVM-hosted dotc, or scalino's native dotc).
- Reachability semantics are simple and can be reused (or ignored: an interpreter can resolve lazily at first call).

Unfavourable / work items:
- **Raw memory semantics.** javalib and nativelib are written against `unsafe` (pointers, `stackalloc`, `Zone`,
  struct layouts via `SizeOf`/`Elem`), and use ~76 `.c` + 9 `.cpp` runtime files (GC, `eh.cpp`, `dyndispatch.c`,
  `module_load.c`, `StackTrace.c`, `delimcc.c`, `zone/`, `platform/`) plus `@extern` calls into libc/POSIX/zlib.
  A faithful interpreter therefore needs (a) a byte-addressable native heap/stack for `Ptr` values, (b) a C-ABI FFI
  (`libffi`/`dlsym`) for `Declare(isExtern)` symbols, and (c) either its own object layout or SN's exact
  `MemoryLayout` so that runtime Scala code doing pointer arithmetic on objects (e.g. `runtime/Arrays.scala`,
  `Intrinsics`) keeps working. The pragmatic route is to adopt SN's layout (rtti header + fields) and *link SN's own
  C runtime* (compiled with the `cc` crate) so `scalanative_*` entry points exist; with GC = **Boehm** (a supported
  SN option) the interpreter needs no precise root maps initially.
- **Format churn**: NIR compat bumps with major SN releases (0.4→0.5 did); the interpreter is pinned to one SN line.
- **No semantic spec**: behaviour of e.g. `Op.Bin` on `Char`, `Sdiv` by zero (`ArithmeticException` is inserted by
  codegen, not by NIR), null checks, and array bounds checks live in `Lower`/codegen and must be re-derived from
  `tools/src/main/scala/scala/scalanative/codegen/Lower.scala`.
- **Link-time conditions** (`LinktimeIf`, `LinktimeInfo` properties such as `isWindows`, `is32BitPlatform`,
  `isMultithreadingEnabled`) must be evaluated by the interpreter (`LinktimeValueResolver` is small and reusable in spirit).
- **Multithreading**: 0.5 javalib assumes threads; `Fence`/memory orders, monitors and `@blocking` GC notifications
  must be honoured or the interpreter must run in the "no system threads → single-threaded mode" configuration the
  toolchain also supports.
- Debug-mode NIR is verbose (many `Var/Varload/Varstore`, redundant boxing). Expect bytecode-interpreter-class
  performance; Interflow-style inlining/specialisation can be added later, or a Cranelift JIT.

Net: **an "NIR VM" is feasible and attractive**, comparable in scope to writing a JVM interpreter that must also
model a small C machine, minus class-file parsing/verification complexity and minus JDK native-method surface.

---

## 2. Scala Native "javalib", runtime, and the unsupported subset

### 2.1 javalib coverage (0.5.12 `main`, file counts from the repo tree) **[V]**

| Package | Files | Notes |
|---|---|---|
| `java.io` | 46 | streams, readers/writers, `File`, `RandomAccessFile` (POSIX/Win32 via `@extern`) |
| `java.lang` | 42 (+`annotation` 3, `constant` 2, `impl` 3, `invoke` 1, `management` 9, `process` 16, `ref` 7, `reflect` 5) | `Thread`, `Process`/`ProcessBuilder`, `ref.WeakReference` (GC-integrated), `reflect` contains only the minimal types needed (`Array`, `Method`-less); **no real reflection** |
| `java.math` | 11 | `BigInteger`/`BigDecimal` (ported from Apache Harmony) |
| `java.net` | 30 | sockets, `ServerSocket`, `DatagramSocket`, `InetAddress` (IPv4/IPv6), `URI`, `URLEncoder/Decoder`; **`java.net.URL` removed in 0.5.0** |
| `java.nio` | 35 (+`channels` 20, `charset` 12, `file` 37, `file.attribute` 22, `file.glob` 4, `spi` 2) | buffers incl. `MappedByteBuffer`, `Files`/`Path`/`FileSystem` on POSIX and Windows |
| `java.security` | 4 | stubs removed in 0.5.0 (`MessageDigest` etc. not implemented) |
| `java.util` | 81 (+`concurrent` 56, `concurrent.atomic` 17, `concurrent.locks` 10, `function` 43, `jar` 8, `random` 8, `regex` 4, `stream` 12, `zip` 20) | collections, `Formatter`, `Properties`, `ServiceLoader` (AOT-configured), `Base64`, `UUID`; `concurrent` is the JSR-166 port (executors, `ForkJoinPool`, `CompletableFuture`…); `zip` uses zlib via C; `regex` is a port of Google RE2 (`scala.scalanative.regex`) |
| `niocharset` | 9 | UTF-8/16/ISO-8859-1/US-ASCII |
| `scala.scalanative.{ffi, nio.fs, runtime.ieee754tostring.ryu, javalibintf}` | ~25 | platform glue, Ryu float printing |

Not provided (use third-party or absent) **[V]** (docs/lib/javalib.md, changelog 0.5.0):
- `java.time` (use `scala-java-time` or `sjavatime`), `java.util.Locale` (`scala-java-locales`), `java.text` (n/a).
- `java.lang.reflect` (removed), `Class.forName`, `ClassLoader`, dynamic class loading, `java.lang.invoke` (single stub file),
  serialization, `SecurityManager`, JNI, `getResource(s)` (only `getResourceAsStream` with `withEmbedResources(true)`).
- `java.util.regex` differences: RE2 — no lookaround, no possessive quantifiers, no class unions/intersections,
  no `\G \R \Z`, several `Pattern` flags unsupported; `Matcher.hitEnd/requireEnd/useTransparentBounds` throw.
- **Java source dependencies do not work at all** (only Scala-cross-published libraries link) — scala-cli docs.
- Finalizers never run. Stack overflow is *documented* as undefined behaviour/segfault (there is a `stackOverflowGuards.c`
  in nativelib on `main`, so this may be improving — **[U]**).
- Unit-test directories `require-jdk9 … require-jdk26` show the javalib tracks APIs up to JDK 26 selectively. **[V]**

Reflection: only *reflective instantiation* via `@EnableReflectiveInstantiation` + `scala.scalanative.reflect.Reflect`
(same design as Scala.js / portable-scala-reflect); classes so annotated set `hasEntryPoints` in their `.nir` prelude
so the linker keeps them. Structural types (`Dynmethod`) are supported. **[V]**

### 2.2 Runtime (`nativelib`) **[V]**

- Scala side: `scala.scalanative.runtime.{Arrays, Boxes, Class, GC, Intrinsics, LLVMIntrinsics, MemoryLayout, NativeThread,
  Object, Platform, Primitives, RawPtr, RawSize, StackTrace, Throwable, VirtualThread, VirtualThreadScheduler, ffi, unwind,
  vmoffset}` plus `monitor.ObjectMonitor`, `unsafe`, `unsigned`, `libc`.
- C/C++ side (in-jar resources `scala-native/`): `gc/` (118 files: `immix`, `commix`, `boehm`, `none`, `shared`
  incl. `YieldPointTrap.c`, `SafepointPollTrampoline-{x86_64,aarch64}.S`), `eh.cpp`/`eh.c` (C++ exception wrapper),
  `dyndispatch.c`, `itable_search.c`, `module_load.c`, `nativeThreadTLS.c`, `delimcc.c` (delimited continuations for
  virtual threads), `StackTrace.c` (libunwind), `string_constants.c`, `zone/`, `platform/` (38 files), `EmbeddedResourceReader.c`.
- GCs: **Immix** (default, mostly-precise mark-region), **Commix** (parallel mark / concurrent sweep Immix), **Boehm**
  (conservative), **none** (bump allocation; per-thread 64 MB blocks). Env: `GC_INITIAL_HEAP_SIZE`, `GC_MAXIMUM_HEAP_SIZE`,
  `SCALANATIVE_GC_SYNC_TIMEOUT_MS` (60 s), `SCALANATIVE_GC_TRAP_BASED_YIELDPOINTS`.
- Threads (since 0.5.0, 2024-04): platform threads behind `java.lang.Thread`; object monitors "following partially JVM
  semantics"; `@volatile` honoured; **final fields use a relaxed model by default** (`@safePublish` / `-Pscalanative:forceStrictFinalFields`
  for JMM semantics); stop-the-world via safepoints/yieldpoints (trap page + `mprotect`, signal handler redirects to an
  asm trampoline — `docs/contrib/gc-safepoint-trampoline.md`); `@blocking` externs notify the GC. If no thread is ever
  created the program is linked in single-threaded mode. 0.5.12 adds experimental virtual threads.

---

## 3. Test suites

### 3.1 What Scala Native runs in CI **[V]** (`project/Commands.scala`, `.github/workflows/run-tests-linux.yml`)

| sbt command / project | Content | Size | Notes |
|---|---|---|---|
| `test-tools <scala>` | `nscPlugin`, `nirJVM`, `toolsJVM`, `testRunner`, `testInterface`, `nir`, `tools` tests + MiMa | tools: 32 test files; nscplugin: 23; nir: 5 | JVM-side toolchain tests (linker, optimizer, codegen, NIR round-trip) |
| `test-runtime <scala>` | `sandbox/run`, **`testsJVM`, `tests`** (`unit-tests/`), `testsExtJVM`, `testsExt` (`unit-tests-ext/`), `junitTestOutputsJVM/Native` (`junit-test/`), **`scalaPartestJunitTests`** | see below | run for every Scala binary version in the matrix (2.12, 2.13, 3), plus extended matrix jobs for GCs/LLVM versions (17–21, latest) and no-opt builds |
| `test-gc <scala>` | `sandbox`, `testInterface`, `junitTestOutputsNative` under each GC (`none, immix, commix, boehm`) | — | |
| `test-scripted <scala>` | sbt plugin scripted tests (`scripted-tests/`) | ~30 min | |
| `test-scala-partests` (nightly / `[test-partest]`) | `scalaPartestTests` — full scala/scala partest (`test/files/{pos,neg,run}`) via SN implementations of the partest interface | ~2 h per Scala version | **Scala 2 only** ("No partests support for Scala 3") |
| `test-mima` | binary-compat of published artifacts | — | |

Test infrastructure: JUnit 4 API re-implemented in `junit-runtime` (`org.junit.*`, `org.hamcrest.*` subset,
`com.novocode.junit.JUnitFramework`), a `junit-plugin` compiler plugin (Scala 3: `ScalaNativeJUnitBootstrappers`) that
generates a `Bootstrapper` object per test class because there is no reflection, an in-binary sbt `test-interface`
(`TestMain`, `NativeRPC`, `FrameworkLoader`, `TestAdapterBridge`) and a JVM-side `test-runner` (`ProcessRunner` spawns the
binary, `ComRunner`/`NativeRunnerRPC` talk to it over a **TCP socket on an ephemeral port**, relaying sbt testing events). **[V]**

### 3.2 `unit-tests` in detail **[V]** (counts = `@Test` annotations, `grep` on `main` 2026-09-24)

- `unit-tests/shared/src/test/` — **505 files, 7,388 tests**, compiled and run on **both JVM (`testsJVM`, denylist of
  17 files) and Native**. Layout `scala/org/scalanative/testsuite/javalib/<pkg>/*Test.scala` plus:
  - javalib **5,479**: `util` 3,749 (of which `util.concurrent` 2,193, `util.stream` 346, `util.zip` 134,
    `util.regex` 98, `util.function` 75, `util.jar` 56), `math` 756, `lang` 354, `nio` 294, `io` 219, `net` 107.
  - `scala/scala/` **27 files, 172 tests of core language/runtime semantics** (`ArrayIntCopyTest`, `AsInstanceOfTest`,
    `DivisionByZeroTest`, `DivisionOverflowTest`, `DoubleComparisonTest`, `EnumerationTest`, `EqualityTest`,
    `ExceptionHandlingTest`, `ExecutionContextTest`, `FloatingToIntegerOverflowTest`, `HashCodeTest`,
    `IsInstanceOfTest`, `NullPointerTest`, `ObjectCloneTest`, `PrimitiveTest`, `ShiftOverflowTest`, `bugcompat/`,
    `compiler/DefaultMethodsTest`, `partest/{DigitIdentifiersTest, MixinTest, t10032Test}`, `util/PropertiesTest`).
  - `scala-3/scala/` (`EnumTest`, `StaticMembersTest`, `issues/`, `reflect/`), `scala-2*/`, and `require-jdk9…26/` +
    `require-scala3-jdkN/` (**1,462 tests** gated by the JDK used for the JVM run).
  - `runtime/{LargeTraitsCompositionTest, NullGuardsTest}`, `scalalib/reflect`.
- `unit-tests/native/src/test/` — 116 files, **952 tests**, Native only: `scalanative.{unsafe (18 files), regex (23),
  runtime (10), libc, unsigned, reflect, meta, annotation}`, `posixlib`, `windowslib`, `niobuffer`, `javalib` process tests.
- `unit-tests-ext` — 10 files, 35 tests (javalib "ext" dummies). `junit-test/shared` — 41 files, 79 tests whose
  expected outputs (681 files under `junit-test/outputs`) check JUnit framework behaviour.

### 3.3 External suites **[V]**

- **`scala-partest-junit-tests`**: fetches the scala/scala sources for the current 2.13 version and compiles
  **`test/junit`** (2.13.x: **334 files, 2,944 `@Test`**, mostly collections/`scala.*` library behaviour) against the
  SN scalalib, minus `src/test/resources/<version>/DenylistedTests.txt` (2.13.18: 243 non-comment lines ≈ 230 files,
  starting with "Do not compile" entries such as `scala/OptionTest`, `collection/immutable/HashMapTest`, and Scala-2-only
  bytecode tests) and `DenylistedTests-require-threads.txt` (2 files). Included in `test-runtime` for every matrix
  Scala version, so for the Scala 3 build the 2.13 test sources are compiled with Scala 3 (**[I]**; the denylist coherence
  check runs per version).
- **`scala-partest-tests`** (`scala/tools/partest/scalanative/<ver>/DenylistedTests.txt`, ≈ 1,000 entries for 2.13.18,
  "Ported from Scala.js, might not be exhaustive"): full partest against scala/scala **`test/files/run` (3,502 entries:
  1,757 top-level `.scala`, 330 dirs, 1,405 `.check`), `pos` (1,879), `neg` (2,858)**. Scala 2 only, nightly.
- Scala 3 **community build** (`community-build/src/scala/dotty/communitybuild/projects.scala`, ~73 projects): contains
  Scala.js variants (`coreJS/test` for cats, fs2, munit, scalacheck, scodec-bits…) but **no Scala Native projects**. Scala
  3 CI has a `tests/sjs-junit` directory for Scala.js; there is no Scala Native job (**[I]** from the tree listing).
  Conversely, the Scala 3 release process triggers SN's `publishForScalaRelease` workflow (artifacts exist for 3.10.0-RC1).

### 3.4 Dotty `tests/run` as a conformance suite **[V]** (`scala/scala3` `main`, sparse checkout 2026-09-24)

- `tests/run`: **2,711 top-level entries** = 2,060 `.scala` files (incl. inside 172 test directories), **880 `.check`
  files**, 127 `.java` files (81 directories need `javac`), 3 `.ignore`, 1 `.javaopts`, 2 `.pastie`.
- Conventions (docs/_docs/contributing/testing.md): a test is `iNNN.scala` or a directory `iNNN/`; must define
  `object Test { def main(args: Array[String]) }` (1,240 files) or `@main def Test` (390 files); stdout is compared with
  `iNNN.check` if present (else just "runs without exception"); `_1/_2` suffixes give separate-compilation order;
  header comments carry flags (`// scalajs: --skip` in **182** files marks tests Scala.js cannot run — a ready-made
  "needs JVM reflection/threads/etc." list). Grep estimates: 36 files use reflection (`Class.forName`,
  `getDeclaredMethod`, `java.lang.reflect`), 9 use threads/futures, 6 use `scala.quoted` at run time.
- Sibling directories: `run-macros` (317 dirs, 203 `.check`), `run-staging` (103 `.scala`), `run-deep-subtype` (14),
  `run-custom-args`, `run-with-compiler` (4), `run-bootstrapped` (3), `run-tasty-inspector`, and
  `old-tasty-interpreter-prototype` (see §5).

### 3.5 Scala.js test-suite (for comparison) **[V]**

`test-suite/shared/src/test/scala/org/scalajs/testsuite/{compiler, javalib, junit, niobuffer, niocharset, scalalib, utils}`:
**272 files, 2,713 `@Test`** (javalib 2,274, compiler 239) + `js/` 959 JS-specific tests + `require-jdk21`/`require-scala2`.
The `sjsir-interpreter` project runs this entire suite through its interpreter (`scalajs-test-suite/test`).

### 3.6 Recommended incremental conformance ladder **[I]**

1. **Bootstrap**: hand-written smoke programs + SN `unit-tests/shared/.../scala/` language-semantics tests (172 tests,
   27 files) — arithmetic/overflow/shift semantics, `isInstanceOf`, equality/hashCode, exceptions, NPE, clone, arrays.
   These need only `java.lang` basics and the JUnit runtime, and they run on JVM too, so expected behaviour is unambiguous.
2. **javalib by package in dependency order**: `lang` (354) → `util` core collections → `math` (756, pure Scala) →
   `io` → `nio` → `util.stream`/`function` → `net`/`zip`/`regex` (need FFI) → `util.concurrent` (2,193; needs threads).
   Track pass counts per package; the JVM denylist (17 files) marks tests known to diverge.
3. **Dotty `tests/run`** filtered: drop directories with `.java`, drop `scalajs: --skip`, drop `run-macros`/`run-staging`
   unless run-time quoting is supported; compile the rest with dotc+nscplugin to NIR and diff stdout against `.check`.
   This is the best *language-level* Scala 3 conformance signal (~1,800 programs).
4. **`scala-partest-junit-tests`** (2.13 `test/junit`, ~2,700 collection-heavy tests after denylist) and the Scala.js
   shared test-suite (2,713 tests, cross-checks javalib semantics from a second implementation).
5. **Ecosystem**: munit/utest, cats, fs2, os-lib test suites cross-published for `native0.5` (same set as the
   Scala Native "community lib" list).

Practical note: because `junit-plugin` generates reflection-free `Bootstrapper`s, an interpreter can run the SN unit
tests by loading the test classes' `.nir` together with `junit-runtime` and driving the bootstrappers from a small Rust
(or NIR) driver — it does not have to speak the sbt TCP `test-runner` protocol, although doing so would allow `sbt test` to
target the interpreter transparently.

---

## 4. Scala.js IR (SJSIR)

### 4.1 Format and level **[V]** (`ir/shared/src/main/scala/org/scalajs/ir/{Trees, Types, Serializers, ScalaJSVersions, Tags}.scala`, `main` = 1.23-SNAPSHOT; 1.22.0 ships with Scala 3.9.0)

- One `.sjsir` file per class. Binary header: magic **`0xCAFE4A53`** ("cafe" + "JS"), then a UTF-8 version string;
  then entry-points info, a file/URI table, an *encoded names* pool, a method-name pool (simple name + param/result
  `TypeRef`s + reflective-proxy flag), a string pool, and exactly one `ClassDef`. Positions are delta-compressed.
  `Tags.scala` defines 140 node tags. The Scala 3 emitter is built into the compiler (`dotty.tools.backend.sjs`,
  enabled by `-scalajs`), no plugin is needed.
- **Level**: a *typed expression-tree IR*, not SSA. `ClassDef(name, kind (Class/ModuleClass/Interface/JSClass/…),
  superClass, interfaces, fields, methods, jsConstructor, jsNativeMembers, topLevelExportDefs)`; `MethodDef(flags, name,
  params, resultType, body, optimizerHints)`; every `Tree` carries `tpe: Type`. Statement/expression nodes: `VarDef, Skip,
  Block, Labeled/Return, If, LinkTimeIf, While, ForIn, TryCatch, TryFinally, Match (int/String, break-free), Throw`;
  Scala nodes `New, LoadModule, StoreModule, Select, SelectStatic, Apply, ApplyStatically, ApplyStatic,
  ApplyDynamicImport, ApplyTypedClosure, NewLambda (since 1.19: lambda descriptor + function), UnaryOp, BinaryOp
  (numeric ops with explicit codes, incl. `Long` ops), NewArray, ArrayValue, ArraySelect, ArrayLength, IsInstanceOf,
  AsInstanceOf, GetClass, Clone, IdentityHashCode, WrapAsThrowable/UnwrapFromThrowable`, literals, and
  **`Closure(flags{arrow, typed, async}, captureParams, params, restParam, resultType, body, captureValues)`** — first-class closures.
  JS-only nodes (`JSNew, JSSelect, JSMethodApply, JSFunctionApply, JSSuper*, JSGlobalRef, JSImportCall, JSNewTarget,
  JSObjectConstr, JSArrayConstr, JSSpread, JSUnaryOp/JSBinaryOp, JSAwait, LoadJSConstructor/LoadJSModule, JSClassDef`)
  are syntactically distinct and can be rejected by a non-JS interpreter.
- **Types**: `VoidType, NothingType, UndefType, BooleanType, CharType, ByteType, ShortType, IntType, LongType, FloatType,
  DoubleType, StringType (non-null), NullType, ClassType(name, nullable, exact), ArrayType(ArrayTypeRef(base, dims),
  nullable, exact), ClosureType(params, result, nullable), RecordType, AnyType/AnyNotNullType`. Post-erasure (no generics;
  method names carry erased `TypeRef`s), with nullability tracking.
- **Semantics document exists**: "Scala.js IR — run-time semantics" (chara.epfl.ch/~doeraene/sjsir-semantics, also in
  Doeraene's 2018 EPFL thesis), written as an addition to the ECMAScript 2018 spec: data types, abstract operations,
  exotic objects (Scala objects, arrays, chars, longs), evaluation rules for 60+ statement/expression forms. NIR has nothing comparable.
- **Stability**: `ScalaJSVersions.binaryEmitted = "1.23-SNAPSHOT"`; readers accept any `1.x` with `x <= current minor`
  ("binary minor versions are backwards compatible"). `Serializers.scala` (2,999 lines) carries **32 `hacks.useBelow(v)`
  rewrites** for IR from 1.0 … 1.19 (e.g. rewriting pre-1.19 `New(AnonFunctionN)` into `NewLambda`, 1.17 nullable
  `IsInstanceOf` fixes, 1.13 `DoWhile` removal). All 1.x libraries ever published remain linkable — a decade-long guarantee.

### 4.2 Existing SJSIR interpreters and backends **[V]**

- **`sjrd/sjsir-interpreter` 0.4.0** (Apache-2.0; "prototyped by @youroff in the context of a master project at EPFL";
  built on `scalajs-linker 1.22.0`, Scala 2.13.18): a Scala.js library that runs in a JS engine, "reads and executes
  `.sjsir` files directly … (much) slower, but it allows to dynamically load more `.sjsir` files". Architecture:
  `Compiler.scala` (19 KB) translates IR trees into a closure/node tree (`Nodes.scala`, 48 KB) with pre-resolved
  local-variable slots (captures indexed 0..N-1, then locals, plus `this`), `Executor.scala` (23 KB) evaluates them,
  `ClassInfo`/`MethodInfo` do dispatch (`Apply` virtual, `ApplyStatically`, `ApplyStatic`), `LambdaSynthesizer`
  materialises `NewLambda`, `LabelException` implements `Labeled/Return`, `Instance`/`ArrayInstance` model values. It runs
  the **full Scala.js test suite** (`scalajs-test-suite/test`) and a reversi demo in the browser.
- No Truffle/GraalVM SJSIR interpreter was found (search 2026-09). Scala Native likewise has no NIR interpreter; the only
  "interpreter" adjacent to SN is scalino's TASTy tree interpreter for macro expansion (§5).
- **WebAssembly backend**: stable since Scala.js 1.22.0; requires Wasm 3.0 (WasmGC + exception handling), ESModule,
  ES2022 host; Node 25 / Chrome 137 / Firefox 134 / Safari 26; ~30 % lower geomean run time on Scala-heavy code, ~2×
  code size; `@JSExport` silently ignored, single module only. Implemented in `linker/.../backend/wasmemitter` (16 files).
  Relevant as a proof that SJSIR's semantics are engine-independent enough to target a typed GC'd VM.

### 4.3 Interpreting SJSIR vs NIR **[I]**

| Criterion | SJSIR | NIR |
|---|---|---|
| Abstraction | typed expression trees; closures, `Match`, `TryCatch`, `Labeled` first class | SSA basic blocks; closures already classes; try/finally expanded |
| Memory model | fully managed; no pointers | managed objects **plus** raw `Ptr`, `Stackalloc`, `Elem`, `Load/Store`, `SizeOf`, C ABI externs |
| Spec | written run-time semantics + reference interpreter + Hashers/Printers | none; `Lower.scala`/codegen are the spec |
| Stability | all 1.x readable; explicit compat shims | forward-compatible only within compat level 6 (0.5.x) |
| Stdlib / javalib available precompiled | yes (`scalajs-library`, `scalajs-javalib` as `.sjsir`) but javalib assumes a JS host: JS strings, `console`, `js.Dynamic`; no files, sockets, processes, threads (294 javalib files) | yes (`scalalib_native0.5_3`, `javalib` — 3,503 files incl. files/sockets/processes/threads) |
| Matches project's compatibility target | no (Scala.js subset; JS numeric/string semantics, single-threaded) | **yes by construction** |
| Compiler integration | built into dotc (`-scalajs`) | `nscplugin` (plugin jar per Scala version, published for 3.9.0) |
| Prior art to reuse | `sjsir-interpreter` design, semantics doc | SN toolchain code (reachability, lowering rules) |

Conclusion: SJSIR is the *better-specified and more stable* IR and the easier one to interpret in isolation, but it
drags in JavaScript host semantics and lacks the I/O and threading javalib the project needs. NIR gives the exact
Scala Native subset and the right libraries at the price of a low-level memory model. A hybrid is worth noting: the
sjsir-interpreter's compile-to-closures design and the SJSIR semantics document are directly reusable as *design
references* for an NIR interpreter (numeric semantics, `Long`, string ops, `isInstanceOf` rules are the same language semantics).

---

## 5. Prior art: fast Scala scripting and non-JVM execution

| Project | What it is | Relevance | Status |
|---|---|---|---|
| **Scala CLI** (VirtusLab) | Native-image launcher; compilation offloaded to Bloop (JVM); `--native` builds a Scala Native binary via clang; `package --native-image` for GraalVM images; Ammonite REPL integration removed (deprecated in 1.13). Scala 3.9 announcement: experimental WebAssembly support in Scala CLI | today's official "scripting" path; JVM start-up (~0.5 s) for compile server + full LLVM link per change | [V] docs/release notes |
| `instant-scala` (jatcwang) | wrapper caching GraalVM-built binaries of scripts for instant start-up | shows demand; not an interpreter | [U] |
| **scalino** (coreyoconnor / lolgab, 2026) | "Write, build, and run Scala 3 without installing a JVM": dotc **self-hosted on Scala Native** (patched scala3 + scala-native sources pinned in `versions.env`), `scalino-lsp`, `scalino-linkdriver` (NIR→native, formerly GraalVM native-image, self-hosted since 2026-09-10), a mini scala-cli CLI; requires only `clang` (+ `cs` for deps). **Macros are expanded by a from-scratch TASTy tree interpreter** patched into `dotty/tools/dotc/quoted/Interpreter.scala` (intrinsics for primitives/String/collections, direct calls into `QuotesImpl`; cross-module macros work because TASTy pickles full bodies; gaps: structural quote-type patterns). `docs/findings.md` documents blockers: `jrt:/` (fixed by extracting `java.base` to a jar), no dynamic plugin loading, artifact naming landmines | strongest evidence a JVM-free Scala 3 toolchain is practical; a native dotc front-end for the interpreter | [V] README + findings (fetched 2026-09-24) |
| `shardulc/tasty-interpreter` | "Reference interpreter for TASTy" in Scala on top of **tasty-query**; 4 source files (~27 KB: `Evaluators`, `Objects`, `TypeEvaluators`, `Interpreter`); tests for classes/objects, closures, inheritance, mixins; blog "Building a TASTy interpreter" | small; scalino judged it "incomplete/unmaintained" | [V] tree, [U] status |
| dotty `tests/old-tasty-interpreter-prototype` | 537 LOC `TreeInterpreter` over `Quotes.reflect` with a JVM back-end that uses reflection/`java.lang.reflect.Proxy` for precompiled classes; `notes.md` lists unfinished design items | historical; shows the "interpret TASTy, delegate precompiled to host" design | [V] |
| `scalacenter/tasty-query` | compiler-independent semantic model of TASTy (JVM + Scala.js); used by TASTy-MiMa | the natural front-end library if a TASTy-level interpreter is attempted (all of the Scala 3.9 stdlib is now compiled by Scala 3 and therefore has TASTy bodies) | [V] |
| GraalVM **Espresso** (Java on Truffle) | JVM bytecode interpreter on Truffle (Java 8/11/17/21, passes JCK, GraalVM 25.x); "can run Scala REPL" | proves interpreting JVM bytecode + JDK works; heavyweight, GraalVM-bound | [U] |
| **Rust JVMs**: `theseus-rs/ristretto` (all opcodes incl. `invokedynamic`, JIT for x86-64/aarch64/s390x/riscv64, concurrent GC, runtime classes from AWS Corretto, ~1,800 commits, "hundreds of native methods" unimplemented → `UnsatisfiedLinkError`); `andreabergia/rjvm` (toy; runs OpenJDK 7 `rt.jar` classes; no threads/reflection); `hextriclosan/rusty-jvm` (Java 25 features, GC/threads planned); `lpoulain/Rust-JVM`; `douchuan/jvm` (LLVM JIT) | none documents running Scala; Scala's stdlib needs `invokedynamic`/`LambdaMetaFactory`, `sun.misc.Unsafe`, reflection for `ClassTag`s etc. | [V] READMEs, [I] Scala applicability |
| Ammonite 3.0.8 (2026-01) | JVM REPL/scripts, supports Scala 3.x | JVM start-up; not a non-JVM path | [U] |
| GraalVM native-image of dotc | scalino's first packaging (blockers documented above); scalafmt-native | works, but each compile still runs the whole compiler; no run-time win beyond start-up | [V] |

---

## 6. Recommendations for the interpreter project **[I]**

1. **Adopt NIR (unlowered, compat 6 / SN 0.5.12) as the execution IR.** It is post-erasure, closed-world and matches the
   Scala Native subset by construction; precompiled `scalalib_native0.5_3/3.9.0+0.5.12`, `javalib`, `nativelib`,
   `auxlib`, `clib`, `posixlib` jars provide the whole library stack as `.nir`. User code compiles with unmodified
   dotc + `nscplugin_3.9.0` (JVM-hosted today; scalino's native dotc is a JVM-free option).
2. **Design the VM as "NIR + small C machine"**: Rust deserializer with lazy per-definition loading (the prelude offset
   table supports it), SSA register interpreter over block parameters, objects laid out like SN's `MemoryLayout` (rtti
   header, field layout), a byte-addressable native heap for `Ptr`/`Stackalloc`/`Zone`, and `libffi`-based calls for
   `Declare(isExtern)`. Link SN's own C runtime (`nativelib` resources, GC = Boehm first) so `scalanative_*` symbols,
   monitors, `delimcc`, zlib, RE2-free regex (pure Scala) and platform code work unchanged; move to a precise GC later
   using the typed SSA locals as roots.
3. **Reuse `Lower.scala` as the semantic spec** for null checks, bounds checks, division-by-zero, `is/as` on arrays and
   boxed primitives, and `LinktimeIf` resolution; reuse the SJSIR semantics document and `sjsir-interpreter` for the
   language-level semantics that both IRs share.
4. **Conformance ladder**: SN `unit-tests/shared/scala/` (172) → javalib packages (5,479) → dotty `tests/run`
   filtered (~1,800 programs, `.check` diff) → `scala-partest-junit-tests` (~2,700) → Scala.js shared suite (2,713) →
   ecosystem libraries. Drive JUnit via the generated `Bootstrapper`s with a Rust driver; optionally implement the sbt
   `test-runner` TCP protocol later.
5. **Risks to track**: NIR compat bump in Scala Native 0.6; multithreading semantics in javalib (`Fence`, monitors,
   safepoints) — start in single-threaded link mode; Scala 3.8+ artifact naming (`scalalib_native0.5_3`, not
   `scala3lib`); macro/quote-at-runtime tests are out of scope.

---

## Sources

Scala Native
- Releases: https://github.com/scala-native/scala-native/releases , 0.5.12: https://github.com/scala-native/scala-native/releases/tag/v0.5.12 , 0.5.11: https://github.com/scala-native/scala-native/releases/tag/v0.5.11 , 0.5.0 changelog: https://scala-native.org/en/latest/changelog/0.5.x/0.5.0.html
- Maven Central listings: https://repo1.maven.org/maven2/org/scala-native/nscplugin_3.9.0/ , https://repo1.maven.org/maven2/org/scala-native/scalalib_native0.5_3/ , https://repo1.maven.org/maven2/org/scala-native/scala3lib_native0.5_3/ , https://repo1.maven.org/maven2/org/scala-native/javalib_native0.5_3/0.5.12/ , https://repo1.maven.org/maven2/org/scala-native/nativelib_native0.5_3/0.5.12/
- Repo (main, 2026-09-24): https://github.com/scala-native/scala-native — `nir/src/main/scala/scala/scalanative/nir/{Versions,Types,Vals,Insts,Ops,Next,Defns,Attrs,Sig,Global,Rt,Bins,Comps,Convs,Linktime}.scala`, `nir/.../serialization/{Prelude,BinaryDeserializer,BinarySerializer,Tags}.scala`, `nscplugin/src/main/scala-3/scala/scalanative/nscplugin/{GenNIR,NirGenExpr,NirGenType,PrepNativeInterop,PostInlineNativeInterop}.scala`, `tools/src/main/scala/scala/scalanative/{linker/Reach.scala,linker/ClassLoader.scala,codegen/RuntimeTypeInformation.scala,codegen/Lower.scala}`, `nativelib/src/main/resources/scala-native/{eh.cpp,dyndispatch.c,module_load.c,gc/}`, `project/{Build.scala,Commands.scala}`, `.github/workflows/run-tests-linux.yml`, `unit-tests/`, `unit-tests-ext/`, `junit-test/`, `junit-runtime/`, `junit-plugin/`, `test-interface/`, `test-runner/src/main/scala/scala/scalanative/testinterface/NativeRunnerRPC.scala`, `scala-partest/`, `scala-partest-tests/src/test/resources/scala/tools/partest/scalanative/2.13.18/DenylistedTests.txt`, `scala-partest-junit-tests/src/test/resources/2.13.18/DenylistedTests.txt`
- Docs: NIR https://scala-native.org/en/latest/contrib/nir.html ; mangling https://scala-native.org/en/latest/contrib/mangling.html ; build guide https://scala-native.org/en/latest/contrib/build.html ; compiler plugin https://scala-native.org/en/latest/contrib/compiler.html ; GC safepoint trampoline (docs/contrib/gc-safepoint-trampoline.md) ; javalib https://scala-native.org/en/latest/lib/javalib.html ; language semantics https://scala-native.org/en/latest/user/lang.html ; interop https://scala-native.org/en/latest/user/interop.html ; runtime/GC settings https://scala-native.org/en/latest/user/runtime.html ; sbt settings https://scala-native.org/en/latest/user/sbt.html ; testing https://scala-native.org/en/latest/user/testing.html ; Interflow blog https://scala-native.org/en/latest/blog/interflow.html ; Interflow paper https://dl.acm.org/doi/10.1145/3241653.3241660
- Partest support PR: https://github.com/scala-native/scala-native/pull/2154 ; reflective instantiation: https://contributors.scala-lang.org/t/scala-native-now-supports-reflective-instantiation/4198 , https://github.com/portable-scala/portable-scala-reflect
- GC issues (multithreaded safepoints): https://github.com/scala-native/scala-native/issues/5051 , https://github.com/scala-native/scala-native/issues/5046

Scala 3 / dotty
- Scala 3.9 announcement: https://scala-lang.org/news/3.9/ ; testing docs: https://github.com/scala/scala3/blob/main/docs/_docs/contributing/testing.md ; `tests/run`: https://github.com/scala/scala3/tree/main/tests/run ; `tests/old-tasty-interpreter-prototype`; community build: https://github.com/scala/scala3/blob/main/community-build/src/scala/dotty/communitybuild/projects.scala
- scala/scala 2.13.x `test/junit`, `test/files/{run,pos,neg}`: https://github.com/scala/scala/tree/2.13.x/test

Scala.js
- IR sources: https://github.com/scala-js/scala-js/tree/main/ir/shared/src/main/scala/org/scalajs/ir (`Trees.scala`, `Types.scala`, `Serializers.scala`, `ScalaJSVersions.scala`, `Tags.scala`)
- IR semantics: https://chara.epfl.ch/~doeraene/sjsir-semantics/ (also http://lampwww.epfl.ch/~doeraene/thesis/sjsir-semantics/ ; thesis https://lampwww.epfl.ch/~doeraene/thesis/doeraene-thesis-2018-cross-platform-language-design.pdf)
- sjsir-interpreter: https://github.com/sjrd/sjsir-interpreter (README, `build.sbt`, `core/src/main/scala/org/scalajs/sjsirinterpreter/core/{Compiler,Executor,Nodes,Interpreter,LambdaSynthesizer}.scala`)
- Wasm backend: https://www.scala-js.org/doc/project/webassembly.html , https://github.com/scala-js/scala-js/issues/4928
- test-suite: https://github.com/scala-js/scala-js/tree/main/test-suite

Prior art
- Scala CLI: https://scala-cli.virtuslab.org/docs/cookbooks/introduction/instant-startup-scala-scripts/ , https://scala-cli.virtuslab.org/docs/release_notes/ , https://github.com/VirtusLab/scala-cli/releases/tag/v1.15.0 ; instant-scala https://github.com/jatcwang/instant-scala
- scalino: https://github.com/coreyoconnor/scalino , https://github.com/lolgab/scalino (README, `docs/findings.md`)
- tasty-interpreter: https://github.com/shardulc/tasty-interpreter , blog https://etaoin-shrdlu.xyz/writing/building-tasty-interpreter.php ; tasty-query: https://github.com/scalacenter/tasty-query
- Espresso: https://www.graalvm.org/latest/reference-manual/espresso/
- Rust JVMs: https://github.com/theseus-rs/ristretto , https://github.com/andreabergia/rjvm (+ https://andreabergia.com/blog/2023/07/i-have-written-a-jvm-in-rust/), https://github.com/hextriclosan/rusty-jvm , https://github.com/lpoulain/Rust-JVM , https://github.com/douchuan/jvm
- Ammonite: https://github.com/com-lihaoyi/Ammonite/releases
