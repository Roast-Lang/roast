# Roast – Near-Term Roadmap (Dec 2025)

## 0–2 days (stabilize current breakages)

- Fix iterator pipeline: ensure `MAKE_ITER` uses `roast_make_iter` in bytecode/LLVM; verify `ForIter` lowers to `__next__` calls and returns `StopIteration` correctly. Target: `tests/test_custom_iterator.roast` prints loop body.
- Unskip tests: run `roastc test tests` and enable real execution; document remaining skips.
- Remove `todo!` catch-alls in `crates/codegen/src/bytecode.rs` for Attr/Await/FloorDiv/In/NotIn/Slice/Set/Lambda/Slice proj; emit real ops or explicit errors.
- Add debug visibility: temporary logging behind feature flag around iterator creation/next to validate fix; strip before merge.

## 3–7 days (finish core gaps)

- Async execution in VM: implement `OpCode::Await`, coroutine protocol, and bridge to runtime executor; support `async for/with`. Add regression tests in `tests/test_async.roast`.
- Debugger wiring: add VM step hook and expose frames; connect breakpoints to interpreter loop.
- Cranelift calls: implement function invocation, string/object creation, and runtime linkage; add a minimal native smoke test (e.g., `fib(10)`).
- Test hygiene: add CI job to run `roastc test tests` + a focused iterator/async/native matrix; fail on skipped tests without allowlist.

## 1–3 weeks (feature completeness)

- F-string evaluation end-to-end (lexer already recognizes) with tests.
- Where-clauses & variance parsing/checking for generics.
- Python interop MVP: `py:` module resolution with simple CPython call-through via PyO3; stub generation for imported modules.
- LSP cross-module go-to-definition and basic code actions (organize imports, add missing import).

## Tech debt & quality

- Audit runtime/LLVM function declarations to ensure parity (`roast_make_iter` newly added); generate a single source of truth or a compile-time check.
- Replace ad hoc `todo!`s with structured errors and tracked tickets.
- Document iterator/generator semantics and ownership rules in `docs/` to prevent regressions.
