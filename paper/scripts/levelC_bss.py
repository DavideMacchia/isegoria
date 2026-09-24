"""Section 5.3: the ratio-form Brier Skill Score is not a proper scoring rule.

For a reviewer with independent beliefs q_j and a crowd baseline b_j, finds the report that
maximizes the exact expected BSS (enumerating all 2^m outcomes), with the baseline either
excluding the reviewer (others only, weight 8) or including the reviewer (weight 1 of 9, as
crates/scoring reputation::crowd_baseline does). Checks the one-item closed form
logit p* = logit q + 2 logit b. Writes data/levelC_bss_curve.csv, data/levelC_bss_distortion.csv.
"""
import itertools
import numpy as np
from scipy.optimize import minimize
from scipy.special import logit, expit


def expected_bss(p, q, others, include_self, w_self=1.0, w_others=8.0):
    outcomes = np.array(list(itertools.product([0.0, 1.0], repeat=len(q))))
    prob = np.prod(np.where(outcomes == 1, q, 1 - q), axis=1)
    base = (w_self * p + w_others * others) / (w_self + w_others) if include_self else others
    num = ((p - outcomes) ** 2).sum(1)
    den = ((base - outcomes) ** 2).sum(1)
    return (prob * (1 - num / den)).sum()


def best_report(q, others, include_self):
    obj = lambda z: -expected_bss(expit(z), q, others, include_self)
    return expit(minimize(obj, logit(q), method="L-BFGS-B").x)


for q, b in [(0.30, 0.65), (0.50, 0.65), (0.78, 0.65), (0.30, 0.20), (0.90, 0.50)]:
    num = best_report(np.array([q]), np.array([b]), False)[0]
    print(f"q={q:.2f} b={b:.2f}: numeric {num:.4f}   closed form {expit(logit(q) + 2 * logit(b)):.4f}")

qs = np.linspace(0.02, 0.98, 49)
curve = [(q, expit(logit(q) + 2 * logit(0.65)),
          best_report(np.array([q]), np.array([0.65]), True)[0]) for q in qs]
np.savetxt("../data/levelC_bss_curve.csv", np.array(curve), delimiter=",", comments="",
           header="q,pstar_fixed,pstar_selfincl", fmt="%.5f")

rng = np.random.default_rng(1)
rows = []
for m in (1, 2, 4, 8):
    for include_self in (False, True):
        dev = []
        for _ in range(200 if m < 8 else 60):
            q, b = rng.uniform(0.1, 0.9, m), rng.uniform(0.2, 0.8, m)
            dev.append(np.abs(best_report(q, b, include_self) - q))
        dev = np.concatenate(dev)
        rows.append((m, int(include_self), dev.mean(), dev.max()))
        print(f"m={m} include_self={include_self}: mean |p*-q| {dev.mean():.3f}  max {dev.max():.3f}", flush=True)
np.savetxt("../data/levelC_bss_distortion.csv", np.array(rows), delimiter=",", comments="",
           header="m,include_self,mean_abs_dev,max_abs_dev", fmt="%.4f")
