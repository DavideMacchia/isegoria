"""Section 7.2 (decisions D33-D36): evaluator scores that learn, and learn honestly.

(a) How many scored items rank reviewers: leave-one-out difference scores for a panel of nine,
    in two regimes (marked skill differences: forecast noise 0.05-0.30; subtle differences:
    shared crowd error with exposure 0-1). (b) Stationary level of the asymmetric update vs a
    symmetric one. (c) CUSUM false alarms and detection delay for a long-con reviewer.
(d) Scored outcomes per reviewer per month under the design-scale assumptions of Section 7.2.
Writes data/rev_eval_learning.csv, data/rev_eval_ema.csv, data/rev_eval_cusum.csv,
data/rev_eval_scenarios.csv.
"""
import numpy as np
from scipy.stats import spearmanr

# ---- (a) regime 1: marked differences (independent forecast noise) ----
rng = np.random.default_rng(3)


def panel_noise(n_items, sig):
    pi = rng.beta(2, 2, n_items)
    o = (rng.random(n_items) < pi).astype(float)
    p = np.clip(pi[None, :] + rng.normal(0, 1, (len(sig), n_items)) * sig[:, None], .02, .98)
    base = (p.sum(0)[None, :] - p) / (len(sig) - 1)
    return (base - o) ** 2 - (p - o) ** 2


sig = np.linspace(.05, .30, 9)
true1 = panel_noise(400000, sig).mean(1)
panel_noise(20000, sig)                                   # keeps the random stream of the original run
learn1 = {k: np.mean([spearmanr(panel_noise(k, sig).mean(1), true1)[0] for _ in range(300)])
          for k in (10, 25, 50, 100, 200, 400)}

# ---- (a) regime 2: subtle differences (exposure to a shared crowd error) ----
rng = np.random.default_rng(5)


def panel_bias(n, beta, noise=.10, cbias=.15, flip=None):
    pi = rng.beta(2, 2, n)
    o = (rng.random(n) < pi).astype(float)
    c = rng.normal(0, cbias, n)
    p = np.clip(pi[None, :] + beta[:, None] * c[None, :] + noise * rng.normal(0, 1, (len(beta), n)), .02, .98)
    if flip is not None:
        start, frac = flip
        m = (np.arange(n) >= start) & (rng.random(n) < frac)
        p[0, m] = 1 - p[0, m]
    base = (p.sum(0)[None, :] - p) / (len(beta) - 1)
    return (base - o) ** 2 - (p - o) ** 2


beta = np.linspace(0, 1, 9)
true2 = panel_bias(300000, beta).mean(1)
panel_bias(20000, beta)
learn2 = {k: np.mean([spearmanr(panel_bias(k, beta).mean(1), true2)[0] for _ in range(300)])
          for k in (10, 25, 50, 100, 200, 400)}
with open("../data/rev_eval_learning.csv", "w") as f:
    f.write("k,rank_corr_marked,rank_corr_subtle\n")
    for k in learn1:
        f.write(f"{k},{learn1[k]:.3f},{learn2[k]:.3f}\n")
        print(f"{k:>3} scored items: rank correlation {learn1[k]:.2f} (marked) / {learn2[k]:.2f} (subtle)")


# ---- (b) asymmetric vs symmetric update ----
def ema(d, up, down):
    e, out = 0.0, np.empty(len(d))
    for i, x in enumerate(d):
        e += (up if x >= e else down) * (x - e)
        out[i] = e
    return out


with open("../data/rev_eval_ema.csv", "w") as f:
    f.write("reviewer,true_mean,ema_asymmetric,ema_symmetric\n")
    for label, noise in (("cautious", .05), ("bold", .20)):
        b = np.array([.3] + [.8] * 8)
        d = panel_bias(200000, b, noise=noise)[0]
        row = (d.mean(), ema(d, .02, .2)[1000:].mean(), ema(d, .05, .05)[1000:].mean())
        f.write(f"{label}," + ",".join(f"{v:.4f}" for v in row) + "\n")
        print(f"{label}: true {row[0]:+.4f} | asymmetric {row[1]:+.4f} | symmetric {row[2]:+.4f}")
    f.write("crowd_copier,0.0000,0.0000,0.0000\n")


# ---- (c) CUSUM ----
def cusum(d, ref, k, h):
    s, alarms = 0.0, []
    for t, x in enumerate(d):
        s = max(0.0, s + (ref - x) - k)
        if s > h:
            alarms.append(t)
            s = 0.0
    return alarms


b = np.array([.3] + [.8] * 8)
ref = panel_bias(200000, b)[0].mean()
with open("../data/rev_eval_cusum.csv", "w") as f:
    f.write("k,h,false_alarms_per_1000,median_delay\n")
    for k_, h in ((.02, 1.0), (.03, 1.5), (.04, 2.0)):
        fa = np.mean([len(cusum(panel_bias(4000, b)[0], ref, k_, h)) for _ in range(100)]) / 4
        delays = []
        for _ in range(200):
            al = [t for t in cusum(panel_bias(1300, b, flip=(300, .2))[0], ref, k_, h) if t >= 300]
            delays.append(al[0] - 300 if al else np.nan)
        f.write(f"{k_},{h},{fa:.2f},{np.nanmedian(delays):.0f}\n")
        print(f"CUSUM k={k_} h={h}: {fa:.2f} false alarms per 1000; long-con caught after {np.nanmedian(delays):.0f}")

# ---- (d) scored outcomes per reviewer per month (assumptions in Section 7.2) ----
reviews = 667 * 9 / 1000                         # live reviews per active reviewer per month
golden = reviews * 5 / 95                        # golden items at 5% of the queue
scen = {"golden_5pct": golden,
        "golden_20pct": reviews * 20 / 80,
        "live_plus_exploration": golden + 0.5 * reviews + 0.05 * 0.5 * reviews}
with open("../data/rev_eval_scenarios.csv", "w") as f:
    f.write("scenario,scored_per_month,months_to_100,months_to_36\n")
    for k, v in scen.items():
        f.write(f"{k},{v:.2f},{100 / v:.0f},{36 / v:.0f}\n")
        print(f"{k}: {v:.2f} scored/month -> 100 in {100 / v:.0f} months; 36 in {36 / v:.0f} months")
