mod scratch_support;
use scratch_support::*;
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn no_alloc(s: stats_alloc::Stats) {
    assert_eq!(
        (
            s.allocations,
            s.reallocations,
            s.deallocations,
            s.bytes_allocated,
            s.bytes_deallocated
        ),
        (0, 0, 0, 0, 0)
    );
}

fn main() {
    let mut controls = 0;
    for r in recipes() {
        for mode in 0..3 {
            let factor = candidate::<f64>(&r, mode);
            let n = factor.n();
            let s = factor.permutation_scratch_len();
            let b = rhs::<f64>(r.n);
            let mut work = vec![0.0; n];
            let mut scratch = vec![f64::NAN; s];
            let query = Region::new(GLOBAL);
            let bytes = factor.retained_heap_bytes().unwrap();
            no_alloc(query.change());
            let first = Region::new(GLOBAL);
            factor
                .solve_into_with_scratch(&b, &mut work, &mut scratch)
                .unwrap();
            no_alloc(first.change());
            let expected = work.clone();
            let first_anchored = Region::new(GLOBAL);
            factor
                .solve_in_place_with_scratch(&mut work, &mut scratch)
                .unwrap();
            no_alloc(first_anchored.change());
            let repeats = Region::new(GLOBAL);
            for _ in 0..64 {
                scratch.fill(f64::NAN);
                factor
                    .solve_into_with_scratch(&b, &mut work, &mut scratch)
                    .unwrap();
                assert_bits(&work, &expected);
                factor
                    .solve_in_place_with_scratch(&mut work, &mut scratch)
                    .unwrap();
            }
            no_alloc(repeats.change());
            let errors = Region::new(GLOBAL);
            if n > 0 {
                assert!(factor
                    .solve_in_place_with_scratch(&mut work[..n - 1], &mut scratch)
                    .is_err());
            }
            if s > 0 {
                assert!(factor
                    .solve_into_with_scratch(&b, &mut work, &mut scratch[..s - 1])
                    .is_err());
            }
            factor
                .solve_into_with_scratch(&b, &mut work, &mut scratch)
                .unwrap();
            no_alloc(errors.change());
            let dropping = Region::new(GLOBAL);
            drop(factor);
            let d = dropping.change();
            assert_eq!(
                (d.allocations, d.reallocations, d.bytes_allocated),
                (0, 0, 0)
            );
            assert_eq!(d.bytes_deallocated, bytes);
            println!("FACTOR_SCRATCH case={} mode={} n={} scratch={} heap={} first=0 repeat=0 errors=0 query=0 drop={}",r.name,mode,n,s,bytes,d.bytes_deallocated);
            controls += 1;
        }
    }
    assert_eq!(controls, 27);
}
