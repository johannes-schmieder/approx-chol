use super::block::Block;
use super::permutation::Permutation;
#[cfg(any(feature = "serde", test))]
use super::FactorError;
use core::fmt;

#[cfg(test)]
mod tests;

/// The encoding a persisted [`Factor`] declares as its first field, incremented in the
/// low half whenever the serialized representation changes in a way an older reader would
/// misread. A non-self-describing format reads the field positionally, so the tag half
/// keeps a payload that predates the field from passing the check on whatever `usize` led
/// it — `1` would collide with the dimension a one-variable system led with.
#[cfg(feature = "serde")]
pub const FACTOR_FORMAT_VERSION: u32 = 0x4143_0003;

#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        bound(deserialize = "T: serde::de::DeserializeOwned + num_traits::Float"),
        try_from = "FactorData<T>"
    )
)]
#[derive(Clone, Debug)]
/// Exact or approximate Cholesky decomposition of an SDDM matrix.
pub struct Factor<T = f64> {
    n: usize,
    permutation: Option<Permutation>,
    blocks: Vec<Block<T>>,
    fallbacks: Vec<Fallback>,
}

/// Borrows what it writes, so declaring the version costs no copy of the factor.
/// Field order and names match [`FactorData`], which is what reads it back.
#[cfg(feature = "serde")]
#[derive(serde::Serialize)]
#[serde(bound(serialize = "T: serde::Serialize"))]
struct FactorRef<'a, T> {
    format_version: u32,
    permutation: Option<&'a Permutation>,
    blocks: &'a [Block<T>],
    fallbacks: &'a [Fallback],
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for Factor<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FactorRef {
            format_version: FACTOR_FORMAT_VERSION,
            permutation: self.permutation.as_ref(),
            blocks: &self.blocks,
            fallbacks: &self.fallbacks,
        }
        .serialize(serializer)
    }
}

/// `format_version` defaults rather than being required, so a payload that predates the
/// field is rejected for the version it implies instead of for a missing field.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned + num_traits::Float"))]
struct FactorData<T> {
    #[serde(default)]
    format_version: u32,
    permutation: Option<Permutation>,
    blocks: Vec<Block<T>>,
    #[serde(default)]
    fallbacks: Vec<Fallback>,
}

#[cfg(feature = "serde")]
impl<T: num_traits::Float> TryFrom<FactorData<T>> for Factor<T> {
    type Error = FactorError;

    fn try_from(data: FactorData<T>) -> Result<Self, Self::Error> {
        if data.format_version != FACTOR_FORMAT_VERSION {
            return Err(FactorError::UnsupportedFormatVersion {
                found: data.format_version,
                supported: FACTOR_FORMAT_VERSION,
            });
        }
        let factor = Self::of(data.permutation, data.blocks, data.fallbacks);
        factor.validate_structure()?;
        Ok(factor)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Why a block [`Backend::ExactBelow`](crate::Backend::ExactBelow) claimed was factored approximately.
pub enum Fallback {
    /// Dense elimination reached a pivot it could not use.
    InvalidPivot(crate::UnusablePivot),
    /// The dense copy would not fit in memory; never fatal, whatever [`ExactFailure`](crate::ExactFailure) says.
    WillNotFit {
        /// Variables the block solves for, so the copy is `dim * dim` scalars.
        dim: usize,
    },
}

impl fmt::Display for Fallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPivot(pivot) => write!(f, "{pivot}"),
            Self::WillNotFit { dim } => write!(f, "{dim} variables do not fit in memory"),
        }
    }
}

/// Every block arrives already checked against its own cholesky, so what is left is what
/// no single block can see.
#[cfg(any(feature = "serde", test))]
impl<T> Factor<T> {
    fn validate_structure(&self) -> Result<(), FactorError> {
        // A Ground anchor overwrites its block's last entry with `-sum`, so a second
        // one silently solves a different system.
        let grounded = Self::ground_blocks(&self.blocks);
        if grounded > 1 {
            return Err(FactorError::MultipleGroundBlocks { grounded });
        }
        if let Some(permutation) = &self.permutation {
            permutation.validate_for_dim(self.n)?;
        }
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
/// Errors returned by fallible [`Factor`] solve methods.
pub enum SolveError {
    /// Right-hand side longer than the solvable (original) dimension.
    RhsLengthExceedsFactor {
        /// Provided RHS length.
        rhs_len: usize,
        /// Maximum accepted RHS length.
        factor_dim: usize,
    },
    /// Permutation scratch smaller than [`Factor::permutation_scratch_len`].
    ScratchBufferTooSmall {
        /// Provided scratch length.
        scratch_len: usize,
        /// Required scalar count.
        required: usize,
    },
    /// Work buffer smaller than the factor dimension.
    WorkBufferTooSmall {
        /// Provided work length.
        work_len: usize,
        /// Factor dimension (`Factor::n()`).
        factor_dim: usize,
    },
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RhsLengthExceedsFactor {
                rhs_len,
                factor_dim,
            } => write!(
                f,
                "rhs length {rhs_len} exceeds original matrix dimension {factor_dim}"
            ),
            Self::ScratchBufferTooSmall {
                scratch_len,
                required,
            } => write!(
                f,
                "permutation scratch too small: got {scratch_len}, need at least {required}"
            ),
            Self::WorkBufferTooSmall {
                work_len,
                factor_dim,
            } => write!(
                f,
                "work buffer too small: got {work_len}, need at least {factor_dim}"
            ),
        }
    }
}

impl std::error::Error for SolveError {}

impl<T> Factor<T> {
    /// Blocks routed to exact Cholesky and factored approximately anyway.
    pub fn fallbacks(&self) -> &[Fallback] {
        &self.fallbacks
    }

    /// Dimension of the original input matrix.
    pub fn original_n(&self) -> usize {
        self.n - Self::ground_blocks(&self.blocks)
    }

    /// Internal factor dimension, including a possible ground vertex.
    #[inline]
    pub fn n(&self) -> usize {
        self.n
    }

    /// Scalar entries needed by the supplied-scratch solve methods.
    ///
    /// Zero for block-contiguous input, otherwise [`Self::n`], including ground
    /// augmentation. The gather overwrites every consumed entry; initial values
    /// and any unused tail do not matter. This query allocates nothing.
    pub fn permutation_scratch_len(&self) -> usize {
        if self.permutation.is_some() {
            self.n
        } else {
            0
        }
    }

    /// Checked sum of this factor's retained heap-array capacities in bytes.
    ///
    /// Includes block/fallback descriptors, the optional permutation and every
    /// nested exact/approximate factor array, including spare capacity. Excludes
    /// inline `Factor` storage, allocator overhead/excess beyond reported vector
    /// capacity, caller scratch and construction peak. This is a post-build
    /// inventory, not construction admission. Returns `None` on arithmetic
    /// overflow. The query allocates nothing and takes O(number of blocks) time.
    pub fn retained_heap_bytes(&self) -> Option<usize> {
        use super::capacity_bytes;
        let mut bytes = capacity_bytes::<Block<T>>(self.blocks.capacity())?
            .checked_add(capacity_bytes::<Fallback>(self.fallbacks.capacity())?)?;
        if let Some(permutation) = &self.permutation {
            bytes = bytes.checked_add(capacity_bytes::<u32>(permutation.forward.capacity())?)?;
        }
        for block in &self.blocks {
            bytes = bytes.checked_add(block.retained_heap_bytes()?)?;
        }
        Some(bytes)
    }

    /// The one place `n` is ever written, so it cannot drift from the blocks it sums.
    fn of(
        permutation: Option<Permutation>,
        blocks: Vec<Block<T>>,
        fallbacks: Vec<Fallback>,
    ) -> Self {
        Self {
            n: blocks.iter().map(|block| block.dim().total()).sum(),
            permutation,
            blocks,
            fallbacks,
        }
    }

    /// Nothing else records that the ground vertex exists, and at most one block can
    /// hold it.
    fn ground_blocks(blocks: &[Block<T>]) -> usize {
        blocks.iter().filter(|block| block.is_ground()).count()
    }
}

impl<T> Factor<T>
where
    T: num_traits::Float + Send + Sync + 'static,
{
    pub(crate) fn from_blocks(
        permutation: Option<Permutation>,
        blocks: Vec<Block<T>>,
        fallbacks: Vec<Fallback>,
    ) -> Self {
        let factor = Self::of(permutation, blocks, fallbacks);
        #[cfg(any(feature = "serde", test))]
        debug_assert_eq!(factor.validate_structure(), Ok(()));
        factor
    }

    pub(crate) fn empty() -> Self {
        Self::from_blocks(None, Vec::new(), Vec::new())
    }

    /// Total elimination steps across all blocks: every block solves for all but one of
    /// its variables, whichever arm factored it.
    pub fn n_steps(&self) -> usize {
        self.blocks.iter().map(|block| block.dim().solved()).sum()
    }

    #[inline]
    fn validate_work(&self, work: &[T]) -> Result<(), SolveError> {
        if work.len() < self.n() {
            return Err(SolveError::WorkBufferTooSmall {
                work_len: work.len(),
                factor_dim: self.n(),
            });
        }
        Ok(())
    }

    fn solve_kernel(&self, b: &[T], work: &mut [T]) {
        work[..b.len()].copy_from_slice(b);
        work[b.len()..self.n()].fill(T::zero());
        self.for_each_block(work, Block::solve_canonical);
    }

    fn for_each_block(&self, values: &mut [T], solve: impl FnMut(&Block<T>, &mut [T])) {
        let mut scratch = vec![T::zero(); self.permutation_scratch_len()];
        self.for_each_block_with_scratch(values, &mut scratch, solve);
    }

    fn for_each_block_with_scratch(
        &self,
        values: &mut [T],
        scratch: &mut [T],
        mut solve: impl FnMut(&Block<T>, &mut [T]),
    ) {
        let values = &mut values[..self.n()];
        let Some(permutation) = &self.permutation else {
            Self::solve_blocks(&self.blocks, values, &mut solve);
            return;
        };
        let scratch = &mut scratch[..self.n()];
        permutation.gather_into(values, scratch);
        Self::solve_blocks(&self.blocks, scratch, &mut solve);
        permutation.scatter_from(scratch, values);
    }

    fn validate_scratch(&self, scratch: &[T]) -> Result<(), SolveError> {
        let required = self.permutation_scratch_len();
        if scratch.len() < required {
            return Err(SolveError::ScratchBufferTooSmall {
                scratch_len: scratch.len(),
                required,
            });
        }
        Ok(())
    }

    #[inline(always)]
    fn solve_blocks(
        blocks: &[Block<T>],
        values: &mut [T],
        solve: &mut impl FnMut(&Block<T>, &mut [T]),
    ) {
        let mut start = 0usize;
        for block in blocks {
            let end = start + block.dim().total();
            solve(block, &mut values[start..end]);
            start = end;
        }
    }

    /// Solve `M x = b`, returning the zero-mean least-squares solution for singular `M`.
    pub fn solve(&self, b: &[T]) -> Result<Vec<T>, SolveError> {
        let mut work = vec![T::zero(); self.n()];
        self.solve_into(b, &mut work)?;
        work.truncate(self.original_n());
        Ok(work)
    }

    /// Solve `M x = b` into a caller-provided work buffer.
    pub fn solve_into(&self, b: &[T], work: &mut [T]) -> Result<(), SolveError> {
        let original_n = self.original_n();
        if b.len() > original_n {
            return Err(SolveError::RhsLengthExceedsFactor {
                rhs_len: b.len(),
                factor_dim: original_n,
            });
        }
        self.validate_work(work)?;
        self.solve_kernel(b, work);
        Ok(())
    }

    /// Solve into caller work and permutation scratch without allocating.
    ///
    /// Uses the same canonical least-squares action as [`Self::solve_into`].
    /// `b` may be shorter than [`Self::original_n`]; missing entries are zero.
    /// Requires at least [`Self::n`] work entries and
    /// [`Self::permutation_scratch_len`] scratch entries. Validates RHS, work,
    /// then scratch sizes before any write. Errors preserve both buffers; success
    /// leaves their unused tails untouched. No numerical finiteness certificate
    /// is implied by success, just as with the existing solve methods.
    pub fn solve_into_with_scratch(
        &self,
        b: &[T],
        work: &mut [T],
        scratch: &mut [T],
    ) -> Result<(), SolveError> {
        let original_n = self.original_n();
        if b.len() > original_n {
            return Err(SolveError::RhsLengthExceedsFactor {
                rhs_len: b.len(),
                factor_dim: original_n,
            });
        }
        self.validate_work(work)?;
        self.validate_scratch(scratch)?;
        work[..b.len()].copy_from_slice(b);
        work[b.len()..self.n()].fill(T::zero());
        self.for_each_block_with_scratch(work, scratch, Block::solve_canonical);
        Ok(())
    }

    /// Solve in place using caller permutation scratch without allocating.
    ///
    /// Uses the same anchored action as [`Self::solve_in_place`]. Requires at
    /// least [`Self::n`] value entries (including any ground augmentation) and
    /// [`Self::permutation_scratch_len`] scratch entries. All sizes validate
    /// before either buffer is touched. On success, unused tails remain intact;
    /// scratch need not be initialized. No threads are started or entered.
    pub fn solve_in_place_with_scratch(
        &self,
        values: &mut [T],
        scratch: &mut [T],
    ) -> Result<(), SolveError> {
        self.validate_work(values)?;
        self.validate_scratch(scratch)?;
        self.for_each_block_with_scratch(values, scratch, Block::solve_anchored);
        Ok(())
    }

    /// Solve in place, leaving one variable per block pinned to zero.
    pub fn solve_in_place(&self, values: &mut [T]) -> Result<(), SolveError> {
        self.validate_work(values)?;
        self.for_each_block(values, Block::solve_anchored);
        Ok(())
    }
}
