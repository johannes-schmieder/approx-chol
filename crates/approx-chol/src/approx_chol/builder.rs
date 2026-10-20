use super::config::{Backend, Config, Route};
use super::factorization::{
    approximate, exact, Anchor, Block, BlockDim, Cholesky, Fallback, Permutation,
};
use crate::construction::{Account, ConstructionLimit, ConstructionReport, Ledger, Unaccounted};
use crate::graph::{BlockVertices, EdgeCount, Ingestion, Multi, Single};
use crate::sampling::CdfSampler;
use crate::types::Real;
use crate::{CsrError, CsrRef, Error, Factor};
use num_traits::PrimInt;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, Clone)]
/// Factorization pipeline behind [`factorize_with`](crate::factorize_with).
pub struct Builder<T = f64> {
    config: Config,
    _scalar: core::marker::PhantomData<T>,
}

impl<T> Builder<T>
where
    T: num_traits::Float + Send + Sync + 'static,
{
    #[must_use]
    /// Create a builder with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _scalar: core::marker::PhantomData,
        }
    }

    /// Factorize any input fallibly convertible into [`CsrRef`].
    ///
    /// Runs the same construction as [`Self::build_with_limit`] without accounting,
    /// so it returns the same factor without its cost.
    pub fn build<'a, I, M>(&self, sddm: M) -> Result<Factor<T>, Error>
    where
        I: PrimInt + 'a + 'static,
        M: TryInto<CsrRef<'a, T, I>>,
        <M as TryInto<CsrRef<'a, T, I>>>::Error: Into<Error>,
    {
        self.build_accounted(sddm, &mut Unaccounted)
    }

    /// Factorize within a declared live construction payload.
    ///
    /// Every construction array, including reallocation overlap, the index copy,
    /// a failed exact block's dense copy and the retained factor, is charged before
    /// it is allocated. Exceeding `limit` returns
    /// [`Error::ConstructionBudgetExceeded`] without the refused allocation and
    /// without a factor. A successful build returns exactly the factor
    /// [`Self::build`] returns, whatever the limit, with its peak and retained
    /// payload. An allocator refusal of a dense exact copy still falls back as in
    /// [`Self::build`]; any other refused reservation returns
    /// [`Error::AllocationFailed`]. Construction is serial.
    pub fn build_with_limit<'a, I, M>(
        &self,
        sddm: M,
        limit: ConstructionLimit,
    ) -> Result<(Factor<T>, ConstructionReport), Error>
    where
        I: PrimInt + 'a + 'static,
        M: TryInto<CsrRef<'a, T, I>>,
        <M as TryInto<CsrRef<'a, T, I>>>::Error: Into<Error>,
    {
        let mut ledger = Ledger::new(limit);
        let factor = self.build_accounted(sddm, &mut ledger)?;
        let report = ledger.report();
        debug_assert_eq!(
            factor.retained_heap_bytes(),
            Some(report.retained_payload_bytes),
            "every transient construction array is released"
        );
        Ok((factor, report))
    }

    /// [`Self::build_with_limit`], reporting the payload accounting whether or not
    /// the build succeeds.
    ///
    /// A failed or refused build reports the peak live payload it reached before
    /// failing and zero retained payload, since everything it allocated is
    /// released. A caller composing its own budget around a build that may fail
    /// and be retried differently, such as an exact attempt under
    /// [`ExactFailure::Error`](crate::ExactFailure::Error), needs that peak. A
    /// successful build returns exactly what [`Self::build_with_limit`] returns.
    pub fn build_with_limit_reported<'a, I, M>(
        &self,
        sddm: M,
        limit: ConstructionLimit,
    ) -> (Result<Factor<T>, Error>, ConstructionReport)
    where
        I: PrimInt + 'a + 'static,
        M: TryInto<CsrRef<'a, T, I>>,
        <M as TryInto<CsrRef<'a, T, I>>>::Error: Into<Error>,
    {
        let mut ledger = Ledger::new(limit);
        let built = self.build_accounted(sddm, &mut ledger);
        let mut report = ledger.report();
        match &built {
            Ok(factor) => debug_assert_eq!(
                factor.retained_heap_bytes(),
                Some(report.retained_payload_bytes),
                "every transient construction array is released"
            ),
            Err(_) => report.retained_payload_bytes = 0,
        }
        (built, report)
    }

    fn build_accounted<'a, I, M>(
        &self,
        sddm: M,
        account: &mut impl Account,
    ) -> Result<Factor<T>, Error>
    where
        I: PrimInt + 'a + 'static,
        M: TryInto<CsrRef<'a, T, I>>,
        <M as TryInto<CsrRef<'a, T, I>>>::Error: Into<Error>,
    {
        let csr = catch_unwind(AssertUnwindSafe(|| sddm.try_into()))
            .map_err(|_| Error::InvalidCsr(CsrError::InputConversionPanicked))?;
        let csr = csr.map_err(Into::into)?;
        let narrowed = csr.narrow_indices_with(account)?;
        let factor = self.build_validated(narrowed.with_values(csr.values()), account)?;
        narrowed.free(account);
        Ok(factor)
    }

    /// The multiplicity decides layout and split together, so each arm names one
    /// algorithm end to end.
    fn build_validated(
        &self,
        sddm: CsrRef<'_, T, u32>,
        ledger: &mut impl Account,
    ) -> Result<Factor<T>, Error> {
        let original_n = sddm.n();
        let ingestion = Ingestion::of(sddm, ledger)?;
        match self.config.split_factor() {
            None => self.factor_blocks::<Single>(ingestion, (), ledger),
            Some(k) => self.factor_blocks::<Multi>(ingestion, k, ledger),
        }
        // The only scope holding both the caller's dimension and the finished factor.
        .inspect(|factor| debug_assert_eq!(factor.original_n(), original_n))
    }

    fn factor_blocks<C: EdgeCount>(
        &self,
        mut ingestion: Ingestion<'_, T>,
        split: C::Split,
        ledger: &mut impl Account,
    ) -> Result<Factor<T>, Error> {
        if ingestion.n() == 0 {
            ingestion.free(ledger);
            return Ok(Factor::empty());
        }
        let mut factorizer = BlockFactorizer::<T, C>::new(self.config, split);
        let Some(layout) = ingestion.take_layout() else {
            let whole = BlockVertices::whole(ingestion.n());
            let (block, fallback) = factorizer.factor(&mut ingestion, &whole, ledger)?;
            let mut blocks = ledger.with_capacity(1)?;
            blocks.push(block);
            let mut fallbacks = ledger.with_capacity(usize::from(fallback.is_some()))?;
            fallbacks.extend(fallback);
            factorizer.sampler.free(ledger);
            ingestion.free(ledger);
            return Ok(Factor::from_blocks(None, blocks, fallbacks));
        };

        let mut blocks = ledger.with_capacity(layout.block_count())?;
        let mut fallbacks = Vec::new();
        // Scratch reused across blocks; each view refills the entries it names.
        let mut local_of = ledger.filled(ingestion.n(), 0u32)?;
        for vertices in layout.blocks() {
            let view = BlockVertices::part(vertices, &mut local_of);
            let (block, fallback) = factorizer.factor(&mut ingestion, &view, ledger)?;
            blocks.push(block);
            if let Some(fallback) = fallback {
                ledger.push(&mut fallbacks, fallback)?;
            }
        }
        ledger.free(local_of);
        factorizer.sampler.free(ledger);
        ingestion.free(ledger);
        let order = layout.into_order(ledger);
        let capacity = order.capacity();
        let permutation = Permutation::from_order(order);
        if permutation.is_none() {
            // The identity order is dropped rather than stored.
            ledger.forget::<u32>(capacity);
        }
        Ok(Factor::from_blocks(permutation, blocks, fallbacks))
    }
}

/// What every block shares, resolved — including the sampler each block restarts its
/// own stream from, which is why [`Config`] does not survive construction.
struct BlockFactorizer<T: Real, C: EdgeCount> {
    backend: Backend,
    sampler: CdfSampler<T>,
    split: C::Split,
}

impl<T: Real, C: EdgeCount> BlockFactorizer<T, C> {
    fn new(config: Config, split: C::Split) -> Self {
        Self {
            sampler: CdfSampler::new(config.seed),
            backend: config.backend,
            split,
        }
    }

    /// Routing first, so a block the dense backend claims never has an elimination
    /// graph built for it — only a fallback from that arm, or the approximate route,
    /// reaches [`Ingestion::block_graph`].
    fn factor(
        &mut self,
        ingestion: &mut Ingestion<'_, T>,
        block: &BlockVertices<'_>,
        ledger: &mut impl Account,
    ) -> Result<(Block<T>, Option<Fallback>), Error> {
        // Restarts for every block, routed or not, so one block's draws never shift
        // because another was factored exactly.
        self.sampler.restart(block.first());

        let dim = BlockDim::of(block.len()).expect("a block has at least one vertex");
        let anchor = if ingestion.carries_ground(block) {
            Anchor::Ground
        } else {
            Anchor::Floating
        };
        let mut fallback = None;
        if let Route::Exact { on_failure } = self.backend.route(dim) {
            match exact::factor(ingestion, block, dim, ledger) {
                Ok(lower) => return Ok((Block::new(dim, anchor, Cholesky::Exact(lower)), None)),
                Err(exact::NotFactorable::Refused(error)) => return Err(error),
                Err(reason) => {
                    fallback = Some(on_failure.accept(reason.at(block))?);
                }
            }
        }
        let graph = ingestion.block_graph::<C>(block, ledger)?;
        let diagonal = ingestion.take_block_diagonal(block, ledger)?;
        let sequence =
            approximate::eliminate::<T, C>(graph, diagonal, &mut self.sampler, self.split, ledger)?;
        Ok((
            Block::new(dim, anchor, Cholesky::Approximate(sequence)),
            fallback,
        ))
    }
}

#[cfg(test)]
mod tests;
