"""Section 3.3: bridge scores are zero-sum within a fit, so the gate is batch-relative.

Checks the stationarity identities of Lemma 1 on the repository simulation's dataset and
scores the six consensus items twice: inside the simulation's ten-item batch, and alone.
Writes data/levelA_relativity.csv and data/levelA_identities.csv.
"""
import numpy as np
from common import bridging_multistart, bootstrap_min, sim_levelA_dataset

LAM_B, LAM_F, TAU = 0.15, 0.03, 0.08
names = ["01", "02", "03", "04", "05", "06", "07", "08", "09", "10"]
R, mask, _ = sim_levelA_dataset()

(mu, bu, bj, fu, fj), res = bridging_multistart(R, mask)
ident = {
    "grad_inf_norm": np.abs(res.jac).max(),
    "sum_b_items": bj.sum(),
    "sum_b_reviewers": bu.sum(),
    "lam_b_sum_fj_bj": LAM_B * (fj @ bj),
    "lam_f_sum_fu": LAM_F * fu.sum(),
    "lam_b_sum_fu_bu": LAM_B * (fu @ bu),
    "lam_f_sum_fj": LAM_F * fj.sum(),
    "sum_fu_sq": fu @ fu,
    "sum_fj_sq": fj @ fj,
    "mu": mu,
}
for key, val in ident.items():
    print(f"{key:>18} = {val: .3e}")
with open("../data/levelA_identities.csv", "w") as f:
    f.write("quantity,value\n")
    for key, val in ident.items():
        f.write(f"{key},{val:.6e}\n")

in_batch = bootstrap_min(R, mask)
consensus = [0, 1, 3, 4, 5, 6]
alone = bootstrap_min(R[:, consensus], mask[:, consensus])
with open("../data/levelA_relativity.csv", "w") as f:
    f.write("item,B_in_batch,B_alone\n")
    for j in range(10):
        a = alone[consensus.index(j)] if j in consensus else float("nan")
        f.write(f"{names[j]},{in_batch[j]:.4f},{a:.4f}\n")
        print(f"item {names[j]}: in batch {in_batch[j]:+.3f}" +
              (f"   alone {a:+.3f}" if j in consensus else ""))
print("pass at tau in batch:", [names[j] for j in consensus if in_batch[j] >= TAU],
      " alone:", [names[j] for i, j in enumerate(consensus) if alone[i] >= TAU])
