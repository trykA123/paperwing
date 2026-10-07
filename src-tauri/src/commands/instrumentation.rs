#[cfg(feature = "benchmark")]
use crate::benchmark;
#[cfg(feature = "diagnostics")]
use crate::diagnostics;
#[cfg(feature = "test-profile")]
use crate::test_profile;

domain! {
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_status,
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_preview,
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_cancel,
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_export,
            #[cfg(feature = "benchmark")] benchmark::benchmark_record,
            #[cfg(feature = "benchmark")] benchmark::benchmark_snapshot,
            #[cfg(feature = "test-profile")] test_profile::benchmark_plan,
            #[cfg(feature = "test-profile")] test_profile::benchmark_finish,
}
