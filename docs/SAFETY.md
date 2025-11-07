# Memory Safety

This document explains our approach to memory safety, unsafe code usage, and fuzzing strategy.

## Core Safety Guarantees

1. **No undefined behavior**: All code paths must be free of UB under all inputs
2. **No memory leaks**: Resources freed deterministically (RAII)
3. **No data races**: Thread-safe by construction (Rust's type system)
4. **No buffer overflows**: Bounds-checked or proven safe

## Unsafe Code Policy

### Where Unsafe is Allowed

#### 1. SIMD Intrinsics
Platform-specific vectorization requires `unsafe` blocks:

```rust
#[cfg(target_arch = "x86_64")]
fn yuv_to_rgb_avx2(input: &[u8], output: &mut [u8]) {
    use std::arch::x86_64::*;

    // SAFETY: AVX2 intrinsics for YUV→RGB conversion
    //   - Input slice is guaranteed to have length multiple of 32 (asserted by caller)
    //   - Input and output are non-overlapping (enforced by &/&mut exclusivity)
    //   - AVX2 available (checked by is_x86_feature_detected! before dispatch)
    //   - Alignment verified at runtime (input aligned to 32 bytes)
    //   Proof: Type system prevents overlap; runtime checks validate alignment/length
    //   Alternatives: Portable SIMD insufficient for ±5% perf target (10% slower in benches)
    unsafe {
        let y_vec = _mm256_loadu_si256(input.as_ptr() as *const __m256i);
        // ... conversion logic ...
    }
}
```

**Requirements**:
- Runtime feature detection (`is_x86_feature_detected!`)
- Bounds and alignment checks outside `unsafe` block
- Benchmark proving scalar impl insufficient

#### 2. Memory-Mapped I/O
File mapping for zero-copy reads:

```rust
struct MmapSource {
    mmap: memmap2::Mmap,
    pos: usize,
}

impl MmapSource {
    fn new(file: File) -> Result<Self> {
        // SAFETY: Memory-mapping for zero-copy file reads
        //   - File is opened read-only (no write access)
        //   - Mmap lives as long as file (owned together)
        //   - No other writers can modify file (filesystem guarantees for RO)
        //   - Returned slice valid until mmap dropped (lifetime tied to self)
        //   Proof: Ownership prevents UAF; OS enforces RO
        //   Alternatives: Buffered reads 15% slower for large files (benched)
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Self { mmap, pos: 0 })
    }
}
```

**Requirements**:
- File opened read-only
- `Mmap` owned, not borrowed (no lifetime issues)
- Benchmark showing buffered I/O insufficient

#### 3. Hardware Decoder Interfaces
GPU buffer access (hwaccel):

```rust
impl VaapiDecoder {
    fn map_surface(&self, surface_id: u32) -> Result<&[u8]> {
        // SAFETY: VAAPI surface mapping for zero-copy GPU→CPU transfer
        //   - Surface ID validated by VAAPI runtime (returns error if invalid)
        //   - Map/unmap paired (RAII guard ensures unmap on drop)
        //   - No aliasing: single &[u8] at a time (enforced by borrow checker)
        //   - Pointer valid until unmap (VAAPI guarantees)
        //   Proof: VAAPI API contract + RAII guard
        //   Alternatives: None (hwaccel requires FFI)
        unsafe {
            let ptr = vaapi_sys::va_map_buffer(self.display, surface_id)?;
            std::slice::from_raw_parts(ptr, self.surface_size)
        }
    }
}
```

**Requirements**:
- RAII guard for map/unmap pairing
- Validation before FFI calls
- No alternative (inherent to hwaccel)

### Where Unsafe is Forbidden

1. **Parsing**: Bitstream parsers, container parsers (pure safe Rust)
2. **Codec logic**: Prediction, transform, reconstruction (safe abstractions)
3. **Error handling**: No `std::hint::unreachable_unchecked()`
4. **Manual memory management**: No raw pointers unless SIMD/FFI

## Common Pitfalls and Mitigations

### 1. Integer Overflow
**Pitfall**: Malicious input triggers `width * height` overflow → allocation panic or UB.

**Mitigation**:
```rust
let width = u32::from(sps.pic_width_in_mbs) * 16;
let height = u32::from(sps.pic_height_in_mbs) * 16;
let pixels = width.checked_mul(height)
    .ok_or(Error::Invalid { what: "SPS", msg: "dimensions overflow" })?;
```

**Policy**: Always use `checked_*` or `saturating_*` for untrusted inputs.

### 2. Slice Indexing
**Pitfall**: `data[offset..offset+len]` panics if out-of-bounds.

**Mitigation**:
```rust
let slice = data.get(offset..offset+len)
    .ok_or(Error::UnexpectedEof)?;
```

**Policy**: Use `.get()` for untrusted offsets; `[]` only when proven in-bounds (e.g., `for i in 0..len`).

### 3. Buffer Underflow
**Pitfall**: Negative DTS/PTS wraps to huge u64 when cast.

**Mitigation**:
```rust
let pts = i64::from(raw_pts);  // Sign-extended
if pts < 0 {
    return Err(Error::Invalid { what: "PTS", msg: "negative timestamp" });
}
```

**Policy**: Validate signed→unsigned conversions.

### 4. Uninitialized Memory
**Pitfall**: `Vec::with_capacity(n)` + index access before push.

**Mitigation**:
```rust
let mut buf = vec![0u8; size];  // Initialized to zero
// Safe to index into buf[0..size]
```

**Policy**: Use `vec![0; n]` or `Vec::new() + push()`. Never `set_len()` on uninitialized.

## Fuzzing Strategy

### Coverage-Guided Fuzzing
**Tool**: `cargo-fuzz` (libFuzzer)

**Targets**:
1. `fuzz/av-format/mp4_box_parser.rs`: MP4 atom parsing
2. `fuzz/av-format/matroska_demuxer.rs`: EBML parsing
3. `fuzz/h264-bitstream/nal_parser.rs`: H.264 NAL units
4. `fuzz/h264-bitstream/sps_parser.rs`: SPS syntax elements
5. `fuzz/fuzz-avc/slice_decoder.rs`: Slice decoding (with mocked reference frames)

### Corpus Management
- **Seed corpus**: FATE samples (known-good files)
- **Generated corpus**: Fuzzer-discovered interesting inputs
- **Minimize**: Reduce corpus to essential coverage (`cargo fuzz cmin`)

### Continuous Fuzzing
- **CI**: 60s per target on every PR (smoke test)
- **Nightly**: 4h per target (deeper exploration)
- **OSS-Fuzz** (future): 24/7 on Google infra

### Crash Triage
1. Fuzzer saves crashing input to `fuzz/artifacts/<target>/crash-<hash>`
2. Dev reproduces: `cargo fuzz run <target> fuzz/artifacts/<target>/crash-<hash>`
3. Fix bug, add input to regression tests
4. Re-run minimizer: `cargo fuzz cmin <target>`

## Sanitizers

### Address Sanitizer (ASan)
Detects: Use-after-free, buffer overflows, leaks

**Usage**:
```bash
RUSTFLAGS="-Z sanitizer=address" cargo +nightly test
```

**CI**: Run on Linux x86_64 (not all platforms support ASan)

### Integer Sanitizer
Detects: Integer overflow, division by zero

**Usage**:
```bash
RUSTFLAGS="-Z sanitizer=integer" cargo +nightly test
```

**Note**: Not needed if overflow checks enabled (`debug_assertions` or explicit `checked_*`)

### Memory Sanitizer (MSan)
Detects: Uninitialized memory reads

**Usage** (requires instrumented stdlib):
```bash
cargo +nightly msan --target x86_64-unknown-linux-gnu
```

**CI**: Weekly (slow; requires custom build)

### Undefined Behavior Sanitizer (UBSan)
Detects: Alignment violations, invalid enum values, etc.

**Usage**:
```bash
RUSTFLAGS="-Z sanitizer=undefined" cargo +nightly test
```

## Miri (Interpreter for Rust MIR)

**Purpose**: Detect UB in pure Rust (no FFI, no SIMD)

**Usage**:
```bash
cargo +nightly miri test --package av-core
cargo +nightly miri test --package av-format
```

**CI**: Run on core crates weekly (slow)

**Limitations**:
- No FFI (can't test hwaccel)
- No inline ASM/SIMD (can't test intrinsics)
- Slow (10-100x slower than native)

## Clippy Lints

### Mandatory Lints (Fail CI)
```toml
[lints.clippy]
unwrap_used = "deny"                # No .unwrap() in library code
expect_used = "deny"                # No .expect() either
indexing_slicing = "warn"           # Prefer .get()
panic = "deny"                      # No explicit panic!() in libraries
```

### Suggested Lints (Warn)
```toml
cast_lossless = "warn"              # u8→u32 should use .into()
cast_possible_truncation = "warn"   # u32→u8 should be explicit
```

## Testing for Safety

### Unit Tests with Adversarial Inputs
```rust
#[test]
fn test_sps_overflow() {
    let sps = b"\x00\x00\x00\x01\x67\xFF\xFF\xFF\xFF";  // Huge width/height
    let result = parse_sps(sps);
    assert!(matches!(result, Err(Error::Invalid { .. })));
}

#[test]
fn test_slice_oob() {
    let data = [0u8; 10];
    let result = data.get(5..15);  // Out of bounds
    assert!(result.is_none());
}
```

### Property-Based Testing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn parse_sps_never_panics(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let _ = parse_sps(&data);  // Should return Err, not panic
    }
}
```

## Dependency Auditing

### `cargo-audit`
Checks dependencies for known vulnerabilities (RustSec database).

**Usage**:
```bash
cargo audit
```

**CI**: Runs daily; fails on high-severity issues.

### `cargo-deny`
Enforces licensing, dependency policies.

**Usage**:
```bash
cargo deny check
```

**Policies**:
- No GPL dependencies
- No duplicate dependencies (except unavoidable)
- No unmaintained crates (last commit >2 years)

## Historical Vulnerabilities (None Yet)

This section will track any vulnerabilities discovered and fixed, for transparency.

**Format**:
- **CVE-YYYY-NNNNN** (if assigned)
- **Affected versions**: X.Y.Z - A.B.C
- **Description**: Brief summary
- **Fix**: PR #NNN, commit hash
- **Credit**: Discoverer name

---

**Example (hypothetical)**:
- **CVE-2025-12345**
- **Affected**: 0.1.0 - 0.1.3
- **Description**: Integer overflow in MP4 `stts` table parsing could cause panic
- **Fix**: PR #42, commit `abc123f`
- **Credit**: John Doe via OSS-Fuzz

---

## Safe Coding Checklist

Use this checklist for code reviews:

- [ ] No `unwrap()` or `expect()` in library code (only tests)
- [ ] Integer arithmetic uses `checked_*` or proven not to overflow
- [ ] Slice indexing uses `.get()` or loops with proven bounds
- [ ] All `unsafe` blocks have SAFETY comments with proof
- [ ] New parsers have fuzz targets
- [ ] Unit tests cover error paths (not just success)
- [ ] No `set_len()` on uninitialized vectors
- [ ] Casts (esp. signed→unsigned) validated

## Resources

- **Rustonomicon**: https://doc.rust-lang.org/nomicon/
- **Unsafe Code Guidelines**: https://rust-lang.github.io/unsafe-code-guidelines/
- **cargo-fuzz Book**: https://rust-fuzz.github.io/book/
- **RustSec**: https://rustsec.org/

---

**Remember**: If in doubt, write safe code first. Profile before adding `unsafe`. Every `unsafe` block is a potential bug.
