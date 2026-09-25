"""Differential oracle for the bridging fit (`docs/08` REPRO-003): the paper's SciPy fit
(`paper/scripts/common.py`) on `<dir>/R.csv`, `mask.csv` and `weights.csv`; writes
`<dir>/oracle.csv` as `name,value` lines."""
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
