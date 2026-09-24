//! Reads a theta column and a respondents x items 0/1 matrix (CSV), runs
//! `scoring::dif::mixture_dif` and applies the protocol's verdict rule
//! (`protocol::revalidation::latent_flags`: converged, >= 2 classes, DIF_j > MIXTURE_DIF_MAX).
use scoring::dif::{mixture_dif, MIXTURE_DIF_MAX};
use scoring::Convergence;
use std::fs;

fn read_csv(path: &str) -> Vec<Vec<f64>> {
    fs::read_to_string(path)
        .expect("readable CSV")
        .lines()
        .map(|l| {
            l.split(',')
                .map(|v| v.trim().parse::<f64>().expect("number"))
                .collect()
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let theta: Vec<f64> = read_csv(&args[1]).into_iter().map(|r| r[0]).collect();
    let x = read_csv(&args[2]);
    let k = x[0].len();
    let res = mixture_dif(&theta, &x, k, 0);
    let trustworthy = res.status == Convergence::Converged && res.classes >= 2;
    let flags: Vec<u8> = res
        .dif
        .iter()
        .map(|&d| u8::from(trustworthy && d > MIXTURE_DIF_MAX))
        .collect();
    let join = |v: Vec<String>| v.join(";");
    println!(
        "{},{},{:.3},{},{},{}",
        res.classes,
        u8::from(res.non_uniform),
        res.bic_gain,
        u8::from(res.status == Convergence::Converged),
        join(res.dif.iter().map(|d| format!("{d:.4}")).collect()),
        join(flags.iter().map(|f| f.to_string()).collect())
    );
}
