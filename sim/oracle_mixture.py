"""Differential oracle for the latent-class mixture (`docs/08` REPRO-003): the paper's
SciPy two-class fit and one-class null (`paper/scripts/common.py::dif_fit`) on
`<dir>/theta.csv` and `X.csv`; writes `<dir>/oracle.csv` as `name,value` lines."""
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
