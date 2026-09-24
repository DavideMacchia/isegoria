"""Section 4.5: error in the ability proxy creates a latent class out of nothing.

Two-class uniform-DIF mixture vs one class (the repository simulation's model), with theta
either the standardized total over 30 anchors (proxy) or the true theta (exact), for 0, 1, 2
biased items. Reports the likelihood ratio, the simulation's BIC gain LR - K ln N, and the
mean estimated gap 2|d_j| on biased and clean items. Writes data/levelB_proxy.csv.
"""
import numpy as np
from common import dif_generate, dif_fit

K, DELTA = 8, 0.9
rows = []
for proxy in (True, False):
    for n_biased in (0, 1, 2):
        for n in (1500, 3000, 6000, 12000):
            lr, gap_b, gap_c = [], [], []
            for s in range(3):
                th, X = dif_generate(n, n_biased, DELTA, 500 + s, proxy=proxy)
                null, mix = dif_fit(th, X, seed=s)
                lr.append(2 * (null.fun - mix.fun))
                gaps = 2 * np.abs(mix.x[1 + 2 * K:])
                if n_biased:
                    gap_b.append(gaps[:n_biased].mean())
                gap_c.append(gaps[n_biased:].mean())
            row = (int(proxy), n_biased, n, np.mean(lr), np.mean(lr) - K * np.log(n),
                   np.mean(gap_b) if gap_b else np.nan, np.mean(gap_c))
            rows.append(row)
            print(f"proxy={proxy!s:5} biased={n_biased} N={n:>5}  LR {row[3]:7.1f}  "
                  f"BIC gain {row[4]:7.1f}  gap biased {row[5]:.2f}  gap clean {row[6]:.2f}", flush=True)
np.savetxt("../data/levelB_proxy.csv", np.array(rows), delimiter=",", comments="",
           header="proxy,n_biased,N,LR,BIC_gain,gap_biased,gap_clean", fmt="%.4f")
