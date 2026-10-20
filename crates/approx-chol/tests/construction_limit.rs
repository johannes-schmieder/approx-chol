//! A construction limit never changes the factor: it either matches the unlimited
//! build exactly or refuses with a typed error.
#[cfg(feature = "serde")]
mod construction_support;

#[cfg(feature = "serde")]
mod with_serde {
    use super::construction_support::*;
    use approx_chol::low_level::Builder;
    use approx_chol::{ConstructionLimit, CsrRef, Error};
    use num_traits::Float;

    fn check<T>(g: &Generated, label: &str, config: approx_chol::Config) -> usize
    where
        T: Float + Send + Sync + serde::Serialize + std::fmt::Debug + 'static,
    {
        let values: Vec<T> = g.values.iter().map(|&x| T::from(x).unwrap()).collect();
        let csr = || CsrRef::new(&g.rows, &g.cols, &values, g.n as u32).unwrap();
        let builder = Builder::<T>::new(config);
        let plain = builder.build(csr()).unwrap();
        let wire = serde_json::to_value(&plain).unwrap();
        let (unlimited, report) = builder
            .build_with_limit(csr(), ConstructionLimit::UNLIMITED)
            .unwrap();
        assert_eq!(serde_json::to_value(&unlimited).unwrap(), wire, "{label}");
        assert_eq!(
            Some(report.retained_payload_bytes),
            unlimited.retained_heap_bytes(),
            "{label}"
        );
        assert!(report.peak_payload_bytes >= report.retained_payload_bytes);
        let (reported, same) =
            builder.build_with_limit_reported(csr(), ConstructionLimit::UNLIMITED);
        assert_eq!(same, report, "{label}: the reporting form accounts alike");
        assert_eq!(
            serde_json::to_value(reported.unwrap()).unwrap(),
            wire,
            "{label}"
        );
        let peak = report.peak_payload_bytes;
        let at = ConstructionLimit {
            maximum_payload_bytes: peak,
        };
        let (exact, again) = builder.build_with_limit(csr(), at).unwrap();
        assert_eq!(again, report, "{label}: accounting is deterministic");
        assert_eq!(serde_json::to_value(&exact).unwrap(), wire, "{label}");
        for limit in [peak - 1, peak / 2, report.retained_payload_bytes / 2, 0] {
            let under = ConstructionLimit {
                maximum_payload_bytes: limit,
            };
            match builder.build_with_limit(csr(), under) {
                Err(Error::ConstructionBudgetExceeded { required, limit: l }) => {
                    assert_eq!(l, limit, "{label}");
                    assert!(required > limit && required <= peak, "{label}");
                }
                other => panic!("{label} at {limit}: {other:?}"),
            }
            let (refused, failed) = builder.build_with_limit_reported(csr(), under);
            assert!(matches!(
                refused,
                Err(Error::ConstructionBudgetExceeded { .. })
            ));
            assert!(failed.peak_payload_bytes <= limit, "{label}");
            assert_eq!(failed.retained_payload_bytes, 0, "{label}");
        }
        unlimited.fallbacks().len()
    }

    #[test]
    fn every_generated_build_is_limit_invariant() {
        let mut fallbacks = 0;
        let mut checked = 0;
        for g in generated() {
            for (name, config) in configs() {
                let label = format!("{} {name}", g.name);
                fallbacks += check::<f64>(&g, &label, config);
                check::<f32>(&g, &label, config);
                checked += 1;
            }
        }
        assert_eq!(checked, 31 * 5);
        assert!(
            fallbacks > 0,
            "the pivot-cancellation fallback is exercised"
        );
    }

    /// A failed exact attempt reports the peak it reached, and that peak is the
    /// inclusive limit at which the same failure, not a budget refusal, recurs.
    #[test]
    fn failed_exact_attempts_report_their_peak() {
        let mut failures = 0;
        for g in generated() {
            for (name, config) in exact_error_configs() {
                let label = format!("{} {name}", g.name);
                let csr = || CsrRef::new(&g.rows, &g.cols, &g.values, g.n as u32).unwrap();
                let builder = Builder::<f64>::new(config);
                let (built, report) =
                    builder.build_with_limit_reported(csr(), ConstructionLimit::UNLIMITED);
                let Err(error) = built else {
                    continue;
                };
                assert!(
                    matches!(error, Error::DenseFactorizationFailed { .. }),
                    "{label}: {error:?}"
                );
                assert_eq!(report.retained_payload_bytes, 0, "{label}");
                assert!(report.peak_payload_bytes > 0, "{label}");
                let at = ConstructionLimit {
                    maximum_payload_bytes: report.peak_payload_bytes,
                };
                let (again, same) = builder.build_with_limit_reported(csr(), at);
                assert!(matches!(again, Err(Error::DenseFactorizationFailed { .. })));
                assert_eq!(same, report, "{label}");
                let under = ConstructionLimit {
                    maximum_payload_bytes: report.peak_payload_bytes - 1,
                };
                let (refused, _) = builder.build_with_limit_reported(csr(), under);
                assert!(
                    matches!(refused, Err(Error::ConstructionBudgetExceeded { .. })),
                    "{label}"
                );
                failures += 1;
            }
        }
        assert!(failures > 0, "the pivot-cancellation failure is exercised");
    }
}
