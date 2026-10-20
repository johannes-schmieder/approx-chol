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

## Checked construction

`Builder::build_with_limit(sddm, ConstructionLimit)` returns the factor with a
`ConstructionReport` of peak and retained payload. Every construction array is
charged to one ledger before allocation: the narrowed index copy, a non-canonical
rewrite and its stable-sort scratch, mirror cursors, union-find, diagonal, row sums,
ground growth, block layout, local maps, adjacency lists including every fill-edge
growth, elimination bits, degrees, ordering buckets, star, dedup, sampler, column and
degree-update scratch, the sequence output and the retained factor arrays. A growing
array keeps its old capacity charged while the new one is allocated. A failed exact
pivot releases its dense copy before the approximate fallback allocates. Exceeding
the inclusive limit returns `Error::ConstructionBudgetExceeded` before the refused
allocation, with no factor. A limit never changes a successful factor. An allocator
refusal of a dense exact copy still falls back as `WillNotFit`, as in 0.5.0. Other
refused reservations return `Error::AllocationFailed` instead of aborting. Both
error variants are appended to the non-exhaustive `Error`, and semver checks pass.

`build` runs the same generic construction with an unaccounted policy, so it pays
no accounting cost. The per-step elimination scratch is bounded by the eliminated
vertex's stored list: one doubling bound covers star, sampler, column, fill and
degree-update arrays. Construction is serial and adds no threads or unsafe code.

### Qualification

The 31 generated inputs cover random sparse, floating, grid, hub stars above the
scatter threshold, interleaved disconnected grounded components, non-canonical
rows with split duplicates, and a natural exact-pivot cancellation fallback, with
n up to 1,500. Five configurations cover AC, default, mixed AC2, AC3 and exact
blocks up to 400.

- `construction_limit`: all 155 f32/f64 builds are wire-identical to `build`
  unlimited and at exactly the reported peak. Accounting is deterministic, and
  every lower limit, from peak - 1 down to 0, refuses with the typed error.
- `construction_allocations`: an isolated counting allocator, whose reallocation
  counts old and new buffers together, measures each build. Across 155 builds, in
  debug and release with all and no features, the real live-heap peak equals the
  ledger peak with zero slack. Retained bytes equal the report and
  `retained_heap_bytes()`, and dropping the factor frees exactly that. Refused
  builds at peak - 1 and peak / 3 stay within the limit and leak nothing.
- The isolated reference workspace compares all 155 generated builds,
  unlimited and at peak, with registry 0.5.0 by serialized wire and finds them
  bit-for-bit equal. The original 27-recipe contracts still pass.
- An informational release timing on one Linux container (min of 30 rotated
  runs, 40 approximate-route cases, n = 1,500) found `build` at 1.012 and
  `build_with_limit` at 1.032 geometric-mean time relative to 0.5.0. The worst
  checked case was 1.10 on hub stars. This is not a performance gate.

This bounds construction inside approx-chol only. Within's Schur complement,
cover assembly, parallel setup and solver-level overlap remain the next increment.
Downstream pins still wait on reviewed merges.
