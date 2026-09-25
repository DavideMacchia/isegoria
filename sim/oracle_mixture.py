"""Differential oracle for the latent-class mixture on a fixed ability (docs/08 REPRO-003, T45).

The paper's NumPy/SciPy two-class uniform-DIF mixture (paper/scripts/common.py::dif_fit:
class difficulties b ± d, a shared discrimination, L-BFGS-B at gtol 1e-8, four seeded starts)
and its one-class null on a batch the Rust tests wrote. The engine's two-class uniform
candidate is the same model in a free parametrization (b_j0, b_j1 instead of b, d; the mixing
logit), so the optimal negative log-likelihoods, the BICs and the gaps 2|d| must agree. Reads
<dir>/theta.csv (one value per respondent) and <dir>/X.csv (respondents x items, 0/1); writes
<dir>/oracle.csv as `name,value` lines: n, k, null_nll, mixture_nll, pi, then d[k].

Usage: python sim/oracle_mixture.py <dir>
"""
import os
import sys

import numpy as np
from scipy.special import expit

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "paper", "scripts"))
from common import dif_fit  # noqa: E402

d = sys.argv[1]
theta = np.loadtxt(os.path.join(d, "theta.csv"), delimiter=",", ndmin=1)
X = np.loadtxt(os.path.join(d, "X.csv"), delimiter=",", ndmin=2)
null, mix = dif_fit(theta, X, seed=0, starts=4)
k = X.shape[1]
with open(os.path.join(d, "oracle.csv"), "w") as f:
    f.write(f"n,{X.shape[0]}\nk,{k}\n")
    f.write(f"null_nll,{float(null.fun)!r}\nmixture_nll,{float(mix.fun)!r}\n")
    f.write(f"pi,{float(expit(mix.x[0]))!r}\n")
    for j, x in enumerate(mix.x[1 + 2 * k:]):
        f.write(f"d[{j}],{float(x)!r}\n")
print(f"null nll {null.fun:.6f}, mixture nll {mix.fun:.6f}")
