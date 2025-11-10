//! SIMD infrastructure with runtime dispatch
//!
//! Detects CPU features and dispatches to optimal implementations.
//! Phase 2: SSE2, AVX2, NEON runtime detection
//! Phase 3: Optimized kernels for IDCT, scaling, deblocking

use std::sync::OnceLock;

/// CPU feature flags detected at runtime
#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    pub sse2: bool,
    pub ssse3: bool,
    pub sse41: bool,
    pub sse42: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub neon: bool, // ARM NEON
}

static CPU_FEATURES: OnceLock<CpuFeatures> = OnceLock::new();

impl CpuFeatures {
    /// Detect CPU features at runtime
    ///
    /// Uses CPUID on x86/x64, reads /proc/cpuinfo on ARM
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self::detect_x86_64()
        }

        #[cfg(target_arch = "aarch64")]
        {
            Self::detect_aarch64()
        }

        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            // Fallback: no SIMD
            Self {
                sse2: false,
                ssse3: false,
                sse41: false,
                sse42: false,
                avx: false,
                avx2: false,
                fma: false,
                neon: false,
            }
        }
    }

    /// Detect x86_64 CPU features
    #[cfg(target_arch = "x86_64")]
    fn detect_x86_64() -> Self {
        Self {
            sse2: is_x86_feature_detected!("sse2"),
            ssse3: is_x86_feature_detected!("ssse3"),
            sse41: is_x86_feature_detected!("sse4.1"),
            sse42: is_x86_feature_detected!("sse4.2"),
            avx: is_x86_feature_detected!("avx"),
            avx2: is_x86_feature_detected!("avx2"),
            fma: is_x86_feature_detected!("fma"),
            neon: false,
        }
    }

    /// Detect ARM AArch64 CPU features
    #[cfg(target_arch = "aarch64")]
    fn detect_aarch64() -> Self {
        Self {
            sse2: false,
            ssse3: false,
            sse41: false,
            sse42: false,
            avx: false,
            avx2: false,
            fma: false,
            neon: true, // NEON is mandatory on AArch64
        }
    }

    /// Get the global CPU features (cached)
    pub fn get() -> &'static Self {
        CPU_FEATURES.get_or_init(Self::detect)
    }

    /// Check if any SIMD is available
    pub fn has_simd(&self) -> bool {
        self.sse2 || self.neon
    }

    /// Get best available x86 SIMD level
    pub fn best_x86_simd(&self) -> SimdLevel {
        if self.avx2 {
            SimdLevel::Avx2
        } else if self.avx {
            SimdLevel::Avx
        } else if self.sse42 {
            SimdLevel::Sse42
        } else if self.sse41 {
            SimdLevel::Sse41
        } else if self.ssse3 {
            SimdLevel::Ssse3
        } else if self.sse2 {
            SimdLevel::Sse2
        } else {
            SimdLevel::Scalar
        }
    }
}

/// SIMD capability level (hierarchical)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimdLevel {
    Scalar = 0,
    Sse2 = 1,
    Ssse3 = 2,
    Sse41 = 3,
    Sse42 = 4,
    Avx = 5,
    Avx2 = 6,
    Neon = 10,
}

impl SimdLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SimdLevel::Scalar => "scalar",
            SimdLevel::Sse2 => "sse2",
            SimdLevel::Ssse3 => "ssse3",
            SimdLevel::Sse41 => "sse4.1",
            SimdLevel::Sse42 => "sse4.2",
            SimdLevel::Avx => "avx",
            SimdLevel::Avx2 => "avx2",
            SimdLevel::Neon => "neon",
        }
    }
}

/// Function dispatch macro for SIMD runtime selection
///
/// # Example
/// ```ignore
/// simd_dispatch!(
///     idct_4x4,
///     avx2 => idct_4x4_avx2,
///     sse2 => idct_4x4_sse2,
///     scalar => idct_4x4_scalar
/// );
/// ```
#[macro_export]
macro_rules! simd_dispatch {
    ($name:ident, $($level:ident => $func:expr),+ $(,)?) => {
        pub fn $name() -> fn() {
            let features = CpuFeatures::get();
            $(
                if stringify!($level) == "avx2" && features.avx2 {
                    return $func;
                } else if stringify!($level) == "avx" && features.avx {
                    return $func;
                } else if stringify!($level) == "sse42" && features.sse42 {
                    return $func;
                } else if stringify!($level) == "sse41" && features.sse41 {
                    return $func;
                } else if stringify!($level) == "ssse3" && features.ssse3 {
                    return $func;
                } else if stringify!($level) == "sse2" && features.sse2 {
                    return $func;
                } else if stringify!($level) == "neon" && features.neon {
                    return $func;
                }
            )+
            // Fallback to scalar
            $(
                if stringify!($level) == "scalar" {
                    return $func;
                }
            )+
            unreachable!("No scalar fallback provided")
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_feature_detection() {
        let features = CpuFeatures::detect();

        #[cfg(target_arch = "x86_64")]
        {
            // Most modern x86_64 CPUs have SSE2
            // Just verify detection runs without panicking
            let _ = features.sse2;
        }

        #[cfg(target_arch = "aarch64")]
        {
            // NEON is mandatory on AArch64
            assert!(features.neon);
        }
    }

    #[test]
    fn test_cached_features() {
        let f1 = CpuFeatures::get();
        let f2 = CpuFeatures::get();

        // Should return same instance (cached)
        assert_eq!(f1.sse2, f2.sse2);
        assert_eq!(f1.avx2, f2.avx2);
    }

    #[test]
    fn test_simd_level_ordering() {
        assert!(SimdLevel::Avx2 > SimdLevel::Avx);
        assert!(SimdLevel::Avx > SimdLevel::Sse42);
        assert!(SimdLevel::Sse2 > SimdLevel::Scalar);
    }

    #[test]
    fn test_best_simd_selection() {
        let features = CpuFeatures::get();
        let best = features.best_x86_simd();

        // Should return some level
        assert!(best >= SimdLevel::Scalar);
    }
}
