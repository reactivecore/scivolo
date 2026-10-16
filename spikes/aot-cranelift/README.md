# Spike: NIR → native code via Cranelift (#113)

Translates Scala Native 0.5.12 NIR (decoded with `crates/nir`) to Cranelift IR and an object
file; runtime operations (module loading, allocation, virtual dispatch, type tests, boxing,
throwing) are calls to the stubs in `c/rt.c`, field offsets are fake, module methods are
devirtualized, unwind edges become checks of a global exception flag. The findings are in
`docs/research/22-native-executables.md`.

```
cargo run --release -- [--opt none|speed] [--eh flag|none] [--verify] [--disasm] [--link] [--run N] OUT.o DIR_OR_JAR...
```

`--link` compiles `c/rt.c` and `c/main.c` with `cc`, links them with the object and prints the
link times; `--run N` runs the binary (`fib(N)` of `tests/programs/fib`). Measured on an M4 Pro
(load 4–10): fib's 14 functions compile in 2–3 ms, link in 26 ms, `fib(30)` runs in 6 ms wall;
the whole library (9,677 files, 87,585 functions, 975k instructions) compiles in 6.5 s on one
core into a 34 MB object that ld64 links in 80–110 ms.
