//! Isolated allocator proof that the construction ledger charges every allocation.
//!
//! A counting allocator records live bytes and their peak on this thread.
//! Every build must stay within its reported peak, retain exactly its reported
//! payload, and a refused build must stay within its limit and release everything.
mod construction_support;
use approx_chol::low_level::Builder;
use approx_chol::{ConstructionLimit, CsrRef, Error};
use construction_support::*;

/// Peak and retained live bytes of `run` on this thread. The counting allocator's
/// default reallocation allocates the new buffer before freeing the old one, which
/// is the overlap the ledger charges.
fn measure<R>(run: impl FnOnce() -> R) -> (R, usize, i64) {
    let mut result = None;
    let info = allocation_counter::measure(|| result = Some(run()));
    (result.unwrap(), info.bytes_max as usize, info.bytes_current)
}

fn main() {
    let mut cases = 0;
    let mut slack_max = 0usize;
    for g in generated() {
        for (name, config) in configs() {
            let builder = Builder::<f64>::new(config);
            let csr = CsrRef::new(&g.rows, &g.cols, &g.values, g.n as u32).unwrap();
            let (built, peak, retained) =
                measure(|| builder.build_with_limit(csr, ConstructionLimit::UNLIMITED));
            let (factor, report) = built.unwrap();
            assert!(
                peak <= report.peak_payload_bytes,
                "{} {name}: allocator peak {peak} exceeds ledger peak {}",
                g.name,
                report.peak_payload_bytes
            );
            assert_eq!(
                retained, report.retained_payload_bytes as i64,
                "{} {name}",
                g.name
            );
            assert_eq!(
                factor.retained_heap_bytes(),
                Some(report.retained_payload_bytes)
            );
            slack_max = slack_max.max(report.peak_payload_bytes - peak);
            let (_, _, released) = measure(|| drop(factor));
            assert_eq!(
                released, -retained,
                "{} {name}: drop releases the payload",
                g.name
            );

            for limit in [report.peak_payload_bytes - 1, report.peak_payload_bytes / 3] {
                let under = ConstructionLimit {
                    maximum_payload_bytes: limit,
                };
                let (refused, peak, retained) = measure(|| builder.build_with_limit(csr, under));
                assert!(matches!(
                    refused,
                    Err(Error::ConstructionBudgetExceeded { .. })
                ));
                assert!(
                    peak <= limit,
                    "{} {name}: refused build peaked at {peak} > {limit}",
                    g.name
                );
                // The error itself owns no heap payload.
                assert_eq!(retained, 0, "{} {name}: refused build leaked", g.name);
            }
            println!(
                "CONSTRUCTION case={} config={name} peak={} allocator_peak={peak} retained={retained}",
                g.name, report.peak_payload_bytes
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 31 * 5);

    // A failed exact attempt releases everything and reports the peak it reached.
    let mut failures = 0;
    for g in generated() {
        for (name, config) in exact_error_configs() {
            let builder = Builder::<f64>::new(config);
            let csr = CsrRef::new(&g.rows, &g.cols, &g.values, g.n as u32).unwrap();
            let ((built, report), peak, retained) =
                measure(|| builder.build_with_limit_reported(csr, ConstructionLimit::UNLIMITED));
            match built {
                Ok(factor) => {
                    assert_eq!(retained, report.retained_payload_bytes as i64);
                    drop(factor);
                }
                Err(error) => {
                    assert!(matches!(error, Error::DenseFactorizationFailed { .. }));
                    assert_eq!(retained, 0, "{} {name}: a failed build leaked", g.name);
                    assert_eq!(report.retained_payload_bytes, 0);
                    failures += 1;
                }
            }
            assert!(
                peak <= report.peak_payload_bytes,
                "{} {name}: allocator peak {peak} exceeds reported peak {}",
                g.name,
                report.peak_payload_bytes
            );
            slack_max = slack_max.max(report.peak_payload_bytes - peak);
        }
    }
    assert!(failures > 0, "a failed exact attempt is measured");
    println!(
        "CONSTRUCTION_SUMMARY cases={cases} failed_exact_attempts={failures} maximum_conservative_slack_bytes={slack_max}"
    );
}
