mod scratch_support;
use scratch_support::*;

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
