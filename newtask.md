# New Tasks (actionable)

1. Fix iterator execution in VM/LLVM

- Scope: Ensure `MAKE_ITER` uses `roast_make_iter`; `ForIter` drives `__next__` with StopIteration handling; custom iterators yield values.
- Acceptance: `tests/test_custom_iterator.roast` prints loop body; add regression test; no runtime panics.

2. Unskip tests and surface failures

- Scope: Make `roastc test tests` run real cases; remove blanket skips or document per-test skip reasons.
- Acceptance: CI run shows executed test count >0; list of remaining skips with justification checked into repo.

3. Replace `todo!` arms in bytecode lowering

- Scope: Implement or error clearly for Attr/Await/FloorDiv/In/NotIn/Slice/Set/Lambda/Slice projection in `crates/codegen/src/bytecode.rs`.
- Acceptance: File builds without `todo!`; missing features emit structured diagnostics; add unit coverage for at least Attr, Await, Slice, Set.

4. Add VM async support

- Scope: Implement `OpCode::Await`, coroutine protocol, `async for/with`, executor bridge.
- Acceptance: New async test in `tests/test_async.roast` passes; awaiting a simple coroutine works in VM and CLI run.

5. Wire debugger to VM

- Scope: Add interpreter step hook exposing frames; connect breakpoint manager; basic stepping works.
- Acceptance: DAP client can set breakpoint and hit it in a running Roast program; call stack visible.

6. Cranelift function calls

- Scope: Emit real calls, string/object creation, runtime linkage.
- Acceptance: Native-compiled `fib(10)` returns 55; add smoke test target.

7. Runtime/LLVM declaration parity check

- Scope: Ensure runtime functions (e.g., `roast_make_iter`) declared identically across native_full and LLVM backend; add compile-time guard or generator.
- Acceptance: Build fails if declarations diverge; doc note in `docs/` describing the source of truth.
