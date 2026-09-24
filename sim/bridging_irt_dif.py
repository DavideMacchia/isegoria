import numpy as np
from scipy.optimize import minimize
rng = np.random.default_rng(7)

N, share_B = 200, 0.60
true_f = np.concatenate([rng.normal(-1, .25, int(N*(1-share_B))),
                         rng.normal(+1, .25, int(N*share_B))])
true_sev = rng.normal(0, .06, N)

#            name                        q     lean
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

# --- the side-balanced bridge score (docs/02 §A.3, docs/01 D32, T49) ---
def two_means(f, iters=100):
    """Deterministic 1-D 2-means on f_u, initialized at its extremes."""
    c = np.array([f.min(), f.max()])
    for _ in range(iters):
        lab = np.abs(f[:, None] - c[None, :]).argmin(1)
        new = np.array([f[lab == k].mean() if (lab == k).any() else c[k] for k in (0, 1)])
        if np.allclose(new, c):
            break
        c = new
    return lab


def side_scores(params):
    """Per-side mean predicted rating, the side-balanced score S_j and the side gap."""
    mu, bu, bj, fu, fj = params
    rhat = mu + bu[:, None] + bj[None, :] + np.outer(fu, fj)
    lab = two_means(fu)
    a, b = rhat[lab == 0].mean(0), rhat[lab == 1].mean(0)
    return a, b, (a + b) / 2, np.abs(a - b), lab


full = fit(R, mask)
mu_hat, _, bj_hat, fu_hat, fj_hat = full
_, _, side_full, gap_full, _ = side_scores(full)
Ss = np.array([side_scores(fit(R, mask & (np.random.default_rng(100+s).random(mask.shape) < .85),
                               seed=s))[2] for s in range(10)])
bridge = Ss.min(0)          # the robust (bootstrap-min) side-balanced score the gate reads
plain = np.array([R[mask[:,j], j].mean() for j in range(M)])
TAU = .80                   # absolute, on the probability scale (provisional, D32)

print("="*78); print("LEVEL A - peer review  (mu = %.2f, tau = %.2f)" % (mu_hat, TAU)); print("="*78)
print(f"{'item':<32}{'mean':>7}{'S_j':>7}{'gap':>7}{'b_j':>7}{'f_j':>7}{'majority':>11}{'bridging':>10}")
sgn = np.sign(np.corrcoef(true_f, fu_hat)[0,1])
for j in range(M):
    print(f"{names[j]:<32}{plain[j]:>7.2f}{bridge[j]:>7.2f}{gap_full[j]:>7.2f}{bj_hat[j]:>7.2f}"
          f"{fj_hat[j]*sgn:>7.2f}"
          f"{('pass' if plain[j]>=.60 else 'drop'):>11}"
          f"{('pass' if bridge[j]>=TAU else 'drop'):>10}")
print(f"\nlatent axis recovered: |corr(f estimated, f true)| = "
      f"{abs(np.corrcoef(true_f, fu_hat)[0,1]):.3f}")

# ---- LEVEL B ----
NT, NA = 1500, 30
theta = rng.normal(0,1,NT); grp = rng.choice([-1,1], NT)
aA, bA = rng.uniform(.9,1.6,NA), rng.normal(0,1,NA)
XA = (rng.random((NT,NA)) < .25+.75/(1+np.exp(-aA*(theta[:,None]-bA)))).astype(float)
th = (XA.sum(1)-XA.sum(1).mean())/XA.sum(1).std()      # theta estimated on the anchors

psy = {"01 number of deputies":(1.5,-.2,0,0), "02 constitutional majority":(1.6,.9,0,0),
       "03 real health spending":(1.4,.3,0,0), "04 what is the ESM":(1.4,.1,.85,0),
       "05 capital of Italy":(.15,-2.8,0,0), "06 wrong answer key":(1.5,0,0,1),
       "07 EU technical jargon":(1.2,.4,0,0), "08 partisan item (majority)":(1.3,.2,0,0),
       "09 partisan item (minority)":(1.3,.1,0,0), "10 electoral reform":(1.4,.5,.25,0)}
X = np.zeros((NT,M))
for j,nm in enumerate(names):
    a,b,d,w = psy[nm]
    x = (rng.random(NT) < .25+.75/(1+np.exp(-a*(theta-b+d*grp)))).astype(float)
    X[:,j] = 1-x if w else x

def dif(y, t, g):
    Z = np.column_stack([np.ones(len(y)), t, g, t*g])
    f = lambda w: np.logaddexp(0, Z@w).sum() - (y*(Z@w)).sum()
    gr = lambda w: Z.T@(1/(1+np.exp(-(Z@w)))-y)
    return minimize(f, np.zeros(4), jac=gr, method="L-BFGS-B").x

print("\n"+"="*78); print("LEVEL B - empirical validation (n=1500, 30 anchor items)"); print("="*78)
print(f"{'item':<32}{'p':>6}{'r_pbis':>8}{'beta2':>8}{'outcome':>24}")
ok_emp = {}
for j,nm in enumerate(names):
    rp = np.corrcoef(X[:,j], XA.sum(1))[0,1]; b2 = dif(X[:,j], th, grp)[2]
    why = ([] + (["discrim."] if rp < .20 else []) + (["DIF"] if abs(b2) > .40 else []))
    ok_emp[nm] = not why
    print(f"{nm:<32}{X[:,j].mean():>6.2f}{rp:>8.2f}{b2:>8.2f}"
          f"{('accepted' if not why else 'REJECTED: '+', '.join(why)):>24}")

# ---- combined verdict ----
print("\n"+"="*78); print("COMBINED VERDICT"); print("="*78)
for j,nm in enumerate(names):
    A = bridge[j] >= TAU; B = ok_emp[nm]
    v = "IN POOL" if (A and B) else ("stopped in A" if not A else "stopped in B")
    print(f"{nm:<32}{v}")

# ---- evaluators ----
out = np.array([1. if (bridge[j]>=TAU and ok_emp[names[j]]) else 0. for j in range(M)])
base = out.mean()
prof = {"always predicts the base rate": lambda j: base,
        "follows the peer average":      lambda j: .9 if plain[j]>=.60 else .1,
        "follows the bridging":          lambda j: .9 if bridge[j]>=TAU else .1,
        "psychometric expert":           lambda j: .9 if out[j] else .1,
        "partisan":                      lambda j: .9 if lean[j]>.3 else .1}
print("\n"+"="*78); print("EVALUATORS - Brier Skill Score"); print("="*78)
for nmv,f in prof.items():
    p = np.clip([f(j) for j in range(M)], .02, .98)
    print(f"{nmv:<32}BSS = {1-((p-out)**2).sum()/(((base-out)**2).sum()):>6.2f}")

# ---- corner case: elite consensus ----
edu = rng.choice([0,1], NT, p=[.6,.4])
xe = (rng.random(NT) < .25+.75/(1+np.exp(-1.2*(theta-.4+.9*(edu-.5)*2)))).astype(float)
print("\n"+"="*78); print("CORNER CASE 1 - elite consensus (item 07)"); print("="*78)
print(f"bridging         = {bridge[6]:+.2f}   passes peer review")
print(f"DIF political axis = {dif(xe, th, grp)[2]:+.2f}   no signal")
print(f"DIF education    = {dif(xe, th, (edu-.5)*2)[2]:+.2f}   item strongly distorted")

# ---- corner case: bipartisan cartel ----
print("\n"+"="*78); print("CORNER CASE 2 - cost of bipartisan corruption (item 08)"); print("="*78)
cB = rng.choice(np.where(true_f>0)[0], 40, replace=False)
for nA in [0,10,20,30,40,55,70]:
    R2, m2 = R.copy(), mask.copy()
    m2[cB,7] = True; R2[cB,7] = 1.
    if nA:
        cA = rng.choice(np.where(true_f<0)[0], min(nA, 79), replace=False)
        m2[cA,7] = True; R2[cA,7] = 1.
    s8 = side_scores(fit(R2, m2, seed=0))[2][7]
    print(f"  40 nodes camp B + {nA:>2} camp A -> bridging S_j {s8:.2f}  "
          f"{'PASS' if s8>=TAU else 'drop'}")

# ---- corner case: threshold sweep ----
print("\n"+"="*78); print("CORNER CASE 3 - the threshold trade-off"); print("="*78)
print(f"{'tau':>6}{'legit items lost':>24}{'partisan items admitted':>24}")
legit = [0,1,2,3,4,5,6]; part = [7,8,9]
for t in [.70,.74,.78,.80,.82,.86]:
    print(f"{t:>6.2f}{sum(bridge[j]<t for j in legit):>20}/7"
          f"{sum(bridge[j]>=t for j in part):>20}/3")
