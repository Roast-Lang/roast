# 🔥 Roast v1.0 Production Release Plan

> **Document Version:** 1.0  
> **Date:** December 25, 2025  
> **Target:** Production-ready v1.0 release  
> **Assumed Resources:** Small core team (1-3 developers)

---

## 1. Executive Summary

### Why Roast is NOT Production-Ready Today

Roast has achieved a remarkable technical foundation—a working LLVM backend that delivers Rust-competitive performance (fib(40) in 0.37s). However, **three critical safety gaps** absolutely prevent production use:

1. **Memory safety is advisory-only.** The borrow checker runs but its results are ignored by the LLVM backend. Use-after-free and double-free are possible in "safe" Roast code.

2. **Data races are possible.** Send/Sync traits exist as scaffolds but are not enforced. Concurrent code can have undefined behavior.

3. **No ecosystem exists.** The package registry is not deployed. Users cannot share or discover packages.

### What Must Change for v1.0

| Category | Current State | v1.0 Requirement |
|----------|--------------|------------------|
| Memory safety | Advisory | **Enforced at compile time** |
| Thread safety | None | **Enforced via Send/Sync** |
| Package registry | Code exists | **Deployed and operational** |
| Platform support | Linux only | Linux + Windows (macOS desirable) |
| Developer tooling | Basic | Cross-module LSP navigation |

### Estimated Effort

| Phase | Scope | Effort | Calendar Time |
|-------|-------|--------|---------------|
| Safety (blocking) | Borrow checker + Send/Sync | 3-4 person-weeks | 4-6 weeks |
| Tooling | Registry + LSP | 4-6 person-weeks | 6-8 weeks |
| Ecosystem | Windows + docs | 3-4 person-weeks | 4-6 weeks |
| Hardening | Security audit + polish | 2-3 person-weeks | 3-4 weeks |
| **Total** | | **12-17 person-weeks** | **~4-6 months** |

---

## 2. Non-Negotiable Blocking Issues

### Blocker #1: Memory Safety Not Enforced

| Aspect | Detail |
|--------|--------|
| **Description** | Borrow checker runs but LLVM backend ignores ownership annotations |
| **Root Cause** | `borrowck` crate results not passed to `llvm_backend` during codegen |
| **Real-World Risk** | Use-after-free, double-free, memory corruption, security vulnerabilities |
| **Severity** | **CRITICAL** - Language is fundamentally unsafe |

**Technical Fix Required:**
1. Pass `BorrowCheckResult` from borrowck to codegen pipeline
2. In `llvm_backend/src/lib.rs`, reject functions that fail borrow check
3. For owned values, generate deallocation calls at end of scope
4. For borrowed references, ensure lifetimes are respected
5. Add compiler flag `--unsafe-allow-borrow-violations` for escape hatch

**Estimated Effort:** 2-3 weeks (experienced compiler engineer)

---

### Blocker #2: Data Races Possible in Concurrent Code

| Aspect | Detail |
|--------|--------|
| **Description** | Send/Sync traits exist but are not enforced by type checker or codegen |
| **Root Cause** | `spawn()` and channel operations don't verify types implement Send |
| **Real-World Risk** | Data races causing undefined behavior, memory corruption, security holes |
| **Severity** | **CRITICAL** - Concurrent code is fundamentally unsafe |

**Technical Fix Required:**
1. In `typer/src/builtins.rs`, add Send/Sync trait bounds to `spawn()`, `channel()`, etc.
2. In `typer/src/types.rs`, implement `is_send()` and `is_sync()` for all types
3. Mark `Rc<T>` as `!Send`, `Arc<T>` as `Send + Sync`
4. Reject attempts to send non-Send types across threads
5. Add clear error messages explaining thread safety requirements

**Estimated Effort:** 1-2 weeks

---

### Blocker #3: No Central Package Registry

| Aspect | Detail |
|--------|--------|
| **Description** | `kitchen` client exists but no registry server is deployed |
| **Root Cause** | Server implementation exists in `registry-server/` but never deployed |
| **Real-World Risk** | No ecosystem, no code sharing, users isolated |
| **Severity** | **HIGH** - Language unusable for real projects |

**Technical Fix Required:**
1. Deploy `registry-server` to cloud (Cloudflare Workers, already implemented)
2. Configure domain `registry.roast-lang.org`
3. Set up R2 storage for package artifacts
4. Set up KV for metadata
5. Test publish/download flow end-to-end
6. Document publishing workflow

**Estimated Effort:** 1-2 weeks (DevOps + testing)

---

## 3. Memory Safety & Concurrency Enforcement Plan

### 3.1 Wire Borrow Checker to LLVM Codegen

**Step 1: Modify Compilation Pipeline**

```
Current:  AST → HIR → MIR → Borrowck (results discarded) → LLVM
Required: AST → HIR → MIR → Borrowck → LLVM (with borrowck results)
```

**File Changes:**

| File | Change |
|------|--------|
| `crates/borrowck/src/lib.rs` | Return `BorrowCheckResult` with lifetime annotations |
| `crates/cli/src/main.rs` | Pass borrowck result to LLVM backend |
| `crates/llvm_backend/src/lib.rs` | Accept and use borrowck result |
| `crates/llvm_backend/src/lib.rs` | Reject compilation if borrowck errors exist |

**Step 2: LLVM-Level Enforcement**

For owned values:
```rust
// In llvm_backend/src/lib.rs, at scope exit:
// For each owned variable being dropped:
self.ir.push_str(&format!("  call void @roast_dealloc(i8* {})\n", ptr));
```

For borrowed references:
- Track lifetime end points from borrowck
- Ensure no access after lifetime ends (compile error)

**Step 3: Unsafe Escape Hatch**

Add `@unsafe` decorator for functions that violate borrowing rules:
```python
@unsafe
def raw_pointer_magic(ptr: int) -> int:
    # Compiler trusts programmer here
    ...
```

### 3.2 Enforce Send/Sync Thread Safety

**Step 1: Define Send/Sync Properties**

| Type | Send | Sync |
|------|------|------|
| `int`, `float`, `bool`, `str` | ✅ | ✅ |
| `list[T]` | if T: Send | if T: Sync |
| `dict[K, V]` | if K, V: Send | if K, V: Sync |
| `Rc[T]` | ❌ | ❌ |
| `Arc[T]` | ✅ | if T: Sync |
| Classes | ❌ by default | ❌ by default |

**Step 2: Type Checker Enforcement**

In `crates/typer/src/check.rs`:
```rust
// When type-checking spawn() call:
if !self.is_send(&closure_captured_types) {
    return Err(TypeError::NotSend(closure_captured_types));
}
```

**Step 3: Error Messages**

```
error[E0277]: `Rc[User]` cannot be sent between threads safely
   --> src/main.roast:15:5
    |
15  |     spawn(lambda: process(user))
    |     ^^^^^ `Rc[User]` is not Send
    |
    = help: use `Arc[User]` for thread-safe reference counting
```

### 3.3 Testing Strategy

| Test Category | Description | Count |
|---------------|-------------|-------|
| Use-after-free rejection | Compile should fail | 10+ |
| Double-free rejection | Compile should fail | 5+ |
| Borrow violation rejection | Compile should fail | 15+ |
| Send/Sync violations | Compile should fail | 10+ |
| Valid code still compiles | Regression tests | All existing |

---

## 4. Compiler & Runtime Hardening

### 4.1 Required Compiler Invariants

| Invariant | Enforcement |
|-----------|-------------|
| All borrow check errors prevent compilation | Error on any violation |
| All type errors prevent compilation | Already enforced |
| No undefined behavior in safe code | Bounds checks, null checks |
| Deterministic compilation | No random seeds, sorted iterations |

### 4.2 Undefined Behavior Elimination

| UB Source | Fix |
|-----------|-----|
| Use-after-free | Borrow checker enforcement |
| Data races | Send/Sync enforcement |
| Integer overflow | Option: Add `--checked-arithmetic` flag |
| Null pointer dereference | Optional types already prevent this |
| Out-of-bounds access | Runtime bounds checking (already exists) |

### 4.3 Runtime Safety Checks

| Check | Debug Build | Release Build |
|-------|-------------|---------------|
| Bounds checking | ✅ Always | ✅ Always (configurable) |
| Null checks | ✅ Always | ✅ Always |
| Integer overflow | ✅ Panic | ❌ Wrap (configurable) |
| Stack overflow | ✅ Detect | ✅ Detect |

### 4.4 Incremental Compilation Design (High-Level)

**MVP Approach:**
1. Hash each function's AST + dependencies
2. Cache compiled object files per function
3. On recompile, check if hash changed
4. Link cached objects where possible

**Estimated Effort:** 3-4 weeks (can be post-v1.0)

---

## 5. Tooling & Developer Experience Plan

### Priority Order (Build First)

1. **Registry deployment** (Week 1-2) - Enables ecosystem
2. **Cross-module LSP** (Week 3-6) - Enables IDE experience
3. **Debugger integration** (Week 7-8) - Professional development
4. **Error improvements** (Week 9-10) - User friendliness

### 5.1 Package Manager Stabilization

| Task | Priority | Effort |
|------|----------|--------|
| Deploy registry server | P0 | 1 week |
| Test publish/install flow | P0 | 3 days |
| Document publishing | P0 | 2 days |
| Add `--dry-run` to publish | P1 | 1 day |
| Improve error messages | P1 | 2 days |

### 5.2 Central Registry Architecture (MVP)

```
┌─────────────────────────────────────────────┐
│          Cloudflare Workers                  │
│  ┌─────────────────────────────────────┐    │
│  │     registry-server (TypeScript)    │    │
│  │  - /api/v1/packages (CRUD)          │    │
│  │  - /api/v1/search                   │    │
│  │  - /api/v1/auth                     │    │
│  └─────────────────────────────────────┘    │
│         │              │                     │
│         ▼              ▼                     │
│   ┌──────────┐   ┌──────────┐               │
│   │    KV    │   │    R2    │               │
│   │ Metadata │   │ Packages │               │
│   └──────────┘   └──────────┘               │
└─────────────────────────────────────────────┘
```

**Security Requirements:**
- Ed25519 package signing (already implemented)
- SHA256 checksums (already implemented)
- Rate limiting on publish
- Reserved namespace protection

### 5.3 LSP Cross-Module Indexing

**Implementation Plan:**

1. **Build module index** (Week 1)
   - Parse all `.roast` files in project
   - Build symbol table with locations
   - Persist index to disk

2. **Wire to go-to-definition** (Week 2)
   - Look up symbol in module index
   - Return cross-module location

3. **Wire to find-references** (Week 3)
   - Scan all modules for symbol usage
   - Return list of locations

4. **Optimize for large projects** (Week 4)
   - Incremental index updates
   - Background indexing

### 5.4 Error Diagnostics Improvements

| Improvement | Effort | Impact |
|-------------|--------|--------|
| "Did you mean?" suggestions | 2 days | High |
| Color-coded output | 1 day | Medium |
| Multi-error reporting | 2 days | Medium |
| Context snippets | 1 day | Medium |

---

## 6. Standard Library & API Stability Plan

### 6.1 Criteria for Stdlib Inclusion

| Criterion | Requirement |
|-----------|-------------|
| General utility | Used by >20% of projects |
| Platform independence | Works on all targets |
| Security | No unsafe code |
| Stability | API finalized |
| Testing | >90% coverage |

### 6.2 API Versioning Rules

**Semantic Versioning (Post v1.0):**
- Major: Breaking changes
- Minor: New features, backward compatible
- Patch: Bug fixes only

**Deprecation Policy:**
1. Mark with `@deprecated("Use X instead")`
2. Warn for 2 minor versions
3. Remove in next major version

### 6.3 What Must Be Frozen Before v1.0

| Module | Status | Action |
|--------|--------|--------|
| `fs`, `path`, `io` | Stable | Freeze |
| `json`, `base64`, `hex` | Stable | Freeze |
| `sync`, `thread`, `channel` | Stable | Freeze |
| `testing` | Stable | Freeze |
| `http`, `web` | Review | Minor changes possible |
| `database` | Scaffold | Mark experimental |
| `gpu` | Active | Mark experimental |

---

## 7. Security Hardening Plan

### 7.1 Threat Model

| Threat | Likelihood | Impact | Mitigation |
|--------|------------|--------|------------|
| Memory corruption | High (currently) | Critical | Borrow checker enforcement |
| Data races | High (currently) | Critical | Send/Sync enforcement |
| Malicious packages | Medium | High | Package signing, checksums |
| Supply chain attack | Medium | High | Registry security, SBOM |
| FFI escape | Low | High | Document risks, audit FFI |

### 7.2 Mandatory Security Features Before Release

| Feature | Status | Required Action |
|---------|--------|-----------------|
| Memory safety enforcement | ❌ | Wire borrow checker |
| Thread safety enforcement | ❌ | Wire Send/Sync |
| Package signing | ✅ | Verify working |
| Checksum verification | ✅ | Verify working |
| Bounds checking | ✅ | Verify working |
| Integer overflow detection | ⚠️ | Add debug mode |

### 7.3 FFI Safety Strategy

1. **Mark all FFI as unsafe by default**
   ```python
   @unsafe  # Required for FFI calls
   def call_c_function() -> None:
       cffi.call("libc", "puts", "Hello")
   ```

2. **Document FFI risks in prominent location**

3. **Provide safe wrappers for common operations**

### 7.4 Audit Readiness Checklist

| Item | Status |
|------|--------|
| Memory safety enforced | ⬜ |
| Thread safety enforced | ⬜ |
| No known CVEs | ⬜ |
| Fuzz testing completed | ⬜ |
| Dependency audit | ⬜ |
| SBOM generated | ⬜ |

---

## 8. Ecosystem & Governance Strategy

### 8.1 Eliminating Bus Factor Risk

**Immediate Actions:**
1. Document all architecture decisions in `ARCHITECTURE.md`
2. Create `CONTRIBUTING.md` with clear guidelines
3. Set up GitHub discussions for community
4. Identify 1-2 potential co-maintainers

**Contribution Paths:**
| Area | Entry Barrier | Need |
|------|---------------|------|
| Documentation | Low | High |
| Stdlib modules | Medium | Medium |
| Compiler internals | High | Low (core team) |

### 8.2 Contribution Process

**For Minor Changes:**
1. Open issue describing change
2. Submit PR
3. Core team reviews within 1 week
4. Merge or request changes

**For Significant Changes (RFC Process):**
1. Open RFC issue with template
2. Community discussion (2 weeks minimum)
3. Core team decision
4. Implementation if accepted

### 8.3 Release Cadence

| Release Type | Frequency | Content |
|--------------|-----------|---------|
| Patch (1.0.x) | As needed | Bug fixes |
| Minor (1.x.0) | Quarterly | New features |
| Major (x.0.0) | Yearly | Breaking changes |

### 8.4 Long-Term Maintenance Strategy

1. **Funded development:** Seek sponsorship (GitHub Sponsors, Open Collective)
2. **Corporate adoption:** Target internal tools at companies
3. **Academic partnerships:** Compiler courses, research

---

## 9. v1.0 Release Criteria (Go / No-Go Checklist)

### Safety (All Required)
- [ ] Memory safety enforced in LLVM codegen
- [ ] Use-after-free rejected at compile time
- [ ] Double-free rejected at compile time
- [ ] Data races impossible in safe code (Send/Sync)
- [ ] Runtime bounds checking enabled by default

### Tooling (All Required)
- [ ] Package registry deployed and operational
- [ ] Package publish/install working
- [ ] LSP cross-module go-to-definition working
- [ ] Windows build passing CI

### Quality (All Required)
- [ ] All 338+ existing tests passing
- [ ] Memory safety test suite passing (new)
- [ ] Thread safety test suite passing (new)
- [ ] No P0/P1 bugs open
- [ ] Fuzz testing run with no crashes

### Documentation (All Required)
- [ ] Language reference complete
- [ ] Getting started tutorial
- [ ] API docs for stdlib
- [ ] Migration guide from alpha

### Governance (Recommended)
- [ ] CONTRIBUTING.md written
- [ ] Code of Conduct adopted
- [ ] At least 2 maintainers with commit access
- [ ] Security policy published

---

## 10. Phased Roadmap

### Phase 0: Safety (Blocking) - Weeks 1-6

**Goals:**
- Make Roast memory-safe
- Make Roast thread-safe
- Enable ecosystem

**Deliverables:**

| Week | Deliverable | Owner | Verification |
|------|-------------|-------|--------------|
| 1-2 | Borrow checker wired to LLVM | Core | Use-after-free tests fail to compile |
| 3-4 | Send/Sync enforcement | Core | Data race tests fail to compile |
| 5 | Registry deployed | Core | `kitchen publish` works |
| 6 | Safety test suite | Core | All safety tests passing |

**Exit Criteria:**
- [ ] `tests/safety/*.roast` (50+ tests) all pass
- [ ] Registry accepts package uploads
- [ ] No memory-unsafe code compiles in safe mode

---

### Phase 1: Tooling - Weeks 7-14

**Goals:**
- Professional IDE experience
- Cross-platform support
- Reliable debugging

**Deliverables:**

| Week | Deliverable | Owner | Verification |
|------|-------------|-------|--------------|
| 7-8 | Cross-module LSP | Core | Go-to-definition across files |
| 9-10 | Windows CI + support | Core | Windows build passing |
| 11-12 | Debugger DWARF | Core | Breakpoints hit in VS Code |
| 13-14 | Error message improvements | Core | "Did you mean?" working |

**Exit Criteria:**
- [ ] LSP works for multi-file projects
- [ ] Windows binary downloadable
- [ ] Debugger tutorial complete

---

### Phase 2: Ecosystem - Weeks 15-22

**Goals:**
- Enable community packages
- Complete killer features
- Documentation

**Deliverables:**

| Week | Deliverable | Owner | Verification |
|------|-------------|-------|--------------|
| 15-16 | Python FFI complete | Core | `import py:pandas` works |
| 17-18 | Documentation site | Core | docs.roast-lang.org live |
| 19-20 | Incremental compilation | Core | Large projects build fast |
| 21-22 | Getting started tutorial | Core | User can follow tutorial |

**Exit Criteria:**
- [ ] 10+ community packages published
- [ ] Tutorial completion rate >80%
- [ ] Python interop docs complete

---

### Phase 3: Hardening - Weeks 23-26

**Goals:**
- Security confidence
- Performance validation
- Release polish

**Deliverables:**

| Week | Deliverable | Owner | Verification |
|------|-------------|-------|--------------|
| 23 | Security audit (external) | Vendor | Report received |
| 24 | Audit findings fixed | Core | All critical/high fixed |
| 25 | Benchmark suite | Core | Published results |
| 26 | v1.0 RC | Core | All criteria met |

**Exit Criteria:**
- [ ] Security audit complete
- [ ] All P0/P1 issues fixed
- [ ] Performance benchmarks published
- [ ] Go/No-Go checklist all green

---

### Phase 4: v1.0 Release - Week 27

**Activities:**
1. Final testing pass
2. Build release binaries
3. Publish to registry
4. Announce on social media
5. Monitor for critical issues

---

## Appendix: Resource Allocation

### Assuming 1 Full-Time Developer

| Phase | Duration | Focus |
|-------|----------|-------|
| Safety | 6 weeks | 100% on blockers |
| Tooling | 8 weeks | 80% features, 20% bugs |
| Ecosystem | 8 weeks | 60% features, 40% docs |
| Hardening | 4 weeks | 100% polish and audit |

### Assuming 2-3 Developers

| Track | Developer | Duration |
|-------|-----------|----------|
| Safety (borrow checker) | Dev 1 | Weeks 1-4 |
| Safety (Send/Sync) | Dev 2 | Weeks 1-4 |
| Registry deployment | Dev 3 | Weeks 1-2 |
| LSP | Dev 2 | Weeks 5-8 |
| Windows | Dev 3 | Weeks 5-6 |
| Python FFI | Dev 1 | Weeks 5-8 |

**Total with 3 developers:** ~12 weeks to v1.0

---

## Appendix: Key Decisions

| Decision | Chosen Approach | Justification |
|----------|-----------------|---------------|
| Memory safety model | Enforce borrow checker | Correctness over flexibility |
| Thread safety model | Send/Sync traits | Proven by Rust |
| Unsafe escape hatch | `@unsafe` decorator | Pragmatic for FFI |
| Registry hosting | Cloudflare Workers | Already implemented |
| Platform priority | Linux → Windows → macOS | User base |

---

*Plan prepared by: Senior Compiler Engineer / Security Auditor*  
*Review status: Ready for execution*
