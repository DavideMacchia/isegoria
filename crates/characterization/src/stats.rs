//! The statistics of the summaries (`docs/13` §5): Wilson intervals over independent runs,
//! design-effect Wilson intervals over items grouped in batches, type-7 quantiles.

pub const Z95: f64 = 1.959_963_984_540_054;

fn wilson_real(x: f64, n: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 1.0);
    }
    let p = x / n;
    let z2 = Z95 * Z95;
    let denominator = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denominator;
    let half = Z95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denominator;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// The 95% Wilson score interval of `x` successes in `n` independent trials; `(0, 1)` at `n = 0`.
pub fn wilson(x: usize, n: usize) -> (f64, f64) {
    wilson_real(x as f64, n as f64)
}

/// NaN for no values.
pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// The sample standard deviation; NaN for fewer than two values.
pub fn sd(v: &[f64]) -> f64 {
    let m = mean(v);
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt()
}

/// A rate over items in batches, `(hits, items)` per batch: the pooled rate, and the Wilson
/// interval on the effective sample size the between-batch variance gives, kept between one
/// batch counted once (the fewest) and every item counted once. NaN with no item.
pub fn clustered_rate(batches: &[(usize, usize)]) -> (f64, f64, f64) {
    let hits: usize = batches.iter().map(|b| b.0).sum();
    let items: usize = batches.iter().map(|b| b.1).sum();
    if items == 0 {
        return (f64::NAN, 0.0, 1.0);
    }
    let (total, p) = (items as f64, hits as f64 / items as f64);
    let squares: f64 = batches.iter().map(|b| (b.1 * b.1) as f64).sum();
    let fewest = total * total / squares;
    let b = batches.len() as f64;
    let spread: f64 = batches
        .iter()
        .map(|&(h, m)| (h as f64 - p * m as f64).powi(2))
        .sum::<f64>()
        * b
        / ((b - 1.0) * total * total);
    let binomial = p * (1.0 - p);
    let effective = if binomial == 0.0 || batches.len() < 2 {
        fewest
    } else if spread > 0.0 {
        (binomial / spread).clamp(fewest, total)
    } else {
        total
    };
    let (lo, hi) = wilson_real(p * effective, effective);
    (p, lo, hi)
}

/// The type-7 (linear) quantile of sorted values, an infinite value kept infinite; NaN for none.
pub fn quantile(sorted: &[f64], q: f64) -> f64 {
    match sorted.len() {
        0 => f64::NAN,
        1 => sorted[0],
        n => {
            let h = (n - 1) as f64 * q;
            let lo = h.floor() as usize;
            let (a, b) = (sorted[lo], sorted[(lo + 1).min(n - 1)]);
            let t = h - lo as f64;
            if t == 0.0 || a == b {
                a
            } else if b.is_infinite() {
                b
            } else {
                a + t * (b - a)
            }
        }
    }
}

/// `values` sorted by `total_cmp`, NaN last.
pub fn sorted(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values
}
