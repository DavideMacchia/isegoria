"""Section 7.3 (decision D37): anchor reliability and the differential gap.

(a) KR-20 of the anchor total, drawn exactly as in the null batches of Table 6
    (common.dif_generate, seeds 1300-1303, N = 6000). (b) Raw vs differential gap when 2, 4 or
    6 of 8 items are shifted the same way (30 anchors, N = 6000).
Writes data/rev_dif_kr20.csv and data/rev_dif_campaign.csv.
"""
import numpy as np
from scipy.special import expit
from common import dif_generate, dif_fit

with open("../data/rev_dif_kr20.csv", "w") as f:
    f.write("anchors,kr20,true_reliability\n")
    for na in (10, 20, 25, 30, 35, 40, 45, 50, 60):
        kr, tr = [], []
        for s in range(4):
            r = np.random.default_rng(1300 + s)
            theta = r.normal(0, 1, 6000)
            r.choice([-1, 1], 6000)
            a, b = r.uniform(.9, 1.6, na), r.normal(0, 1, na)
            XA = (r.random((6000, na)) < expit(a * (theta[:, None] - b))).astype(float)
            T, p = XA.sum(1), XA.mean(0)
            kr.append(na / (na - 1) * (1 - (p * (1 - p)).sum() / T.var()))
            tr.append(np.corrcoef(T, theta)[0, 1] ** 2)
        f.write(f"{na},{np.mean(kr):.3f},{np.mean(tr):.3f}\n")
        print(f"{na:>2} anchors: KR-20 {np.mean(kr):.3f} (true reliability {np.mean(tr):.3f})")

K = 8
with open("../data/rev_dif_campaign.csv", "w") as f:
    f.write("n_biased,raw_min_biased,raw_max_clean,diff_min_biased,diff_max_clean\n")
    for n_b in (2, 4, 6):
        rb, rc, db, dc = [], [], [], []
        for s in range(3):
            th, X = dif_generate(6000, n_b, .9, 700 + s, proxy=True)
            _, mix = dif_fit(th, X, seed=s)
            d = mix.x[1 + 2 * K:]
            raw, diff = 2 * np.abs(d), 2 * np.abs(d - np.median(d))
            rb.append(raw[:n_b].min()); db.append(diff[:n_b].min())
            rc.append(raw[n_b:].max()); dc.append(diff[n_b:].max())
        row = (np.mean(rb), np.mean(rc), np.mean(db), np.mean(dc))
        f.write(f"{n_b}," + ",".join(f"{v:.3f}" for v in row) + "\n")
        print(f"{n_b} of 8 biased: raw {row[0]:.2f} / {row[1]:.2f} | differential {row[2]:.2f} / {row[3]:.2f}")
