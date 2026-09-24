"""Section 6.3: what random assignment and sparse overlap imply for coalitions.

Probability that a coalition holding a share s of the eligible reviewers wins a majority of a
k = 9 panel drawn uniformly at random, and the expected number of items two reviewers share in
one epoch when each rates r = 9 of m items. Writes data/assignment.csv.
"""
from scipy.stats import binom

with open("../data/assignment.csv", "w") as f:
    f.write("share,p_majority_k9\n")
    for s in (0.1, 0.2, 0.3, 0.4, 0.5):
        p = binom.sf(4, 9, s)
        f.write(f"{s},{p:.4f}\n")
        print(f"share {s:.1f}: P(>= 5 of 9) = {p:.4f}")
for m in (50, 100, 300):
    print(f"m = {m}: expected shared items per epoch {81 / m:.2f}; epochs to share 30: {30 * m / 81:.0f}")
