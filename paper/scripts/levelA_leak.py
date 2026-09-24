"""Section 3.4: the origin of the latent axis decides how majoritarian the bridge score is.

Mirror-pair design (common.mirror_design): the leak is the gap b(P+) - b(P-) as a fraction of
the gap a camp-size-weighted mean would produce, 2 (pi_B - pi_A) delta. Proposition 2 predicts
1 / (1 + rho) with rho = (lam_b / lam_f) sqrt(S / N), S = sum of squared item leans.
Writes data/levelA_leak.csv.
"""
import numpy as np
from common import bridging_multistart, mirror_design

SHARE_B, DELTA, N_PARTISAN = 0.6, 0.45 * 0.8, 10
rows = []
for lam_b, lam_f in [(0.15, 0.03), (0.15, 0.003), (0.15, 0.0003)]:
    for n in [50, 100, 200, 400, 800, 1600, 3200]:
        leaks = []
        for seed in range(3):
            R, mask, lean = mirror_design(n, SHARE_B, seed)
            (mu, bu, bj, fu, fj), res = bridging_multistart(R, mask, starts=4, lam_b=lam_b, lam_f=lam_f)
            gap = bj[lean > 0].mean() - bj[lean < 0].mean()
            leaks.append(gap / (2 * (2 * SHARE_B - 1) * DELTA))
        rho = (lam_b / lam_f) * np.sqrt(N_PARTISAN * DELTA ** 2 / n)
        rows.append((lam_b / lam_f, n, np.mean(leaks), np.std(leaks), 1 / (1 + rho)))
        print(f"ratio {lam_b / lam_f:>5.0f}  N {n:>5}  leak {np.mean(leaks):.3f} "
              f"(sd {np.std(leaks):.3f})  predicted {1 / (1 + rho):.3f}", flush=True)
np.savetxt("../data/levelA_leak.csv", np.array(rows), delimiter=",", comments="",
           header="ratio,N,leak_mean,leak_sd,predicted", fmt="%.4f")
