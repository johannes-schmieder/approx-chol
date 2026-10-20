# MultiwayMG factor boundary

This owner fork starts at published approx-chol 0.5.0 source
`36f07b1dfc6300deb790a29053a56d8718dede54`. Its dedicated `multiwaymg` branch
preserves upstream main and MIT licensing. All 34 registry Rust source files,
manifest, README and license were checked against that revision. No publication
or upstream change is part of this increment.

## Supplied scratch

`Factor::permutation_scratch_len()` returns zero for block-contiguous input and
`n()` otherwise, including any ground augmentation. The caller supplies that many
scalar entries to `solve_in_place_with_scratch` (anchored action) or
`solve_into_with_scratch` (canonical least-squares action with zero-padded RHS).
Both validate every public shape before changing either buffer and preserve
unused tails. Gather initializes every consumed scratch entry, so prior values
need not be zero. Short scratch returns a static typed error without allocating.

Old convenience methods and the new APIs share the same gather/block/scatter
core. Block factorization, substitution, anchor rules and visit order are
unchanged; no unsafe code, thread pool or stored fields are added. Factor
serialization is unchanged. Success has the same numerical limitations as the
old action; it is not an original-system certificate.

## Capacity inventory

`retained_heap_bytes()` checks capacity-times-element-size arithmetic and every
addition. It counts outer block/fallback descriptors, optional permutation and
every exact packed or approximate header/neighbor/fraction array. Spare capacity
is included. It allocates nothing and takes O(number of blocks) time. Inline
Factor storage, allocator headers/excess beyond vector capacity, caller scratch,
construction peak and other owners are outside this precisely scoped count.
Overflow returns `None`; an unknown total must not become zero.

This is a post-build inventory, not pre-admission. Checked construction/fill,
failed exact-to-approximate overlap and enclosing solver lifetimes remain work.
The planned within Direct/Cover integration can reuse already dead regions of
its existing two buffers, but this factor increment does not implement that
integration or change within/MultiwayMG pins.

## Qualification

Nine explicit recipes crossed with forced approximate, exact and mixed routing
cover empty/zero/positive-singleton, connected, interleaved floating, mixed
ground/floating, floating/grounded signed-cover-shaped and unequal-block inputs.
`qualification/factor-reference` is an isolated test workspace with an exact
registry 0.5.0 oracle (checksum
`affa21d606ec089747ab3ef1baff8feac12b5c115e42d4451b01668c9acded56`). It compares
f32/f64 values with the unchanged published implementation, including old APIs,
new actions, short/zero RHS, poisoned scratch, padding, errors/recovery and
serialized roundtrips. Interleaved cases explicitly require a nonidentity
permutation; the wire check also requires a permutation that is not its own
inverse. An independent exact equation reference checks component means.

The isolated allocator process checks 27 f64 factor configurations: query, first
canonical and anchored actions, 64 repeats, rejection/recovery, and destruction.
Reported retained bytes must equal all deallocation layouts on factor drop.
Private coverage exercises spare capacities and fallback descriptors, and checked
byte overflow. Permanent Rust1.85/three-platform debug/release checks retain raw
records and frozen-original comparisons. This is correctness/memory evidence,
not a speedup, construction-admission or sparse-terminal range certificate.

The inherited Criterion 0.8.2 dev dependency needs Rust1.86, although the library
MSRV is1.85. Pin Criterion0.7.0 (MSRV1.80) to run the entire development suite on
Rust1.85. The lock removes its unused benchmark-only dependencies and adds pinned
stats_alloc0.1.10. Cargo also corrects tempfile3.26.0's locked dependency edge to
its declared getrandom0.4.1; no runtime library version is upgraded. The isolated
oracle lock is separate, so it does not introduce a production duplicate or alter
ordinary upstream package selection. Preserve original preflight failure logs.

Rust1.85 strict Clippy also requires eliding five redundant lifetime names in
existing CSR/step implementation and test signatures. Their bodies and inferred lifetimes
are unchanged; the original failures remain in the qualification logs.

All 13 local Rust1.85 required/release/reference groups pass. Exact source/PR
CI, provider-verified platform artifacts and guarded merge remain required before
downstream pinning. Local logs and input hashes: `qualification/local-checks.json`.

## Discriminant repair

The recovered source `60e6cfc` inserted `SolveError::ScratchBufferTooSmall` before
`WorkBufferTooSmall`. That shifted the published variant's implicit discriminant
from 1 to 2, and the inherited `cargo semver-checks` gate failed with
`enum_no_repr_variant_discriminant_changed` against registry 0.5.0. The repair
appends the new variant instead, so every published variant keeps its
discriminant. Messages, fields and every caller are unchanged. The failure is
reproduced and preserved, and the repaired source passes all 196 semver checks
with "no semver update required". All 15 Rust 1.85.0 contract groups above pass
again on Linux, and `cargo deny` passes. Its yanked `chacha20` warning is also
present on the unchanged 0.5.0 baseline. Hashes:
`qualification/local-checks-repair.json`. Exact source/PR CI and guarded merge
still precede downstream pinning.
