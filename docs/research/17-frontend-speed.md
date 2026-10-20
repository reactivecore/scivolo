# 17 — Frontend speed: where the edit-then-run time of `moderate.scala` goes

Date: 2026-10-14. Claims are [V] verified by measuring or reading the code, [I] inferred. Measured
on a throwaway branch (`83-typer-f1` + `82-f-interpolator`, before their merge), release build, M4
Pro under heavy load from other builds (load average 57–145 during every run; minima over 6–15
runs are quoted, medians are noise). Paths: `T/` = `crates/tasty/src/`, `Y/` = `crates/typer/src/`,
`L/` = `crates/lower/src/`, `S/` = `crates/scivolo/src/`. The timing laps, call counters and the
in-process compile loop used below were throwaway instrumentation, not in the repository; the
profiles are macOS `sample` runs of that loop (inclusive per function, recursion counted once).

**Recommendation.** The frontend's ~70 ms are not one hot spot but four avoidable algorithmic
costs (junk type variables, by-name member lookup, uncached member types, uncached erasure) plus
fixed costs (whole-file TASTy decoding, eager `Definitions`, file writing). Four prototype fixes of
345 lines cut the in-process compile of `moderate.scala` from 62.7 to 30.3 ms with every tasty,
typer, lower and scivolo test passing [V]; the MR plan in §5 carries that to an estimated edit-then-run
of 45–50 ms. The VM's own 24 ms (cached run) is the other half of the budget and stays as it is.

## 1. Baseline (branch before any fix)

Edit-then-run, `scivolo FILE.scala` in strict mode with a private `SCIVOLO_CACHE`, each miss on a
freshly edited copy, load 57–69 [V]:

| program | miss min / median | hit min / median | miss − hit |
|---|---:|---:|---:|
| hello | 19.3 / 19.8 ms | 13.4 / 13.5 ms | 6 ms |
| caseclasses | 46.0 / 46.9 ms | 17.4 / 18.2 ms | 29 ms |
| moderate | 90.7 / 114.9 ms | 24.1 / 34.0 ms | 67 ms |

Laps of a moderate miss (min over 8, load 69; `S/main.rs:run`, `S/frontend.rs:compile`,
`L/compile.rs`, `Y/lib.rs:type_unit`, `L/lib.rs:lower`, `L/phases/mod.rs:run`) [V]:

| lap | ms | notes |
|---|---:|---|
| open library image (`S/libs.rs:18`) | 0.95 | |
| `Universe::open_image` (`T/symbols/open.rs:28-74`) | 2.7 | sorts 2,031 TASTy paths, enters packages, 4 builtin files (hello: paths 1.1, builtins 0.1–0.3) |
| parse + desugar | 0.30 | |
| `Typer::new` + root context | 0.30 | 10 files loaded by `Defn::new` and builtins |
| `typer.typed` | **37.2** | 163 files loaded, 14.9k symbols, 7.1k types after it |
| zonk + inlining + annotations | 2.5 | |
| `Definitions::new` (`L/definitions/mod.rs:81`) | 0.8 | hello: 1.6–2.9 ms, loads ~40 files (8 primitives, `Character`, `MurmurHash3`, …) |
| lowering phases (`L/phases/mod.rs:run`) | **12.5** | erasure 5.0, lambda_lift 1.4, value classes/specialize/repeated 1.3, select_static/tailrec 0.7, synthetic 0.5, mixin 0.4, bridges 0.3 |
| codegen (`L/codegen/mod.rs:unit`) | **9.2** | NIR encoding itself 0.4 (35 files) |
| write 35 `.nir` files (`S/frontend.rs:73-79`) | 2.1–3.6 | `create_dir_all` + `write` per file |
| `main_classes` (`S/script.rs:135-159`) | 0.9 | reads and decodes every written file again |
| cache prune ×2 (`S/script.rs:59-60`, `libimage/src/cache.rs:35`) | 0.35 | `read_dir` + `symlink_metadata` per entry, 96 entries |
| `Classpath` + `Vm::new` | 1.0 | |
| `run_main` | 18.3 | also on the hit path; hello 7, caseclasses 12 |

Sum of the frontend laps ≈ 68 ms = the miss − hit difference. Process start, image and VM setup
are the same on both paths, so the budget for the frontend is 50 − 24 ≈ 26 ms on moderate.

## 2. Where the typer's 37 ms and the lowering's 22 ms go

Call counters (one miss of moderate) and a 4,291-sample profile of the in-process loop before the
fixes [V]:

| cost centre | evidence |
|---|---|
| **By-name member lookup.** `Universe::decl` (`T/symbols/lookup.rs:47-82`) filters `owner.decls` by string compare, collects a `Vec` and `encode`s the name (`:66`) on every call; `member_where` (`T/symbols/classes.rs:69-87`) calls it once per base class of the linearization, and `linearization`/`derives_from` (`:44-65`) clone the linearization `Vec` on every call. | 104,894 `decl` calls, 32.5 % inclusive; `decl` is the top `Vec::from_iter` caller (502 of 893 samples); `memcmp` 4.3 % |
| **By-name type references resolved again and again.** Library types keep `Type::TypeRef(prefix, name)` (`T/types/mod.rs:31-33`); `resolve` (`T/symbols/resolve.rs:14-32`) is never memoized, and `class_symbol_within` (`T/symbols/classes.rs:99-119`), `class_param` (`T/symbols/seen.rs:18-29`, called by the `as_seen_from` closure for every node, `:132-149`), `erase` (`T/erasure/mod.rs:69-79`) and `Cmp::named` (`T/subtype/refs.rs:40-55`) all go through it. | 84,841 `resolve` calls, 29.3 % inclusive; callers: `class_param` 453, `class_symbol_within` 437, `erase` 119, `named` 89 samples |
| **Junk type variables.** `with_type_vars` (`Y/typer/type_apply.rs:46-66`) makes fresh variables for every implicit candidate and overload alternative; `Constraint::rollback` (`Y/constraint/mod.rs:165-170`) restores bounds but never removes variables. At the end of `moderate`'s main method `interpolate` (`Y/typer/interpolate.rs:18-59`) sees 1,263 variables, 810 of them pending, and `instantiate_in_order` (`:123-149`) runs `is_less` (`:153-167`, two `Vec` allocations each) 327,645 times: 10.3 ms in one call. `bound_variances` (`:89-101`) also scans every variable on every `interpolate`. | one `interpolate` of 10.34 ms: 1,263 variables, 810 pending, 327,645 `is_less`; `interpolate` 15.4 % inclusive, `is_less` 10.6 % |
| **Implicit search types every candidate.** `fitting` (`Y/typer/implicits/search.rs:102-127`) calls `try_implicit` (`:132-176`) for each candidate: `candidate_ref`, `with_type_vars` → `method_info` (`Y/typer/seen_from.rs:9-21`: `member_info` → `as_seen_from` → `map_type` over the whole method type, then `resolve_class_params` and `normalized`, `:66-157`, which compares `full_name` strings per `Applied` node, `:87,102`) and `conforms`. No prefilter on the candidate's result type as dotty's `isCompatible`/`eligible` cache does. 23 top-level searches, 1,787 tries. | the search for `Ordering[?]` 11.36 ms, 1,122 tries; `implicit_arg` 25.4 % inclusive, `method_info` 21.2 %, `full_name` 4.2 %, `format` 4.0 % |
| **Erasure per call site.** `Gen::param_types`/`result_type`/`method_sig` (`L/codegen/erased.rs:80-101`) call `erased_method` for every call; `erase_symbol` (`T/erasure/mod.rs:98-131`) rebuilds `full_name`/`binary_name` strings and `underlying` (`:136-150`) looks `AnyVal` up by name on every class. | 17,489 `erase` calls, 12.8 ms; callers `param_types` 210, `result_type` 142, `adapt::erased_type` 114, `sam::abstract_methods` 106, `erasure::calls` 92 samples; `erased_method_as` 16.7 % |
| **Tree walks clone every node.** `map_children` (`Y/tpd/children.rs:31-55`) clones the `Tree` and `to_vec`s every list (`:62-72`); `children` (`:7-17`) allocates a `Vec` per node. `phases::run` (`L/phases/mod.rs:97-179`) walks the unit about 45 times. | `Tree::fields` 9.5 %, `map_children` 7.2 %, `walk::subtrees` 1.9 % |
| **Whole-file TASTy decoding.** `Universe::load` (`T/symbols/enter.rs:19-39`) parses the whole AST section into 32-byte nodes (`T/tree.rs:8-16,28-39`) and shows every name as a `String`, for 188 files, 2.1 MB, 244,834 nodes, 30,677 names, 16,144 symbols; most files are loaded only for one member lookup. hello loads 57 files (650 KB) including `java/lang/Character.tasty` (136 KB, 20,925 nodes). | loading 9.3–9.8 ms (`Ast::parse` 4.4–5.1, names 1.4, `enter` 1.8); 9.6 % inclusive |
| **Allocation in general.** `Type` holds `String`s and `Vec`s (`T/types/mod.rs:28-97`); `map_type` clones the node it visits (`T/types/map.rs:18`), 365,410 calls; `Types::intern` hashes the clone. | `malloc`+`free`+`realloc` ≈ 20 % of samples, `Type::clone` 10.2 %, `Types::intern` 5.6 % |

Fixed versus per-line costs (hello 19 ms, caseclasses 46, moderate 91 edit-then-run) [V]: fixed are
image 1 + universe 2.7 + `Definitions` 1.6–2.9 + VM 1 + prune/key 0.5 ≈ 7 ms plus process start; the
rest scales with the library surface the script touches (files loaded 57 / 75 / 188, symbols
4.9k / 8.6k / 16.8k), not with its line count: caseclasses types in 3.8 ms but lowers in 18.6 ms
(erasure 3.5, codegen 8.0), moderate types in 37 ms because of its five implicit searches and
the junk variables they leave.

## 3. Prototype fixes and their measured effect

Four fixes, 345 lines, not committed, each alone small enough for one MR; tests:
`cargo test --release -p tasty -p typer -p lower -p scivolo` all green (the lowered programs compare
NIR byte for byte in strict mode) [V].

| fix | where | moderate counters | in-process compile min (15 runs) |
|---|---|---|---|
| — | baseline | `decl` 104,894, `resolve` 84,841, `interpolate` 10.3 ms | 62.7 ms (57.7 over 150) |
| A. discard the type variables of a rolled-back candidate: `Entry.discarded`, `discard_from` (`Y/constraint/mod.rs`), set in `fitting` (`Y/typer/implicits/search.rs`), skipped by `interpolate` and `bound_variances` | 25 lines | the slow `interpolate` gone (< 0.2 ms); `Ordering[(Int,String)]` 11.4 → 4.9 ms | |
| B. per-owner name index `by_name` kept by `add`/`enter`/`unenter`/`rename` (`T/symbols/mod.rs`, `define.rs`), `decl` uses it; `resolve_ref(t)` memo of positive resolutions keyed by the `TypeRef` type id (`T/symbols/resolve.rs`), used by `class_symbol_within`, `class_param`, `base_type`, `erase`, `named`, `widen_once`, `referenced_alias`, `bound_of`, `normalized` | 60 lines | `decl` 21,511, `resolve` 4,466, `member_where` 489 | A+B: **41.0 ms** |
| C. `method_info` memo in the typer keyed by the function type, dropped when the universe's `generation` changes (bumped by `set_info`, `set_parents`, `define`, `enter`, `unenter`, `rename`, `set_flags`) | 30 lines | implicit searches 19.4 → 12.6 ms; `as_seen_from` 9,047 → 5,245 calls | |
| D. `erased_method_as` memo in the universe, cleared on every bump | 10 lines | `erase` 17,489 → 5,267 calls, 5.7 → 1.6 ms; codegen 6.1 → 3.2 ms | A–D: **30.3 ms** (29.3 over 200) |

Edit-then-run laps after A–D (load 100–125, so absolute numbers are inflated; the deltas hold) [V]:
`typer.typed` 37.2 → 17.3 ms, `lower` 22.6 → 12.1 (phases 8.0, codegen 3.2), the whole
`lower::compile` 66.2 → 34.5; caseclasses `lower` 18.6 → 12.2. Scaled to the baseline load,
moderate edit-then-run lands near 58 ms [I].

What remains after A–D (4,280 samples, 29 ms per compile) [V]: TASTy loading 22 % (6.5 ms;
`Ast::parse` 10 %, names 3.8 %, `enter_stat` 4.8 %), implicit search 19 % (`method_info` misses
14 %: `C`'s memo is cleared by every one of the 106 completions), `class_members`
(`Y/typer/members.rs:116-126`: completes every base class and clones the linearization per
`typed_select`) 10 %, `linearization` misses 10 % (725 classes, `lin.contains` is quadratic,
2.8 ms), `Tree::fields` 8.7 %, `normalized`'s `full_name` compares 5.7 % + `format` 5.5 %,
`zonk_trees` 6.8 %, erasure phase 5.9 %, `Definitions::new` 3.4 %, `Universe::of` 1.6 %.

## 4. Options not taken

- **Threads.** CLAUDE.md forbids async and global state, not threads, but nothing overlaps: the
  VM needs the script's NIR, the typer needs the universe, and the fixed costs that could run
  beside parsing (image, universe, `Definitions`) are 4 ms. Not worth a second thread [I].
- **Snapshotting the typer universe in the image** (the heap-snapshot idea for symbols): a
  serialized arena of the symbols, infos, parents and linearizations of a training set of files
  would replace loading (6.5–9 ms), `Universe::of` (2.7) and `Definitions` (1–3) by a mapping;
  but types, infos and `by_tree` hold `String`s and hash maps, so the arenas would have to become
  flat first. Larger than MR 7 below for a similar gain; revisit after MR 7 if moderate is still
  above 50 ms [I].
- **Fusing the lowering's 45 walks into miniphase groups** as dotty does: 3–5 ms, but it changes
  every phase's shape; MR 8 takes the cheaper half (no clone per node) first [I].

## 5. MR plan (each ≤ 700 lines, in order; gains on moderate at baseline load)

1. **Typer: discard rolled-back type variables** (fix A; also `applicability.rs:66-78`,
   `adapt_by_result.rs:140`, `ranking.rs:104` which roll back the same way; `is_less`/`factors`
   without allocation; a test that a failed candidate leaves no pending variable). dotty discards
   them with the candidate's `TyperState` [I]. ~120 lines. −10 ms. Risk: low, tests green.
2. **tasty: name index and memoized references** (fix B plus `derives_from`/`linearization`
   without clones, a hash set in the linearization merge, `encode` only for package lookups,
   `underlying` with a cached `AnyVal`). ~250 lines. −12 ms, caseclasses −3. Risk: the memo must
   be dropped on `unenter`/`rename` (REPL `:reset`, redefinition; covered by `tests/repl`).
3. **Typer + tasty: member-type and erasure memos** (fixes C and D; invalidate the `method_info`
   memo on structural changes only, not on `set_info`, since `method_info` completes its symbol and
   prefix first; `normalized`/`simplify` compare symbols, not `full_name` strings; `class_members`
   without completing library bases). ~250 lines. −10 ms, caseclasses −4. Risk: medium (stale
   member type if a prefix mentions a script alias completed later; the typer golden tests and
   `just typer-checks` guard it).
4. **scivolo: the cache path** (`S/frontend.rs`, `S/script.rs`): one `create_dir_all` per package,
   main classes found in the in-memory NIR bytes instead of re-reading the files, prune only when
   the cache directory changed (a stamp file), `touch` skipped on a miss. ~120 lines. −3 to −4 ms
   on every script. Risk: low.
5. **Fixed costs: package table in the image and lazy `Definitions`** (`libimage` extra blob
   listing TASTy paths per package so `Universe::of` enters packages without sorting 2,031 paths;
   `L/definitions` and `Y/defn.rs` resolve members on first use through `Option` fields). ~300
   lines. −3 to −5 ms on every script (hello 19 → ~14). Risk: low.
6. **Typer: implicit candidate prefilter** (dotty's `isCompatible` on the candidate's declared
   result type with its type parameters as wildcards, before `try_implicit`; cache the eligible
   list per (scope, expected class)). ~250 lines. −5 to −7 ms on moderate. Risk: medium, must not
   drop a candidate dotty keeps (`tests/typer` implicit cases, `just typer-checks`).
7. **tasty: lazy TASTy decoding** (per-file symbol table as the image's extra blob, like the NIR
   member tables of ADR 0005: names, kinds, flags, owners and tree addresses of the entered
   symbols; `Ast::parse` and the name table on first `info`/`body`/`parents`). ~600 lines.
   −4 to −6 ms on moderate, −1.5 on hello. Risk: medium (two readers of one format; the image key
   covers the table version).
8. **Lowering: cheaper walks** (`map_children` on a borrowed tree with a small inline vector of
   children, `children` without `Vec`). ~300 lines. Estimated −3 to −5 ms; measured (#100,
   `bench-edit-base`, 25 runs) about −1 ms: moderate 50.8 → 50.1 ms min, caseclasses 32.1 → 31.3,
   hello unchanged [V]. Erasure's `typed` does not re-walk: on moderate it runs once per node
   (1,306 calls, 1,306 distinct nodes of 7,224) [V]. Split of a moderate compile after it: typer
   23.9 ms, lowering phases 7.6 (erasure 1.7), codegen 3.4 [V]. Risk: low.

Expected total after 1–8: in-process compile 62.7 → ~15 ms, frontend plumbing 8 → 3 ms, so
moderate edit-then-run ≈ 24 (VM, as cached) + 18 + process ≈ 45–50 ms at the baseline load [I];
hello ≈ 14 ms, caseclasses ≈ 30 ms. The VM's 18 ms `run_main` for moderate (7 for hello) is the
next lever beyond the frontend: docs/performance.md item 7, the heap snapshot (#9).
