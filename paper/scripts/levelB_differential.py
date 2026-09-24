"""Section 4.5, remedy (b): measure DIF relative to the batch's common class shift.

The raw gap 2|d_j| of a two-class uniform-DIF fit on a proxy theta contains a shift common to
all items (the artefact of Proposition 6). The differential gap 2|d_j - median_k d_k| removes
it. Theta from 30 anchors; 0, 2 or 3 biased items (delta = 0.9); N in {3000, 6000}; 3 datasets.
Writes data/levelB_differential.csv.
"""
import numpy as np
from common import dif_generate, dif_fit

K = 8
rows = []
for n_biased in (0, 2, 3):
    for n in (3000, 6000):
        raw_b, raw_c, dif_b, dif_c = [], [], [], []
        for s in range(3):
            th, X = dif_generate(n, n_biased, 0.9, 500 + s, proxy=True)
            _, mix = dif_fit(th, X, seed=s)
            d = mix.x[1 + 2 * K:]
            raw, diff = 2 * np.abs(d), 2 * np.abs(d - np.median(d))
            if n_biased:
                raw_b.append(raw[:n_biased].min())
                dif_b.append(diff[:n_biased].min())
            raw_c.append(raw[n_biased:].max())
            dif_c.append(diff[n_biased:].max())
        mean = lambda v: np.mean(v) if v else np.nan
        rows.append((n_biased, n, mean(raw_b), mean(raw_c), mean(dif_b), mean(dif_c)))
        print(f"biased={n_biased} N={n}: raw min biased {rows[-1][2]:.2f} max clean {rows[-1][3]:.2f} | "
              f"differential min biased {rows[-1][4]:.2f} max clean {rows[-1][5]:.2f}", flush=True)
np.savetxt("../data/levelB_differential.csv", np.array(rows), delimiter=",", comments="",
           header="n_biased,N,raw_min_biased,raw_max_clean,diff_min_biased,diff_max_clean", fmt="%.4f")
