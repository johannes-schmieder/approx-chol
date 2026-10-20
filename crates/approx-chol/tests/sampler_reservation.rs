//! A reserved clique-tree sampler requests exactly its predicted bytes and then
//! samples stars without allocating, with unchanged results.
use approx_chol::low_level::CliqueTreeSampler;

fn measure<R>(run: impl FnOnce() -> R) -> (R, allocation_counter::AllocationInfo) {
    let mut result = None;
    let info = allocation_counter::measure(|| result = Some(run()));
    (result.unwrap(), info)
}

/// Deterministic star with `n` distinct neighbors and dyadic weights.
fn star(n: usize, salt: u32) -> Vec<(u32, f64)> {
    (0..n as u32)
        .map(|i| (i * 7 + salt, 2.0_f64.powi(((i * 5 + salt) % 9) as i32 - 4)))
        .collect()
}

fn main() {
    let mut cases = 0;
    for split in [None, Some(2), Some(3)] {
        for max_entries in [0usize, 1, 2, 33, 200] {
            let mut reserved = CliqueTreeSampler::<f64>::new(19, split);
            let mut reference = CliqueTreeSampler::<f64>::new(19, split);
            let predicted = reserved.reservation_bytes(max_entries).unwrap();
            let before = reserved.retained_heap_bytes().unwrap();
            let (result, info) = measure(|| reserved.try_reserve(max_entries));
            result.unwrap();
            assert_eq!(
                info.bytes_total as usize, predicted,
                "{split:?} {max_entries}"
            );
            let after = reserved.retained_heap_bytes().unwrap();
            assert_eq!(after as i64 - before as i64, info.bytes_current);
            assert_eq!(reserved.reservation_bytes(max_entries), Some(0));
            let (again, info) = measure(|| reserved.try_reserve(max_entries));
            again.unwrap();
            assert_eq!((info.count_total, info.bytes_total), (0, 0));

            let mut out = Vec::with_capacity(reserved.max_fill_edges(max_entries));
            let mut expected = Vec::new();
            for (index, n) in [
                max_entries,
                max_entries / 2,
                max_entries.min(1),
                max_entries,
            ]
            .into_iter()
            .enumerate()
            {
                let entries = star(n, index as u32);
                out.clear();
                let ((), info) = measure(|| reserved.sample(index as u64, &entries, &mut out));
                assert_eq!(
                    info.count_total, 0,
                    "{split:?} {max_entries} star {n} allocated"
                );
                assert!(out.len() <= reserved.max_fill_edges(n));
                expected.clear();
                reference.sample(index as u64, &entries, &mut expected);
                assert_eq!(out.len(), expected.len());
                for (a, b) in out.iter().zip(&expected) {
                    assert_eq!((a.0, a.1, a.2.to_bits()), (b.0, b.1, b.2.to_bits()));
                }
            }
            assert_eq!(reserved.retained_heap_bytes(), Some(after));
            let ((), info) = measure(|| drop(reserved));
            assert_eq!(-info.bytes_current, after as i64);
            println!(
                "SAMPLER split={split:?} max_entries={max_entries} reserved={predicted} retained={after} samples=0 allocations"
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 15);
}
