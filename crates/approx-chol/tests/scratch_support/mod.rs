#![allow(dead_code)]
use approx_chol::{Backend, Config, CsrRef, ExactFailure, Factor};
use num_traits::Float;

pub struct Recipe {
    pub name: &'static str,
    pub n: usize,
    pub rows: Vec<u32>,
    pub cols: Vec<u32>,
    pub values: Vec<f64>,
}

fn recipe(
    name: &'static str,
    n: usize,
    edges: &[(usize, usize, f64)],
    surplus: &[(usize, f64)],
) -> Recipe {
    let mut dense = vec![0.0; n * n];
    for &(i, j, w) in edges {
        dense[i * n + i] += w;
        dense[j * n + j] += w;
        dense[i * n + j] -= w;
        dense[j * n + i] -= w;
    }
    for &(i, w) in surplus {
        dense[i * n + i] += w;
    }
    let (mut rows, mut cols, mut values) = (vec![0], vec![], vec![]);
    for i in 0..n {
        for j in 0..n {
            if i == j || dense[i * n + j] != 0.0 {
                cols.push(j as u32);
                values.push(dense[i * n + j]);
            }
        }
        rows.push(cols.len() as u32);
    }
    Recipe {
        name,
        n,
        rows,
        cols,
        values,
    }
}

pub fn recipes() -> Vec<Recipe> {
    let interleaved = [
        (0, 2, 0.5),
        (2, 4, 2.0),
        (4, 6, 8.0),
        (1, 3, 1.0),
        (3, 5, 4.0),
        (5, 7, 0.25),
    ];
    vec![
        recipe("empty", 0, &[], &[]),
        recipe("zero-singleton", 1, &[], &[]),
        recipe("ground-singleton", 1, &[], &[(0, 2.0)]),
        recipe(
            "connected",
            7,
            &[
                (0, 1, 0.5),
                (1, 2, 2.0),
                (2, 3, 8.0),
                (3, 4, 1.0),
                (4, 5, 4.0),
                (5, 6, 0.25),
                (0, 3, 0.5),
                (2, 6, 2.0),
            ],
            &[],
        ),
        recipe("interleaved", 8, &interleaved, &[]),
        recipe("mixed-ground-floating", 9, &interleaved, &[(1, 2.0)]),
        recipe(
            "cover-floating",
            8,
            &[
                (0, 5, 1.0),
                (1, 4, 1.0),
                (1, 6, 2.0),
                (2, 5, 2.0),
                (2, 7, 0.5),
                (3, 6, 0.5),
            ],
            &[],
        ),
        recipe(
            "cover-ground",
            8,
            &[
                (0, 5, 1.0),
                (1, 4, 1.0),
                (1, 6, 2.0),
                (2, 5, 2.0),
                (2, 7, 0.5),
                (3, 6, 0.5),
            ],
            &[(0, 2.0), (4, 2.0)],
        ),
        recipe(
            "unequal-blocks",
            9,
            &[
                (0, 3, 0.5),
                (1, 4, 2.0),
                (4, 6, 8.0),
                (6, 8, 1.0),
                (2, 5, 4.0),
            ],
            &[],
        ),
    ]
}

pub fn candidate<T: Float + Send + Sync + 'static>(r: &Recipe, mode: usize) -> Factor<T> {
    let values: Vec<T> = r.values.iter().map(|&x| T::from(x).unwrap()).collect();
    approx_chol::factorize_with(
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

pub fn rhs<T: Float>(n: usize) -> Vec<T> {
    (0..n)
        .map(|i| T::from(((i * 13 + 3) % 19) as f64 * 0.25 - 2.0).unwrap())
        .collect()
}

pub fn assert_bits<T: Float>(a: &[T], b: &[T]) {
    assert_eq!(a.len(), b.len());
    for (&x, &y) in a.iter().zip(b) {
        assert_eq!(x.to_f64().unwrap().to_bits(), y.to_f64().unwrap().to_bits());
    }
}
