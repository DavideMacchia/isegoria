"""Section 7.5 (decisions D39-D41): residual correlation, panel diversification, the beacon.

(a) Raw vs residual correlation on a long dense history: 200 reviewers x 60 items, camps 40/60,
    a cartel of 10 members of the minority camp rating 5 items favoured by the majority camp
    at 1.0 (+ jitter 0.05). (b) Probability that a cluster of 50 among 1,000 reviewers places
    2+ or 3+ members on a uniformly drawn panel of 9. (c) Withholding the last reveal of a
    commit-reveal beacon doubles a one-shot chance of k/n.
Writes data/rev_collusion.csv and data/rev_panels.csv.
"""
import numpy as np
from scipy.stats import binom, hypergeom
from common import bridging_multistart

rng = np.random.default_rng(11)
n, nA = 200, 80
true_f = np.concatenate([rng.normal(-1, .25, nA), rng.normal(1, .25, n - nA)])
sev = rng.normal(0, .06, n)
q = np.concatenate([rng.uniform(.8, .9, 30), rng.uniform(.5, .6, 30)])
lean = np.concatenate([np.zeros(30), np.tile([.8, -.8], 15)])
R = np.clip(q[None, :] + .45 * np.outer(true_f, lean) + sev[:, None] + rng.normal(0, .07, (n, 60)), 0, 1)
cartel = np.arange(10)
targets = np.where(lean > 0)[0][:5]
R[np.ix_(cartel, targets)] = np.clip(1.0 + rng.normal(0, .05, (10, 5)), 0, 1)
(mu, bu, bj, fu, fj), _ = bridging_multistart(R, np.ones_like(R, bool), starts=4)
resid = R - (mu + bu[:, None] + bj[None, :] + np.outer(fu, fj))

with open("../data/rev_collusion.csv", "w") as f:
    f.write("method,honest_same_camp,honest_cross_camp,cartel,threshold_90pct,honest_flagged_share,honest_flagged_pairs\n")
    for name, M in (("raw", R), ("residual", resid)):
        C = np.corrcoef(M)
        iu = np.triu_indices(n, 1)
        camp = (true_f > 0).astype(int)
        is_c = np.zeros(n, bool); is_c[cartel] = True
        both = is_c[iu[0]] & is_c[iu[1]]
        honest = ~is_c[iu[0]] & ~is_c[iu[1]]
        same = camp[iu[0]] == camp[iu[1]]
        v = C[iu]
        thr = np.quantile(v[both], .10)
        row = (v[honest & same].mean(), v[honest & ~same].mean(), v[both].mean(), thr,
               np.mean(v[honest] >= thr), int((v[honest] >= thr).sum()))
        f.write(f"{name}," + ",".join(f"{x:.4f}" for x in row[:5]) + f",{row[5]}\n")
        print(f"{name}: same camp {row[0]:+.2f}, cross camp {row[1]:+.2f}, cartel {row[2]:+.2f}, "
              f"threshold {row[3]:+.2f}, honest pairs flagged {row[4]:.1%} ({row[5]})")

with open("../data/rev_panels.csv", "w") as f:
    f.write("quantity,value\n")
    p2 = hypergeom.sf(1, 1000, 50, 9); p3 = hypergeom.sf(2, 1000, 50, 9)
    one = 9 / 1000; two = 1 - (1 - one) ** 2
    for k, v in (("p_two_or_more_hypergeom", p2), ("p_three_or_more_hypergeom", p3),
                 ("p_two_or_more_binom", binom.sf(1, 9, .05)), ("p_three_or_more_binom", binom.sf(2, 9, .05)),
                 ("discounted_share_sqrt50_over_50", np.sqrt(50) / 50),
                 ("one_shot_chance", one), ("with_withholding", two)):
        f.write(f"{k},{v:.4f}\n")
        print(f"{k}: {v:.4f}")
