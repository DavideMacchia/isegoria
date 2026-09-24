"""Shared numerics for the paper's experiments.

Re-implements, in NumPy/SciPy, the Level A bridging fit and the two-class latent DIF model
used by the repository simulations (sim/), with analytic gradients and tight tolerances so
that stationarity conditions can be checked to optimizer precision. The production engine is
the Rust crate `crates/scoring`; `dif-harness/` runs that engine where the paper reports
production behaviour.
"""
import numpy as np
from scipy.optimize import minimize
from scipy.special import expit, logsumexp

# ----------------------------------------------------------------------------- Level A


def bridging_fit(R, mask, lam_b=0.15, lam_f=0.03, seed=0, weights=None):
    """Weighted bridging objective of docs/02 §A (d = 1), mu unpenalized.

    Returns (mu, b_u, b_j, f_u, f_j) and the SciPy result object.
    """
    n, m = R.shape
    rng = np.random.default_rng(seed)
    idx = np.where(mask)
    obs = R[idx]
    w = np.ones(n) if weights is None else np.asarray(weights, float)
    wo = w[idx[0]]
    x0 = np.concatenate([[obs.mean()], rng.normal(0, .1, n), rng.normal(0, .1, m),
                         rng.normal(0, .3, n), rng.normal(0, .3, m)])

    def unpack(x):
        return x[0], x[1:1 + n], x[1 + n:1 + n + m], x[1 + n + m:1 + 2 * n + m], x[1 + 2 * n + m:]

    def cost_grad(x):
        mu, bu, bj, fu, fj = unpack(x)
        e = mu + bu[idx[0]] + bj[idx[1]] + fu[idx[0]] * fj[idx[1]] - obs
        cost = (wo * e ** 2).sum() + lam_b * (bu @ bu + bj @ bj) + lam_f * (fu @ fu + fj @ fj)
        g = 2 * wo * e
        grad = np.concatenate([[g.sum()],
                               np.bincount(idx[0], g, n) + 2 * lam_b * bu,
                               np.bincount(idx[1], g, m) + 2 * lam_b * bj,
                               np.bincount(idx[0], g * fj[idx[1]], n) + 2 * lam_f * fu,
                               np.bincount(idx[1], g * fu[idx[0]], m) + 2 * lam_f * fj])
        return cost, grad

    res = minimize(cost_grad, x0, jac=True, method="L-BFGS-B",
                   options={"maxiter": 20000, "gtol": 1e-10, "ftol": 1e-15, "maxcor": 20})
    return unpack(res.x), res


def bridging_multistart(R, mask, starts=8, **kw):
    """Lowest objective over `starts` seeded starts (as the Rust engine does, T48)."""
    best = None
    for s in range(starts):
        params, res = bridging_fit(R, mask, seed=s, **kw)
        if best is None or res.fun < best[1].fun:
            best = (params, res)
    return best


def bootstrap_min(R, mask, n_boot=10, keep=0.85, **kw):
    """Bootstrap-min bridge score, repository-simulation convention (cold starts)."""
    scores = []
    for s in range(n_boot):
        sub = mask & (np.random.default_rng(100 + s).random(mask.shape) < keep)
        scores.append(bridging_fit(R, sub, seed=s, **kw)[0][2])
    return np.min(scores, axis=0)


def sim_levelA_dataset():
    """The Level A dataset of sim/bridging_irt_dif.py, rebuilt from the same RNG stream."""
    rng = np.random.default_rng(7)
    n, share_b = 200, 0.60
    true_f = np.concatenate([rng.normal(-1, .25, int(n * (1 - share_b))),
                             rng.normal(+1, .25, int(n * share_b))])
    sev = rng.normal(0, .06, n)
    q = np.array([.86, .84, .58, .85, .88, .84, .85, .55, .55, .72])
    lean = np.array([.00, .00, .75, .05, .00, .00, .00, .80, -.80, .30])
    m = len(q)
    mask = np.zeros((n, m), bool)
    for u in range(n):
        mask[u, rng.choice(m, 9, replace=False)] = True
    for u in range(n):
        if true_f[u] > 0 and mask[u, 8] and rng.random() < .8:
            mask[u, 8] = False
    R = np.clip(q[None, :] + .45 * np.outer(true_f, lean) + sev[:, None]
                + rng.normal(0, .07, (n, m)), 0, 1)
    return R, mask, true_f


def mirror_design(n, share_b, seed, m_consensus=10, m_partisan=10, per_reviewer=9):
    """Two camps; partisan items come in mirror pairs (lean +0.8 / -0.8, same quality)."""
    rng = np.random.default_rng(seed)
    n_b = int(round(n * share_b))
    true_f = np.concatenate([rng.normal(-1, .25, n - n_b), rng.normal(+1, .25, n_b)])
    sev = rng.normal(0, .06, n)
    q = np.concatenate([rng.uniform(.82, .88, m_consensus), np.full(m_partisan, .55)])
    lean = np.concatenate([np.zeros(m_consensus), np.tile([.8, -.8], m_partisan // 2)])
    m = m_consensus + m_partisan
    mask = np.zeros((n, m), bool)
    for u in range(n):
        mask[u, rng.choice(m, min(per_reviewer, m), replace=False)] = True
    R = np.clip(q[None, :] + .45 * np.outer(true_f, lean) + sev[:, None]
                + rng.normal(0, .07, (n, m)), 0, 1)
    return R, mask, lean

# ----------------------------------------------------------------------------- Level B


def dif_generate(n, n_biased, delta, seed, proxy=True, n_anchor=30, k=8):
    """Batch of k items, the first n_biased shifted by +-delta on a hidden binary axis.

    Mirrors sim/latent_dif_and_capacity.py. With proxy=True the ability used for the fit is
    the standardized total over n_anchor clean anchor items; otherwise the true theta.
    """
    r = np.random.default_rng(seed)
    theta = r.normal(0, 1, n)
    z = r.choice([-1, 1], n)
    a_anchor, b_anchor = r.uniform(.9, 1.6, n_anchor), r.normal(0, 1, n_anchor)
    XA = (r.random((n, n_anchor)) < expit(a_anchor * (theta[:, None] - b_anchor))).astype(float)
    total = XA.sum(1)
    th = (total - total.mean()) / total.std() if proxy else theta
    a, b = r.uniform(1.0, 1.5, k), r.normal(0, .6, k)
    d = np.zeros(k)
    d[:n_biased] = delta
    X = (r.random((n, k)) < expit(a * (theta[:, None] - b - d * z[:, None]))).astype(float)
    return th, X


def _dif_nll(p, th, X, mixture):
    k = X.shape[1]
    if mixture:
        eta, a, b, d = p[0], p[1:1 + k], p[1 + k:1 + 2 * k], p[1 + 2 * k:]
        pi = expit(eta)
        classes, log_w = (-1., 1.), np.log([1 - pi, pi])
    else:
        a, b, d = p[:k], p[k:2 * k], np.zeros(k)
        classes, log_w = (0.,), np.array([0.])
    ll, logits = [], []
    for z in classes:
        lo = a * (th[:, None] - b - d * z)
        ll.append((X * lo - np.logaddexp(0, lo)).sum(1))
        logits.append(lo)
    T = np.array(ll) + log_w[:, None]
    lse = logsumexp(T, axis=0)
    post = np.exp(T - lse)
    g_a, g_b, g_d = np.zeros(k), np.zeros(k), np.zeros(k)
    for c, z in enumerate(classes):
        resid = post[c][:, None] * (X - expit(logits[c]))
        g_a -= (resid * (th[:, None] - b - d * z)).sum(0)
        g_b += (resid * a).sum(0)
        g_d += (resid * a * z).sum(0)
    if mixture:
        return -lse.sum(), np.concatenate([[-(post[1] - pi).sum()], g_a, g_b, g_d])
    return -lse.sum(), np.concatenate([g_a, g_b])


def dif_fit(th, X, seed=0, starts=4):
    """One-class 2PL (null) and two-class uniform-DIF mixture; returns (null, mixture)."""
    k = X.shape[1]
    opts = {"maxiter": 5000, "gtol": 1e-8}
    null = minimize(_dif_nll, np.concatenate([np.ones(k), np.zeros(k)]), args=(th, X, False),
                    jac=True, method="L-BFGS-B", options=opts)
    rng = np.random.default_rng(seed)
    best = None
    for _ in range(starts):
        p0 = np.concatenate([[rng.normal(0, .3)], null.x[:k], null.x[k:], rng.normal(0, .5, k)])
        res = minimize(_dif_nll, p0, args=(th, X, True), jac=True, method="L-BFGS-B", options=opts)
        if best is None or res.fun < best.fun:
            best = res
    return null, best
