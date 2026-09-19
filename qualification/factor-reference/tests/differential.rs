#[path = "../../../crates/approx-chol/tests/scratch_support/mod.rs"]
mod scratch_support;
use approx_chol::SolveError;
use num_traits::Float;
use scratch_support::*;

fn contract<T: Float + Send + Sync + 'static>() {
    let mut permuted = 0;
    for r in recipes() {
        for mode in 0..3 {
            let factor = candidate::<T>(&r, mode);
            let old = reference::<T>(&r, mode);
            assert_eq!(
                (factor.n(), factor.original_n(), factor.n_steps()),
                (old.n(), old.original_n(), old.n_steps())
            );
            let (n, s) = (factor.n(), factor.permutation_scratch_len());
            if r.name == "interleaved" {
                assert_eq!(s, n);
            }
            if s > 0 {
                permuted += 1;
            }
            let input = rhs::<T>(n);
            for length in [0, factor.original_n() / 2, factor.original_n()] {
                let mut expected = vec![T::from(-9876.0).unwrap(); n + 3];
                old.solve_into(&input[..length], &mut expected).unwrap();
                let mut legacy = vec![T::from(-9876.0).unwrap(); n + 3];
                factor.solve_into(&input[..length], &mut legacy).unwrap();
                assert_bits(&legacy, &expected);
                for _ in 0..3 {
                    let mut work = vec![T::from(-9876.0).unwrap(); n + 3];
                    let mut scratch = vec![T::nan(); s + 2];
                    factor
                        .solve_into_with_scratch(&input[..length], &mut work, &mut scratch)
                        .unwrap();
                    assert_bits(&work, &expected);
                    assert!(scratch[s..].iter().all(|x| x.is_nan()));
                }
            }
            let mut expected = input.clone();
            old.solve_in_place(&mut expected).unwrap();
            let mut legacy = input.clone();
            factor.solve_in_place(&mut legacy).unwrap();
            assert_bits(&legacy, &expected);
            let mut work = input.clone();
            work.extend([T::from(333.0).unwrap(); 3]);
            let mut scratch = vec![T::nan(); s + 2];
            factor
                .solve_in_place_with_scratch(&mut work, &mut scratch)
                .unwrap();
            assert_bits(&work[..n], &expected);
            assert!(work[n..].iter().all(|&x| x == T::from(333.0).unwrap()));
            assert!(scratch[s..].iter().all(|x| x.is_nan()));

            // All static errors preserve both buffers, including NaN payload padding.
            let original = work.clone();
            let scratch_before = scratch.clone();
            assert!(matches!(
                factor.solve_into_with_scratch(
                    &vec![T::zero(); factor.original_n() + 1],
                    &mut work,
                    &mut scratch
                ),
                Err(SolveError::RhsLengthExceedsFactor { .. })
            ));
            assert_bits(&work, &original);
            assert_bits(&scratch, &scratch_before);
            if n > 0 {
                assert!(matches!(
                    factor.solve_in_place_with_scratch(&mut work[..n - 1], &mut scratch),
                    Err(SolveError::WorkBufferTooSmall { .. })
                ));
                assert!(matches!(
                    factor.solve_into_with_scratch(&[], &mut work[..n - 1], &mut scratch),
                    Err(SolveError::WorkBufferTooSmall { .. })
                ));
                assert_bits(&work, &original);
                assert_bits(&scratch, &scratch_before);
            }
            if s > 0 {
                assert!(
                    matches!(factor.solve_in_place_with_scratch(&mut work,&mut scratch[..s-1]),Err(SolveError::ScratchBufferTooSmall{scratch_len,required}) if scratch_len==s-1 && required==s)
                );
                assert!(matches!(
                    factor.solve_into_with_scratch(&[], &mut work, &mut scratch[..s - 1]),
                    Err(SolveError::ScratchBufferTooSmall { .. })
                ));
                assert_bits(&work, &original);
                assert_bits(&scratch, &scratch_before);
            }
            // A valid call after errors clears/preserves the same regions as before.
            factor
                .solve_into_with_scratch(&[], &mut work, &mut scratch)
                .unwrap();
            assert!(work[..n].iter().all(|&x| x == T::zero()));
            assert_bits(&work[n..], &original[n..]);
        }
    }
    assert!(permuted >= 6);
}

#[test]
fn f64_original_and_supplied_scratch_contract() {
    contract::<f64>();
}
#[test]
fn f32_original_and_supplied_scratch_contract() {
    contract::<f32>();
}

#[test]
fn independent_two_component_solution() {
    let r = recipes()
        .into_iter()
        .find(|r| r.name == "interleaved")
        .unwrap();
    let factor = candidate::<f64>(&r, 1);
    let x = rhs::<f64>(r.n);
    let mut b = vec![0.0; r.n];
    for (i, out) in b.iter_mut().enumerate() {
        for k in r.rows[i] as usize..r.rows[i + 1] as usize {
            *out += r.values[k] * x[r.cols[k] as usize];
        }
    }
    let mut got = vec![0.0; factor.n()];
    let mut scratch = vec![f64::NAN; factor.permutation_scratch_len()];
    factor
        .solve_into_with_scratch(&b, &mut got, &mut scratch)
        .unwrap();
    for parity in 0..2 {
        let mean = (parity..r.n).step_by(2).map(|i| x[i]).sum::<f64>() / 4.0;
        for i in (parity..r.n).step_by(2) {
            assert!((got[i] - (x[i] - mean)).abs() < 1e-12);
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn wire_format_matches_original_and_roundtrips() {
    for r in recipes() {
        for mode in 0..3 {
            let factor = candidate::<f64>(&r, mode);
            let old = reference::<f64>(&r, mode);
            let wire = serde_json::to_value(&factor).unwrap();
            assert_eq!(wire, serde_json::to_value(&old).unwrap());
            if r.name == "interleaved" {
                let p = wire["permutation"]["forward"].as_array().unwrap();
                assert!(p
                    .iter()
                    .enumerate()
                    .any(|(i, v)| p[v.as_u64().unwrap() as usize].as_u64().unwrap() as usize != i));
            }
            let restored: approx_chol::Factor = serde_json::from_value(wire).unwrap();
            let input = rhs::<f64>(r.n);
            let expected = old.solve(&input).unwrap();
            let mut work = vec![0.0; restored.n()];
            let mut scratch = vec![f64::NAN; restored.permutation_scratch_len()];
            restored
                .solve_into_with_scratch(&input, &mut work, &mut scratch)
                .unwrap();
            assert_bits(&work[..r.n], &expected);
        }
    }
}

pub fn reference<T: Float + Send + Sync + 'static>(
    r: &Recipe,
    mode: usize,
) -> approx_chol_reference::Factor<T> {
    use approx_chol_reference::{Backend, Config, CsrRef, ExactFailure};
    let values: Vec<T> = r.values.iter().map(|&x| T::from(x).unwrap()).collect();
    approx_chol_reference::factorize_with(
        CsrRef::new(&r.rows, &r.cols, &values, r.n as u32).unwrap(),
        Config {
            seed: 17,
            split_merge: Some(2),
            backend: match mode {
                0 => Backend::Approximate,
                _ => Backend::ExactBelow {
                    max_dim: if mode == 1 { 256 } else { 2 },
                    on_failure: ExactFailure::Error,
                },
            },
        },
    )
    .unwrap()
}
