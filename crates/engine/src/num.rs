//! Float rounding that works without std, where `f64::floor` and friends don't exist. libm's
//! versions are exact (IEEE 754 rounding of the same value), so std and no_std builds agree.

#[inline]
pub fn floor(x: f64) -> f64 { libm::floor(x) }
#[inline]
pub fn ceil(x: f64) -> f64 { libm::ceil(x) }
/// Half away from zero, like `f64::round`.
#[inline]
pub fn round(x: f64) -> f64 { libm::round(x) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_std_rounding() {
        for x in [-2.5, -1.5, -0.5, -0.49, 0.0, 0.49, 0.5, 1.5, 2.5, 9.999, 13.000001, 1e15 + 0.5] {
            assert_eq!(floor(x), std::primitive::f64::floor(x), "floor {x}");
            assert_eq!(ceil(x), std::primitive::f64::ceil(x), "ceil {x}");
            assert_eq!(round(x), std::primitive::f64::round(x), "round {x}");
        }
    }
}
