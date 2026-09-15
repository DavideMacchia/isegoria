"""Export the Level A dataset and full-fit results as fixtures for the Rust tests.
Mirrors the setup of bridging_irt_dif.py (seed 7) and the full fit (seed 0)."""
import numpy as np
from scipy.optimize import minimize
import os, sys

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(7)

N, share_B = 200, 0.60
true_f = np.concatenate([rng.normal(-1, .25, int(N*(1-share_B))),
                         rng.normal(+1, .25, int(N*share_B))])
true_sev = rng.normal(0, .06, N)

items = [("01 number of deputies",       .86,  .00),
         ("02 constitutional majority",  .84,  .00),
         ("03 real health spending",     .58,  .75),
         ("04 what is the ESM",          .85,  .05),
         ("05 capital of Italy",         .88,  .00),
         ("06 wrong answer key",         .84,  .00),
         ("07 EU technical jargon",      .85,  .00),
         ("08 partisan item (majority)", .55,  .80),
         ("09 partisan item (minority)", .55, -.80),
         ("10 electoral reform",         .72,  .30)]
names = [i[0] for i in items]
q = np.array([i[1] for i in items]); lean = np.array([i[2] for i in items])
M = len(items)

mask = np.zeros((N, M), bool)
for u in range(N):
    mask[u, rng.choice(M, 9, replace=False)] = True
for u in range(N):
    if true_f[u] > 0 and mask[u, 8] and rng.random() < .8: mask[u, 8] = False
R = np.clip(q[None,:] + .45*np.outer(true_f, lean) + true_sev[:,None]
            + rng.normal(0, .07, (N, M)), 0, 1)

def fit(R, mask, lam_b=.15, lam_f=.03, seed=0):
    n, m = R.shape; r = np.random.default_rng(seed)
    idx = np.where(mask); obs = R[idx]
    x0 = np.concatenate([[obs.mean()], r.normal(0,.1,n), r.normal(0,.1,m),
                         r.normal(0,.3,n), r.normal(0,.3,m)])
    def up(x): return x[0], x[1:1+n], x[1+n:1+n+m], x[1+n+m:1+2*n+m], x[1+2*n+m:]
    def L(x):
        mu,bu,bj,fu,fj = up(x)
        e = mu+bu[idx[0]]+bj[idx[1]]+fu[idx[0]]*fj[idx[1]]-obs
        return (e**2).sum()+lam_b*(bu@bu+bj@bj)+lam_f*(fu@fu+fj@fj)
    def G(x):
        mu,bu,bj,fu,fj = up(x)
        e = 2*(mu+bu[idx[0]]+bj[idx[1]]+fu[idx[0]]*fj[idx[1]]-obs)
        return np.concatenate([[e.sum()],
            np.bincount(idx[0],e,n)+2*lam_b*bu, np.bincount(idx[1],e,m)+2*lam_b*bj,
            np.bincount(idx[0],e*fj[idx[1]],n)+2*lam_f*fu,
            np.bincount(idx[1],e*fu[idx[0]],m)+2*lam_f*fj])
    return up(minimize(L, x0, jac=G, method="L-BFGS-B", options={"maxiter":4000}).x)

mu_hat, bu_hat, bj_hat, fu_hat, fj_hat = fit(R, mask)
sgn = np.sign(np.corrcoef(true_f, fu_hat)[0,1])
corr = abs(np.corrcoef(true_f, fu_hat)[0,1])

Bs = np.array([fit(R, mask & (np.random.default_rng(100+s).random(mask.shape) < .85),
                   seed=s)[2] for s in range(10)])
bridge = Bs.min(0)
plain = np.array([R[mask[:, j], j].mean() for j in range(M)])
TAU = .08

# --- dump ---
np.savetxt(f"{OUT}/R.csv", R, delimiter=",", fmt="%.10f")
np.savetxt(f"{OUT}/mask.csv", mask.astype(int), delimiter=",", fmt="%d")
np.savetxt(f"{OUT}/true_f.csv", true_f, delimiter=",", fmt="%.10f")
with open(f"{OUT}/expected_levelA.csv", "w") as fo:
    fo.write("idx,name,q,lean,bj_full,fj_full_signed\n")
    for j in range(M):
        fo.write(f"{j},{names[j]},{q[j]:.4f},{lean[j]:.4f},"
                 f"{bj_hat[j]:.6f},{fj_hat[j]*sgn:.6f}\n")
with open(f"{OUT}/expected_meta.csv", "w") as fo:
    fo.write("key,value\n")
    fo.write(f"N,{N}\nM,{M}\nmu_hat,{mu_hat:.6f}\ncorr_axis,{corr:.6f}\n"
             f"tau,0.08\nlam_b,0.15\nlam_f,0.03\n")
print("fit full: mu=%.4f  corr_axis=%.4f" % (mu_hat, corr))
print("bj_full:", np.round(bj_hat, 3).tolist())

# ===================== Level B — empirical validation ======================
# Mirrors the Level B block of bridging_irt_dif.py (same rng, seed 7 continued).
NT, NA = 1500, 30
theta = rng.normal(0, 1, NT)
grp = rng.choice([-1, 1], NT)
aA, bA = rng.uniform(.9, 1.6, NA), rng.normal(0, 1, NA)
XA = (rng.random((NT, NA)) < .25 + .75 / (1 + np.exp(-aA * (theta[:, None] - bA)))).astype(float)
th = (XA.sum(1) - XA.sum(1).mean()) / XA.sum(1).std()

psy = {"01 number of deputies": (1.5, -.2, 0, 0), "02 constitutional majority": (1.6, .9, 0, 0),
       "03 real health spending": (1.4, .3, 0, 0), "04 what is the ESM": (1.4, .1, .85, 0),
       "05 capital of Italy": (.15, -2.8, 0, 0), "06 wrong answer key": (1.5, 0, 0, 1),
       "07 EU technical jargon": (1.2, .4, 0, 0), "08 partisan item (majority)": (1.3, .2, 0, 0),
       "09 partisan item (minority)": (1.3, .1, 0, 0), "10 electoral reform": (1.4, .5, .25, 0)}
X = np.zeros((NT, M))
for j, nm in enumerate(names):
    a, b, d, w = psy[nm]
    x = (rng.random(NT) < .25 + .75 / (1 + np.exp(-a * (theta - b + d * grp)))).astype(float)
    X[:, j] = 1 - x if w else x


def dif(y, t, g):
    Z = np.column_stack([np.ones(len(y)), t, g, t * g])
    f = lambda w: np.logaddexp(0, Z @ w).sum() - (y * (Z @ w)).sum()
    gr = lambda w: Z.T @ (1 / (1 + np.exp(-(Z @ w))) - y)
    return minimize(f, np.zeros(4), jac=gr, method="L-BFGS-B").x


np.savetxt(f"{OUT}/levelb_XA.csv", XA, delimiter=",", fmt="%d")
np.savetxt(f"{OUT}/levelb_X.csv", X, delimiter=",", fmt="%d")
np.savetxt(f"{OUT}/levelb_grp.csv", grp, delimiter=",", fmt="%d")
ok_emp = np.zeros(M)
with open(f"{OUT}/levelb_expected.csv", "w") as fo:
    fo.write("idx,p,r_pbis,beta2\n")
    for j in range(M):
        rp = np.corrcoef(X[:, j], XA.sum(1))[0, 1]
        b2 = dif(X[:, j], th, grp)[2]
        ok_emp[j] = 1.0 if (rp >= .20 and abs(b2) <= .40) else 0.0
        fo.write(f"{j},{X[:, j].mean():.6f},{rp:.6f},{b2:.6f}\n")

# ===================== Level C — evaluator score (BSS) =====================
# Mirrors the VALUTATORI block of bridging_irt_dif.py.
out = np.array([1. if (bridge[j] >= TAU and ok_emp[j]) else 0. for j in range(M)])
base = out.mean()
profiles = [
    ("base_rate", lambda j: base),
    ("follows_peers", lambda j: .9 if plain[j] >= .60 else .1),
    ("follows_bridging", lambda j: .9 if bridge[j] >= TAU else .1),
    ("expert", lambda j: .9 if out[j] else .1),
    ("partisan", lambda j: .9 if lean[j] > .3 else .1),
]
P = np.array([[min(max(f(j), .02), .98) for j in range(M)] for _, f in profiles])
np.savetxt(f"{OUT}/levelc_o.csv", out, delimiter=",", fmt="%.1f")
np.savetxt(f"{OUT}/levelc_p.csv", P, delimiter=",", fmt="%.4f")
with open(f"{OUT}/levelc_bss.csv", "w") as fo:
    fo.write("profile,bss\n")
    for k, (nm, _) in enumerate(profiles):
        bss = 1 - ((P[k] - out) ** 2).sum() / (((base - out) ** 2).sum())
        fo.write(f"{nm},{bss:.6f}\n")

# ===================== Mixture IRT — latent-class DIF ======================
# One instance of latent_dif_and_capacity.py run(n_biased, seed), dumped whole.
def mixture_instance(n_biased, seed, NTm=3000, NAm=30, K=8):
    r = np.random.default_rng(seed)
    th_true = r.normal(0, 1, NTm)
    edu = r.choice([-1, 1], NTm)                       # hidden axis, never observed
    aA2, bA2 = r.uniform(.9, 1.6, NAm), r.normal(0, 1, NAm)
    XA2 = (r.random((NTm, NAm)) < 1 / (1 + np.exp(-aA2 * (th_true[:, None] - bA2)))).astype(float)
    thm = (XA2.sum(1) - XA2.sum(1).mean()) / XA2.sum(1).std()
    a = r.uniform(1.0, 1.5, K); b = r.normal(0, .6, K)
    d_true = np.zeros(K); d_true[:n_biased] = .9
    Xm = (r.random((NTm, K)) < 1 / (1 + np.exp(-a * (th_true[:, None] - b - d_true * edu[:, None])))).astype(float)
    return thm, Xm, edu, d_true


for tag, nb in [("batch", 3), ("single", 1)]:
    thm, Xm, edu, _ = mixture_instance(nb, 200)
    np.savetxt(f"{OUT}/mixture_{tag}_theta.csv", thm, delimiter=",", fmt="%.10f")
    np.savetxt(f"{OUT}/mixture_{tag}_X.csv", Xm, delimiter=",", fmt="%d")
    np.savetxt(f"{OUT}/mixture_{tag}_edu.csv", edu, delimiter=",", fmt="%d")
    with open(f"{OUT}/mixture_{tag}_meta.csv", "w") as fo:
        fo.write("key,value\n")
        fo.write(f"NT,{Xm.shape[0]}\nK,{Xm.shape[1]}\nn_biased,{nb}\nseed,200\n")

print("wrote fixtures to", OUT)
