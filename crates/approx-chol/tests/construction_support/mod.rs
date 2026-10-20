//! Deterministic generated inputs for checked-construction qualification.
#![allow(dead_code)]
use approx_chol::{Backend, Config, ExactFailure};

pub struct Generated {
    pub name: String,
    pub n: usize,
    pub rows: Vec<u32>,
    pub cols: Vec<u32>,
    pub values: Vec<f64>,
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % bound as u64) as usize
    }
}

/// Assemble an SDDM CSR from weighted edges and diagonal surpluses. With
/// `canonical == false`, rows are reversed and one off-diagonal per row is split
/// into two stored halves, so ingestion must rewrite and coalesce.
fn assemble(
    name: String,
    n: usize,
    edges: &[(usize, usize, f64)],
    surplus: &[(usize, f64)],
    canonical: bool,
) -> Generated {
    let mut entries: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
    let mut diagonal = vec![0.0; n];
    for &(i, j, w) in edges {
        if i == j {
            continue;
        }
        diagonal[i] += w;
        diagonal[j] += w;
        entries[i].push((j as u32, -w));
        entries[j].push((i as u32, -w));
    }
    for &(i, w) in surplus {
        diagonal[i] += w;
    }
    let (mut rows, mut cols, mut values) = (vec![0u32], Vec::new(), Vec::new());
    for (i, row) in entries.iter_mut().enumerate() {
        row.push((i as u32, diagonal[i]));
        row.sort_by_key(|&(col, _)| col);
        // Coalesce generated duplicates so the canonical case is strictly ascending.
        let mut merged: Vec<(u32, f64)> = Vec::new();
        for &(col, value) in row.iter() {
            match merged.last_mut() {
                Some(last) if last.0 == col => last.1 += value,
                _ => merged.push((col, value)),
            }
        }
        if !canonical {
            merged.reverse();
            if let Some(position) = merged.iter().position(|&(col, _)| col as usize != i) {
                let (col, value) = merged[position];
                merged[position].1 = value * 0.5;
                merged.push((col, value * 0.5));
            }
        }
        for (col, value) in merged {
            cols.push(col);
            values.push(value);
        }
        rows.push(cols.len() as u32);
    }
    Generated {
        name,
        n,
        rows,
        cols,
        values,
    }
}

fn dyadic(random: &mut Lcg) -> f64 {
    2.0_f64.powi(random.next(9) as i32 - 4)
}

pub fn generated() -> Vec<Generated> {
    let mut out = Vec::new();
    let mut random = Lcg(0x6d77_6d67);
    for &n in &[40usize, 300, 1500] {
        for canonical in [true, false] {
            // Random sparse graph, average degree about six.
            let edges: Vec<_> = (0..3 * n)
                .map(|_| (random.next(n), random.next(n), dyadic(&mut random)))
                .collect();
            let surplus: Vec<_> = (0..n / 5)
                .map(|i| (i * 5 % n, dyadic(&mut random)))
                .collect();
            out.push(assemble(
                format!("random-{n}-{canonical}"),
                n,
                &edges,
                &surplus,
                canonical,
            ));
            out.push(assemble(
                format!("random-floating-{n}-{canonical}"),
                n,
                &edges,
                &[],
                canonical,
            ));

            // Square-ish grid.
            let side = (n as f64).sqrt() as usize;
            let mut grid = Vec::new();
            for r in 0..side {
                for c in 0..side {
                    let v = r * side + c;
                    if c + 1 < side {
                        grid.push((v, v + 1, dyadic(&mut random)));
                    }
                    if r + 1 < side {
                        grid.push((v, v + side, dyadic(&mut random)));
                    }
                }
            }
            out.push(assemble(
                format!("grid-{n}-{canonical}"),
                side * side,
                &grid,
                &[(0, 1.0)],
                canonical,
            ));

            // Hubs whose stars exceed the scatter threshold.
            let hubs: Vec<_> = (1..n)
                .map(|v| (v % 3, v, dyadic(&mut random)))
                .chain((3..n - 1).map(|v| (v, v + 1, 1.0)))
                .collect();
            out.push(assemble(
                format!("hubs-{n}-{canonical}"),
                n,
                &hubs,
                &[],
                canonical,
            ));

            // Interleaved components: vertex v belongs to component v % 4.
            // Every size is a multiple of four, so a stride of four stays in a component.
            let components: Vec<_> = (0..2 * n)
                .map(|_| {
                    let a = random.next(n);
                    let b = (a + 4 * (1 + random.next(n / 4 - 1))) % n;
                    (a, b, dyadic(&mut random))
                })
                .chain((4..n).map(|v| (v - 4, v, 1.0)))
                .collect();
            let grounded: Vec<_> = (0..n).filter(|v| v % 8 == 1).map(|v| (v, 0.5)).collect();
            out.push(assemble(
                format!("interleaved-{n}-{canonical}"),
                n,
                &components,
                &grounded,
                canonical,
            ));
        }
    }
    // Exact elimination of this path rounds a pivot to zero: a natural fallback.
    out.push(assemble(
        "pivot-cancellation".to_owned(),
        8,
        &[
            (0, 1, 1e-20),
            (1, 2, 1.0),
            (2, 3, 1e-20),
            (4, 5, 1.0),
            (5, 6, 2.0),
            (6, 7, 0.5),
        ],
        &[],
        true,
    ));
    out
}

pub fn configs() -> Vec<(&'static str, Config)> {
    let exact = |max_dim| Backend::ExactBelow {
        max_dim,
        on_failure: ExactFailure::FallBackToApproximate,
    };
    vec![
        (
            "ac-approximate",
            Config {
                seed: 7,
                split_merge: None,
                backend: Backend::Approximate,
            },
        ),
        (
            "ac-default",
            Config {
                seed: 7,
                split_merge: None,
                backend: Backend::default(),
            },
        ),
        (
            "ac2-mixed",
            Config {
                seed: 7,
                split_merge: Some(2),
                backend: exact(24),
            },
        ),
        (
            "ac3-approximate",
            Config {
                seed: 11,
                split_merge: Some(3),
                backend: Backend::Approximate,
            },
        ),
        (
            "ac-exact",
            Config {
                seed: 7,
                split_merge: None,
                backend: exact(400),
            },
        ),
    ]
}

/// Exact backends that report a failed dense factorization instead of falling back.
pub fn exact_error_configs() -> Vec<(&'static str, Config)> {
    let exact = |max_dim| Backend::ExactBelow {
        max_dim,
        on_failure: ExactFailure::Error,
    };
    vec![
        (
            "ac-exact-error",
            Config {
                seed: 7,
                split_merge: None,
                backend: exact(400),
            },
        ),
        (
            "ac2-mixed-error",
            Config {
                seed: 7,
                split_merge: Some(2),
                backend: exact(24),
            },
        ),
    ]
}
