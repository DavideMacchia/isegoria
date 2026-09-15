import numpy as np
from scipy.optimize import minimize
from scipy.special import logsumexp
rng = np.random.default_rng(11)

NT, NA, K = 3000, 30, 8      # respondents, anchor items, items under validation

def run(n_biased, seed):
    r = np.random.default_rng(seed)
    theta = r.normal(0, 1, NT)
    edu = r.choice([-1, 1], NT)          # HIDDEN axis, never observed

    # clean anchors -> estimated theta
    aA, bA = r.uniform(.9, 1.6, NA), r.normal(0, 1, NA)
    XA = (r.random((NT, NA)) < 1/(1+np.exp(-aA*(theta[:,None]-bA)))).astype(float)
    th = (XA.sum(1) - XA.sum(1).mean()) / XA.sum(1).std()

    # batch under validation: n_biased items distorted on the same hidden axis
    a = r.uniform(1.0, 1.5, K); b = r.normal(0, .6, K)
    d_true = np.zeros(K); d_true[:n_biased] = .9
    X = (r.random((NT, K)) < 1/(1+np.exp(-a*(theta[:,None]-b-d_true*edu[:,None])))).astype(float)

    def nll(p, free_delta):
        pi = 1/(1+np.exp(-p[0]))
        aa, bb = p[1:1+K], p[1+K:1+2*K]
        dd = p[1+2*K:] if free_delta else np.zeros(K)
        out = []
        for z in (-1., 1.):
            lo = aa*(th[:,None]-bb-dd*z)
            out.append(np.sum(X*lo - np.logaddexp(0, lo), axis=1))
        w = np.log(np.array([1-pi, pi]))[:, None]
        return -logsumexp(np.array(out) + w, axis=0).sum()

    p0 = np.concatenate([[0.], np.ones(K), np.zeros(K), r.normal(0, .3, K)])
    full = minimize(nll, p0, args=(True,), method="L-BFGS-B",
                    options={"maxiter": 3000})
    null = minimize(nll, p0[:1+2*K], args=(False,), method="L-BFGS-B",
                    options={"maxiter": 3000})

    d_hat = np.abs(full.x[1+2*K:])
    lr = 2*(null.fun - full.fun)                       # likelihood ratio
    bic = lr - K*np.log(NT)                            # >0 => the 2-class model wins

    # how well the inferred latent class reconstructs the hidden axis
    pi = 1/(1+np.exp(-full.x[0])); aa, bb = full.x[1:1+K], full.x[1+K:1+2*K]
    dd = full.x[1+2*K:]
    ll = []
    for z in (-1., 1.):
        lo = aa*(th[:,None]-bb-dd*z)
        ll.append(np.sum(X*lo - np.logaddexp(0, lo), axis=1))
    post = np.exp(np.array(ll) + np.log(np.array([1-pi, pi]))[:,None])
    z_hat = post[1]/post.sum(0)
    return d_hat, d_true, lr, bic, abs(np.corrcoef(z_hat, edu)[0,1])

print("="*76)
print("DETECTING BIAS WITHOUT KNOWING WHICH AXIS TO LOOK ON")
print(f"{NT} respondents, {K} items in the batch, distorting axis never observed")
print("="*76)
print(f"{'biased items':>14}{'estimated delta (biased)':>26}{'(clean)':>12}"
      f"{'BIC':>10}{'axis recovered':>18}")
for nb in [1, 2, 3, 5, 8]:
    D, T, lr, bic, corr = [], None, 0, 0, 0
    res = [run(nb, 200+s) for s in range(3)]
    dh = np.mean([r[0][:nb] for r in res])
    dc = np.mean([r[0][nb:] for r in res]) if nb < K else float('nan')
    bic = np.mean([r[3] for r in res]); corr = np.mean([r[4] for r in res])
    verdict = "DETECTED" if bic > 0 and dh > .35 else "invisible"
    print(f"{nb:>10}/{K}{dh:>26.2f}{dc:>12.2f}{bic:>10.0f}{corr:>13.2f}   {verdict}")

# ----------------------------------------------------------------------
# REVIEW CAPACITY: how many questions per month a node can propose
# ----------------------------------------------------------------------
print("\n" + "="*76)
print("SUSTAINABLE PROPOSAL QUOTA")
print("="*76)
k_rev = 9                       # reviewers per question
print(f"{'reviews/month per node':>26}{'proposal quota/month':>24}")
for r_cap in [5, 9, 18, 27, 45]:
    print(f"{r_cap:>26}{r_cap/k_rev:>24.1f}")
print("\nconstraint:  quota <= (reviews a node is willing to do) / 9")
print("with 10,000 nodes and quota 2/month -> 20,000 questions/month to review")
print("                                     -> 180,000 reviews/month required")
print("                                     -> 18 reviews/month each")
print("survival 30% -> 6,000 questions/month to the pilot (unrealistic)")
print("real bottleneck: the pilot respondents, not the reviewers")
