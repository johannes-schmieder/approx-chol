//! Checked live-payload accounting for factor construction.
//!
//! Every construction array is reserved through one [`Ledger`], which charges the
//! requested capacity before allocating and keeps the old capacity charged while a
//! growing array is reallocated. Exceeding the declared limit fails before the
//! allocation it would have needed, so a refused build never touches the
//! allocator beyond what the ledger already admitted. Numerics never depend on the
//! limit: a build either returns the same factor as an unlimited one or an error.

use crate::Error;

/// Declared ceiling on live construction payload.
///
/// Counts requested heap-array capacity of every construction and retained factor
/// array, including the old and new buffers of a reallocation. Excludes the
/// caller's input, inline objects, stack, allocator headers and excess capacity.
/// This is not a process-memory quota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionLimit {
    /// Inclusive ceiling in bytes.
    pub maximum_payload_bytes: usize,
}

impl ConstructionLimit {
    /// No ceiling; the accounting still runs and is reported.
    pub const UNLIMITED: Self = Self {
        maximum_payload_bytes: usize::MAX,
    };
}

/// Payload accounting of one successful build.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConstructionReport {
    /// Peak live requested payload in bytes, including reallocation overlap.
    pub peak_payload_bytes: usize,
    /// Payload still charged when the factor is returned; equals
    /// [`Factor::retained_heap_bytes`](crate::Factor::retained_heap_bytes).
    pub retained_payload_bytes: usize,
}

/// Live requested payload against one declared limit.
pub(crate) struct Ledger {
    live: usize,
    peak: usize,
    limit: usize,
}

impl Ledger {
    pub(crate) fn new(limit: ConstructionLimit) -> Self {
        Self {
            live: 0,
            peak: 0,
            limit: limit.maximum_payload_bytes,
        }
    }

    pub(crate) fn report(&self) -> ConstructionReport {
        ConstructionReport {
            peak_payload_bytes: self.peak,
            retained_payload_bytes: self.live,
        }
    }

    /// A size no allocation can satisfy is an allocation failure, as the standard
    /// library's capacity overflow is; only a representable excess is a budget refusal.
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        let required = self
            .live
            .checked_add(bytes)
            .filter(|&required| required <= isize::MAX as usize)
            .ok_or(Error::AllocationFailed { bytes })?;
        if required > self.limit {
            return Err(Error::ConstructionBudgetExceeded {
                required,
                limit: self.limit,
            });
        }
        self.live = required;
        self.peak = self.peak.max(required);
        Ok(())
    }

    fn release(&mut self, bytes: usize) {
        debug_assert!(bytes <= self.live, "released more payload than charged");
        self.live -= bytes;
    }

    fn bytes<T>(capacity: usize) -> Result<usize, Error> {
        capacity
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(Error::AllocationFailed { bytes: usize::MAX })
    }

    /// The reallocating half of [`Account::ensure`], kept out of the elimination loop.
    #[cold]
    #[inline(never)]
    fn grow<T>(&mut self, values: &mut Vec<T>, total: usize) -> Result<(), Error> {
        let old = values.capacity();
        let target = total.max(old.saturating_mul(2));
        let requested = Self::bytes::<T>(target)?;
        self.charge(requested)?;
        if values.try_reserve_exact(target - values.len()).is_err() {
            self.release(requested);
            return Err(Error::AllocationFailed { bytes: requested });
        }
        // Charge any capacity the allocator handed out beyond the request.
        self.charge(Self::bytes::<T>(values.capacity() - target)?)?;
        self.release(Self::bytes::<T>(old)?);
        Ok(())
    }
}

/// How construction reserves arrays: [`Ledger`] charges and checks every one;
/// [`Unaccounted`] is the plain standard-library path of [`Builder::build`].
///
/// [`Builder::build`]: crate::low_level::Builder::build
pub(crate) trait Account {
    /// Grow `values` to hold at least `total` elements without another reallocation.
    fn ensure<T>(&mut self, values: &mut Vec<T>, total: usize) -> Result<(), Error>;

    /// Drop an array, releasing its whole capacity.
    fn free<T>(&mut self, values: Vec<T>);

    /// Release the capacity of an array the caller has already dropped or replaced.
    fn forget<T>(&mut self, capacity: usize);

    /// Charge library-internal scratch of up to `count` elements, such as a stable
    /// sort's merge buffer, for the duration of one operation.
    fn begin_transient<T>(&mut self, count: usize) -> Result<usize, Error>;

    /// End a [`Self::begin_transient`] charge.
    fn end_transient(&mut self, bytes: usize);

    /// An empty array with room for `capacity` elements.
    fn with_capacity<T>(&mut self, capacity: usize) -> Result<Vec<T>, Error> {
        let mut values = Vec::new();
        self.ensure(&mut values, capacity)?;
        Ok(values)
    }

    /// `len` copies of `value`.
    fn filled<T: Clone>(&mut self, len: usize, value: T) -> Result<Vec<T>, Error> {
        let mut values = self.with_capacity(len)?;
        values.resize(len, value);
        Ok(values)
    }

    /// Room for `additional` more elements beyond the current length.
    #[inline]
    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<(), Error> {
        let total = values
            .len()
            .checked_add(additional)
            .ok_or(Error::AllocationFailed { bytes: usize::MAX })?;
        self.ensure(values, total)
    }

    /// Push one element, growing through [`Self::ensure`] when full.
    #[inline]
    fn push<T>(&mut self, values: &mut Vec<T>, value: T) -> Result<(), Error> {
        if values.len() == values.capacity() {
            self.reserve(values, 1)?;
        }
        values.push(value);
        Ok(())
    }
}

/// The unaccounted path: standard amortized growth, with fallible reservations
/// mapped to [`Error::AllocationFailed`] so a refused dense copy still falls back.
pub(crate) struct Unaccounted;

impl Account for Unaccounted {
    #[inline]
    fn ensure<T>(&mut self, values: &mut Vec<T>, total: usize) -> Result<(), Error> {
        if total <= values.capacity() {
            return Ok(());
        }
        values
            .try_reserve(total - values.len())
            .map_err(|_| Error::AllocationFailed { bytes: usize::MAX })
    }

    #[inline]
    fn free<T>(&mut self, values: Vec<T>) {
        drop(values);
    }

    #[inline]
    fn forget<T>(&mut self, _capacity: usize) {}

    #[inline]
    fn begin_transient<T>(&mut self, _count: usize) -> Result<usize, Error> {
        Ok(0)
    }

    #[inline]
    fn end_transient(&mut self, _bytes: usize) {}

    #[inline]
    fn push<T>(&mut self, values: &mut Vec<T>, value: T) -> Result<(), Error> {
        values.push(value);
        Ok(())
    }
}

impl Account for Ledger {
    /// Grow `values` to hold at least `total` elements without another reallocation.
    ///
    /// Grows to `max(total, 2 * capacity)`, charging the new buffer while the old
    /// one is still live. A no-op when the capacity already suffices.
    #[inline]
    fn ensure<T>(&mut self, values: &mut Vec<T>, total: usize) -> Result<(), Error> {
        if total <= values.capacity() || core::mem::size_of::<T>() == 0 {
            return Ok(());
        }
        self.grow(values, total)
    }

    /// Charge library-internal scratch of up to `count` elements, such as a stable
    /// sort's merge buffer, for the duration of one operation.
    fn begin_transient<T>(&mut self, count: usize) -> Result<usize, Error> {
        let bytes = Self::bytes::<T>(count)?;
        self.charge(bytes)?;
        Ok(bytes)
    }

    /// End a [`Self::begin_transient`] charge.
    fn end_transient(&mut self, bytes: usize) {
        self.release(bytes);
    }

    /// Drop an array, releasing its whole capacity.
    fn free<T>(&mut self, values: Vec<T>) {
        let capacity = values.capacity();
        drop(values);
        if core::mem::size_of::<T>() != 0 {
            self.release(capacity * core::mem::size_of::<T>());
        }
    }

    /// Release the capacity of an array the caller has already dropped or replaced.
    fn forget<T>(&mut self, capacity: usize) {
        self.release(capacity * core::mem::size_of::<T>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_charges_overlap_and_limits_are_inclusive() {
        let mut ledger = Ledger::new(ConstructionLimit {
            maximum_payload_bytes: 8 * 12,
        });
        let mut values: Vec<u64> = ledger.with_capacity(4).unwrap();
        assert_eq!(ledger.report().peak_payload_bytes, 32);
        values.extend([1, 2, 3, 4]);
        ledger.push(&mut values, 5).unwrap();
        // Old four and new eight coexist during the move: exactly the limit.
        assert_eq!(ledger.report().peak_payload_bytes, 96);
        assert_eq!(ledger.report().retained_payload_bytes, 64);
        assert_eq!(values, [1, 2, 3, 4, 5]);
        ledger.free(values);
        assert_eq!(ledger.report().retained_payload_bytes, 0);

        let mut tight = Ledger::new(ConstructionLimit {
            maximum_payload_bytes: 95,
        });
        let mut values: Vec<u64> = tight.filled(4, 0).unwrap();
        assert_eq!(
            tight.push(&mut values, 1),
            Err(Error::ConstructionBudgetExceeded {
                required: 96,
                limit: 95
            })
        );
        assert_eq!(values.capacity(), 4, "a refused growth allocates nothing");
    }

    #[test]
    fn impossible_sizes_are_refused_before_allocation() {
        let mut ledger = Ledger::new(ConstructionLimit::UNLIMITED);
        for refused in [
            ledger.with_capacity::<u64>(usize::MAX).map(drop),
            ledger
                .with_capacity::<u8>(isize::MAX as usize + 1)
                .map(drop),
        ] {
            assert!(matches!(refused, Err(Error::AllocationFailed { .. })));
        }
        assert_eq!(ledger.report().peak_payload_bytes, 0);
        let zst: Vec<()> = ledger.with_capacity(usize::MAX).unwrap();
        assert!(zst.is_empty());
        assert_eq!(ledger.report().peak_payload_bytes, 0);
    }
}
