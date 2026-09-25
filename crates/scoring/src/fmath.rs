//! Transcendental functions via `libm`, not the platform's libm, for bit-identical results
//! across machines (CLAUDE.md invariant #7, `docs/08` INV-7, AT-BR-04).

#[inline]
pub(crate) fn exp(x: f64) -> f64 {
    libm::exp(x)
}

#[inline]
pub(crate) fn ln(x: f64) -> f64 {
    libm::log(x)
}

#[inline]
pub(crate) fn ln_1p(x: f64) -> f64 {
    libm::log1p(x)
}

#[inline]
pub(crate) fn cos(x: f64) -> f64 {
    libm::cos(x)
}

#[inline]
pub(crate) fn powf(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches the platform's libm to a few ulps (AT-BR-04).
    #[test]
    fn agrees_with_the_platform_libm_to_a_few_ulps() {
        for x in [-30.0, -2.5, -0.3, 0.0, 1e-9, 0.7, 2.0, 25.0] {
            assert!((exp(x) - x.exp()).abs() <= 4.0 * f64::EPSILON * x.exp().abs());
            if x > 0.0 {
                assert!((ln(x) - x.ln()).abs() <= 4.0 * f64::EPSILON * x.ln().abs().max(1.0));
            }
            if x > -1.0 {
                assert!(
                    (ln_1p(x) - x.ln_1p()).abs() <= 4.0 * f64::EPSILON * x.ln_1p().abs().max(1.0)
                );
            }
            assert!((cos(x) - x.cos()).abs() <= 4.0 * f64::EPSILON);
        }
        assert!((powf(7.0, 0.5) - 7.0_f64.sqrt()).abs() <= 4.0 * f64::EPSILON * 7.0_f64.sqrt());
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln_1p(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
    }
}
