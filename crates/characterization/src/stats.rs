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
/// interval on items over the design effect the batches show, taken as the batch size when
/// the batches cannot estimate it. NaN with no item.
pub fn clustered_rate(batches: &[(usize, usize)]) -> (f64, f64, f64) {
    let hits: usize = batches.iter().map(|b| b.0).sum();
    let items: usize = batches.iter().map(|b| b.1).sum();
    if items == 0 {
        return (f64::NAN, 0.0, 1.0);
    }
    let p = hits as f64 / items as f64;
    let size = items as f64 / batches.len() as f64;
    let rates: Vec<f64> = batches
        .iter()
        .filter(|b| b.1 > 0)
        .map(|&(h, n)| h as f64 / n as f64)
        .collect();
    let binomial = p * (1.0 - p) / size;
    let effect = if binomial > 0.0 && rates.len() > 1 {
        (sd(&rates).powi(2) / binomial).clamp(1.0, size)
    } else {
        size
    };
    let effective = items as f64 / effect;
    let (lo, hi) = wilson_real(p * effective, effective);
    (p, lo, hi)
}

/// The type-7 (linear) quantile of sorted values; NaN for none.
pub fn quantile(sorted: &[f64], q: f64) -> f64 {
    match sorted.len() {
        0 => f64::NAN,
        1 => sorted[0],
        n => {
            let h = (n - 1) as f64 * q;
            let lo = h.floor() as usize;
            let hi = (lo + 1).min(n - 1);
            sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
        }
    }
}

/// `values` sorted by `total_cmp`, NaN last.
pub fn sorted(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values
}
