//! CSR to elimination graph, one module per phase in pipeline order.

mod canonical;
mod sets;
mod validate;

use super::adjacency::{add_edge_pair, AdjListGraph, Edge};
use super::blocks::{BlockLayout, BlockVertices};
use super::multiplicity::EdgeCount;
use crate::construction::Account;
use crate::types::Real;
use crate::{CsrRef, Error};
use canonical::Canonical;
use validate::{validate, Grounding, Ingested};

/// Kept whole so a block routed to the dense arm never gets an adjacency list built.
pub(crate) struct Ingestion<'a, T> {
    canonical: Canonical<'a, T>,
    /// `take_block_diagonal` empties this, so `n` cannot be its length.
    diagonal: Vec<T>,
    n: usize,
    grounding: Grounding<T>,
    layout: Option<BlockLayout>,
}

impl<'a, T: Real> Ingestion<'a, T> {
    pub(crate) fn of(csr: CsrRef<'a, T, u32>, ledger: &mut impl Account) -> Result<Self, Error> {
        let canonical = Canonical::of(csr, ledger)?;
        let Ingested {
            diagonal,
            grounding,
            layout,
        } = validate(&canonical, ledger)?;
        Ok(Self {
            canonical,
            n: diagonal.len(),
            diagonal,
            grounding,
            layout,
        })
    }

    /// Release every array still owned here; the layout must already be taken.
    pub(crate) fn free(self, ledger: &mut impl Account) {
        debug_assert!(self.layout.is_none(), "the layout is released by its taker");
        self.canonical.free(ledger);
        ledger.free(self.diagonal);
        if let Grounding::Grounded { surpluses, .. } = self.grounding {
            ledger.free(surpluses);
        }
    }

    /// Vertices the factorization covers, the ground one included.
    pub(crate) fn n(&self) -> usize {
        self.n
    }

    /// The ground vertex outranks every real one, so it can only be a block's last.
    pub(crate) fn carries_ground(&self, block: &BlockVertices<'_>) -> bool {
        match &self.grounding {
            Grounding::Floating => false,
            Grounding::Grounded { surpluses, .. } => block.last() == surpluses.len() as u32,
        }
    }

    /// `None` when connected. Taken so the caller can walk blocks while asking for each.
    pub(crate) fn take_layout(&mut self) -> Option<BlockLayout> {
        self.layout.take()
    }

    /// The diagonal entry the block's row `local` carries.
    pub(crate) fn block_diagonal(&self, block: &BlockVertices<'_>, local: usize) -> T {
        self.diagonal[block.global(local)]
    }

    /// Upper, not the stored lower: mirrors may differ by ulps and the approximate
    /// route symmetrizes on this one.
    pub(crate) fn upper_row(
        &self,
        block: &BlockVertices<'_>,
        local: usize,
        mut entry: impl FnMut(usize, T),
    ) {
        let (row_ptrs, col_indices, values) = self.canonical.arrays();
        let row = block.global(local);
        if row + 1 >= row_ptrs.len() {
            return;
        }
        let (from, to) = (row_ptrs[row] as usize, row_ptrs[row + 1] as usize);
        for (&col, &value) in col_indices[from..to].iter().zip(&values[from..to]) {
            if col as usize > row && value != T::zero() {
                entry(block.local(col as usize), value);
            }
        }
    }

    /// The whole graph is moved out: it is one block, so nothing reads it again.
    pub(crate) fn take_block_diagonal(
        &mut self,
        block: &BlockVertices<'_>,
        ledger: &mut impl Account,
    ) -> Result<Vec<T>, Error> {
        match block {
            // Ownership moves with its charge; the emptied field releases nothing.
            BlockVertices::Whole(_) => Ok(core::mem::take(&mut self.diagonal)),
            BlockVertices::Part { vertices, .. } => {
                let mut diagonal = ledger.with_capacity(vertices.len())?;
                diagonal.extend(
                    vertices
                        .iter()
                        .map(|&vertex| self.diagonal[vertex as usize]),
                );
                Ok(diagonal)
            }
        }
    }

    /// Builds the block's adjacency, which only the approximate arm needs.
    pub(crate) fn block_graph<C: EdgeCount>(
        &self,
        block: &BlockVertices<'_>,
        ledger: &mut impl Account,
    ) -> Result<AdjListGraph<C, T>, Error> {
        let (row_ptrs, col_indices, values) = self.canonical.arrays();
        let rows = row_ptrs.len() - 1;
        let n = block.len();
        // Every grounded row shares one block, so its degree is the whole count.
        let ground_degree = match self.grounding {
            Grounding::Floating => 0,
            Grounding::Grounded { degree, .. } => degree,
        };

        let mut adj: Vec<Vec<Edge<T, C>>> = ledger.with_capacity(n)?;
        for local in 0..n {
            let global = block.global(local);
            let degree = if global < rows {
                (row_ptrs[global + 1] - row_ptrs[global]) as usize
            } else {
                ground_degree
            };
            adj.push(ledger.with_capacity(degree)?);
        }

        // Measured: an in-loop discriminant test spills `local_of` and reloads per edge.
        match block {
            BlockVertices::Whole(_) => {
                for local in 0..n {
                    if local >= rows {
                        continue;
                    }
                    let (from, to) = (row_ptrs[local] as usize, row_ptrs[local + 1] as usize);
                    for (&col, &value) in col_indices[from..to].iter().zip(&values[from..to]) {
                        if col as usize > local && value != T::zero() {
                            add_edge_pair(&mut adj, local, col as usize, -value, ledger)?;
                        }
                    }
                }
            }
            BlockVertices::Part { vertices, local_of } => {
                // Narrowed so the bound lives in a register.
                let local_of = &local_of[..rows];
                for (local, &global) in vertices.iter().enumerate() {
                    let global = global as usize;
                    if global >= rows {
                        continue;
                    }
                    let (from, to) = (row_ptrs[global] as usize, row_ptrs[global + 1] as usize);
                    for (&col, &value) in col_indices[from..to].iter().zip(&values[from..to]) {
                        if col as usize > global && value != T::zero() {
                            add_edge_pair(
                                &mut adj,
                                local,
                                local_of[col as usize] as usize,
                                -value,
                                ledger,
                            )?;
                        }
                    }
                }
            }
        }

        if let Grounding::Grounded { surpluses, .. } = &self.grounding {
            if self.carries_ground(block) {
                let ground = n - 1;
                for (row, &surplus) in surpluses.iter().enumerate() {
                    // The clamp in `ground` left every surplus non-negative.
                    if surplus > T::zero() {
                        add_edge_pair(&mut adj, block.local(row), ground, surplus, ledger)?;
                    }
                }
            }
        }

        AdjListGraph::from_adjacency_with(adj, ledger)
    }
}

#[cfg(test)]
mod tests;
