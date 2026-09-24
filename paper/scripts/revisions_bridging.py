"""Section 7.1 (decision D32): the side-balanced bridge score.

After the unchanged fit, reviewers are split into two sides by a deterministic 1-D 2-means on
f_u; the model's predicted ratings are averaged within each side and the two side averages are
averaged with equal weight. Compared with the item intercept on (a) the mirror-pair leak of
Table 3 at 60/40 and 80/20 camps, (b) the consensus items of the repository simulation scored
in their batch, alone and next to ten weak decoys, (c) the ten simulation items side by side.
Writes data/rev_bridging_leak.csv, data/rev_bridging_batch.csv, data/rev_bridging_sides.csv.
"""
import numpy as np
from common import bridging_multistart, mirror_design, sim_levelA_dataset


def two_means(f, iters=100):
    """Deterministic 1-D 2-means, initialized at the extremes of f."""
    c = np.array([f.min(), f.max()])
    for _ in range(iters):
        lab = np.abs(f[:, None] - c[None, :]).argmin(1)
        new = np.array([f[lab == k].mean() if (lab == k).any() else c[k] for k in (0, 1)])
        if np.allclose(new, c):
            break
        c = new
    return lab


def side_scores(params):
    """Per-side mean predicted rating (side 0, side 1) and the side-balanced score."""
    mu, bu, bj, fu, fj = params
    rhat = mu + bu[:, None] + bj[None, :] + np.outer(fu, fj)
    lab = two_means(fu)
    a, b = rhat[lab == 0].mean(0), rhat[lab == 1].mean(0)
    return a, b, (a + b) / 2, lab


if __name__ == "__main__":
    # (a) leak, intercept vs side-balanced score
    rows = []
    for share, sizes in ((0.6, (50, 200, 800, 3200)), (0.8, (200, 800))):
        full = 2 * (2 * share - 1) * 0.36
        for n in sizes:
            li, ls = [], []
            for seed in range(3):
                R, mask, lean = mirror_design(n, share, seed)
                params, _ = bridging_multistart(R, mask, starts=4)
                _, _, m, _ = side_scores(params)
                li.append((params[2][lean > 0].mean() - params[2][lean < 0].mean()) / full)
                ls.append((m[lean > 0].mean() - m[lean < 0].mean()) / full)
            rows.append((share, n, np.mean(li), np.mean(ls), np.std(ls)))
            print(f"camps {share:.0%}/{1 - share:.0%} n={n:>5}: leak intercept {np.mean(li):.3f} | "
                  f"side-balanced {np.mean(ls):+.3f} (sd {np.std(ls):.3f})", flush=True)
    np.savetxt("../data/rev_bridging_leak.csv", np.array(rows), delimiter=",", comments="",
               header="share_majority,n,leak_intercept,leak_side,leak_side_sd", fmt="%.4f")

    # (b) batch relativity: in batch, alone, with ten weak decoys
    R, mask, true_f = sim_levelA_dataset()
    cons = [0, 1, 3, 4, 5, 6]
    rng = np.random.default_rng(99)
    n = R.shape[0]
    Rd = np.clip(0.30 + rng.normal(0, .06, (n, 1)) + rng.normal(0, .07, (n, 10)), 0, 1)
    maskd = rng.random((n, 10)) < 0.9
    fits = {"batch": bridging_multistart(R, mask)[0],
            "alone": bridging_multistart(R[:, cons], mask[:, cons])[0],
            "decoys": bridging_multistart(np.hstack([R, Rd]), np.hstack([mask, maskd]))[0]}
    with open("../data/rev_bridging_batch.csv", "w") as f:
        f.write("item,intercept_batch,intercept_alone,intercept_decoys,side_batch,side_alone,side_decoys\n")
        sb = {k: side_scores(v)[2] for k, v in fits.items()}
        for i, j in enumerate(cons):
            vals = (fits["batch"][2][j], fits["alone"][2][i], fits["decoys"][2][j],
                    sb["batch"][j], sb["alone"][i], sb["decoys"][j])
            f.write(f"{j + 1:02d}," + ",".join(f"{v:.4f}" for v in vals) + "\n")
            print(f"item {j + 1:02d}: intercept {vals[0]:+.3f} / {vals[1]:+.3f} / {vals[2]:+.3f} | "
                  f"side-balanced {vals[3]:.3f} / {vals[4]:.3f} / {vals[5]:.3f}")

    # (c) the ten simulation items, side by side (side B = the side where camp B is the majority)
    a, b, m, lab = side_scores(fits["batch"])
    side_b = 1 if (true_f[lab == 1] > 0).mean() > 0.5 else 0
    A, B = (b, a) if side_b == 0 else (a, b)
    with open("../data/rev_bridging_sides.csv", "w") as f:
        f.write("item,side_A,side_B,side_mean,side_min,intercept\n")
        for j in range(10):
            f.write(f"{j + 1:02d},{A[j]:.4f},{B[j]:.4f},{m[j]:.4f},{min(A[j], B[j]):.4f},{fits['batch'][2][j]:.4f}\n")
    print("side sizes:", int((lab != side_b).sum()), int((lab == side_b).sum()))
