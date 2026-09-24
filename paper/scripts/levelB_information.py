"""Section 4.4: information a two-class uniform DIF leaves in the data, given exact theta.

Single biased item: minimal expected KL between the mixture response curve and any 2PL.
Pair of biased items: conditional mutual information I(X1; X2 | theta), which no
conditionally independent model can reproduce. Both reported as 2 N x (per-respondent
information), i.e. the expected likelihood-ratio contribution, at N = 3000.
Writes data/levelB_information.csv.
"""
import numpy as np
from scipy.optimize import minimize
from scipy.special import expit

A, B, PI, N = 1.25, 0.0, 0.5, 3000
t, w = np.polynomial.hermite_e.hermegauss(80)
w = w / w.sum()


def kl_bern(p, q):
    return p * np.log(p / q) + (1 - p) * np.log((1 - p) / (1 - q))


def single(delta):
    m = (1 - PI) * expit(A * (t - B + delta)) + PI * expit(A * (t - B - delta))
    obj = lambda x: (w * kl_bern(m, expit(x[0] * (t - x[1])))).sum()
    return minimize(obj, [A, B], method="Nelder-Mead",
                    options={"xatol": 1e-11, "fatol": 1e-15, "maxiter": 40000}).fun


def pair(delta):
    p = [expit(A * (t - B + delta)), expit(A * (t - B - delta))]
    m = (1 - PI) * p[0] + PI * p[1]
    info = np.zeros_like(t)
    for x1 in (0, 1):
        for x2 in (0, 1):
            joint = sum(pk_w * (pk if x1 else 1 - pk) * (pk if x2 else 1 - pk)
                        for pk, pk_w in zip(p, (1 - PI, PI)))
            marg = (m if x1 else 1 - m) * (m if x2 else 1 - m)
            info += joint * np.log(joint / marg)
    return (w * info).sum()


rows = []
for delta in [0.15, 0.3, 0.45, 0.6, 0.9]:
    s, p = single(delta), pair(delta)
    rows.append((delta, 2 * N * s, 2 * N * p, p / s, s / delta ** 4, p / delta ** 4))
    print(f"delta {delta:.2f}: single {2 * N * s:8.3f}  pair {2 * N * p:8.2f}  ratio {p / s:6.1f}")
np.savetxt("../data/levelB_information.csv", np.array(rows), delimiter=",", comments="",
           header="delta,lr_single,lr_pair,ratio,single_over_delta4,pair_over_delta4", fmt="%.6g")
