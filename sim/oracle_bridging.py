"""Differential oracle for the bridging fit (docs/08 REPRO-003, T45).

The paper's NumPy/SciPy implementation of the weighted bridging objective
(paper/scripts/common.py: analytic gradient, L-BFGS-B at gtol 1e-10, ftol 1e-15) on a dataset
the Rust tests wrote, from eight seeded starts, the lowest objective kept — the same rule the
engine follows (T48). Reads <dir>/R.csv (respondents x items), <dir>/mask.csv (0/1) and
<dir>/weights.csv (one weight per reviewer); writes <dir>/oracle.csv as `name,value` lines:
objective, mu, then b_j[k], f_j[k], b_u[k], f_u[k].

Usage: python sim/oracle_bridging.py <dir>
"""
import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "paper", "scripts"))
from common import bridging_multistart  # noqa: E402

d = sys.argv[1]
R = np.loadtxt(os.path.join(d, "R.csv"), delimiter=",", ndmin=2)
mask = np.loadtxt(os.path.join(d, "mask.csv"), delimiter=",", ndmin=2) > 0.5
weights = np.loadtxt(os.path.join(d, "weights.csv"), delimiter=",", ndmin=1)
(mu, bu, bj, fu, fj), res = bridging_multistart(R, mask, starts=8, weights=weights)
with open(os.path.join(d, "oracle.csv"), "w") as f:
    f.write(f"objective,{float(res.fun)!r}\n")
    f.write(f"mu,{float(mu)!r}\n")
    for name, v in (("b_j", bj), ("f_j", fj), ("b_u", bu), ("f_u", fu)):
        for k, x in enumerate(v):
            f.write(f"{name}[{k}],{float(x)!r}\n")
print(f"objective {res.fun:.10f} after {res.nit} iterations ({res.message})")
