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
}
