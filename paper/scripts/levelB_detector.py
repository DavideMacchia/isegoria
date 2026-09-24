"""Sections 4.4-4.5: the production detector (Rust, crates/scoring) on controlled batches.

(a) 0, 1 or 2 biased items (delta = 0.9), proxy vs exact theta, N in {3000, 6000}.
(b) No biased item, N = 6000, theta proxy from 10, 20, 30 or 60 anchors.
Needs cargo; builds dif-harness/ in release mode. Writes data/levelB_detector.csv and
data/levelB_anchors.csv.
"""
import os
import subprocess
import tempfile
import numpy as np
from common import dif_generate

HERE = os.path.dirname(os.path.abspath(__file__))
subprocess.run(["cargo", "build", "--release", "-q"], cwd=os.path.join(HERE, "dif-harness"), check=True)
BIN = os.path.join(HERE, "dif-harness", "target", "release", "dif-harness")


def detect(th, X):
    with tempfile.TemporaryDirectory() as d:
        np.savetxt(os.path.join(d, "th.csv"), th[:, None], delimiter=",", fmt="%.10f")
        np.savetxt(os.path.join(d, "x.csv"), X, delimiter=",", fmt="%d")
        out = subprocess.run([BIN, "th.csv", "x.csv"], cwd=d, capture_output=True, text=True,
                             check=True).stdout.strip()
    classes, non_uniform, bic_gain, converged, dif, flags = out.split(",")
    return (int(classes), float(bic_gain), [float(v) for v in dif.split(";")],
            [int(v) for v in flags.split(";")])


with open(os.path.join(HERE, "..", "data", "levelB_detector.csv"), "w") as f:
    f.write("proxy,n_biased,N,seed,classes,bic_gain,max_gap_biased,max_gap_clean,"
            "flagged_biased,flagged_clean\n")
    for proxy in (True, False):
        for n_biased in (0, 1, 2):
            for n in (3000, 6000):
                for s in range(4):
                    th, X = dif_generate(n, n_biased, 0.9, 900 + s, proxy=proxy)
                    classes, bic, dif, flags = detect(th, X)
                    gb = max(dif[:n_biased]) if n_biased else float("nan")
                    row = (int(proxy), n_biased, n, s, classes, bic, gb, max(dif[n_biased:]),
                           sum(flags[:n_biased]), sum(flags[n_biased:]))
                    f.write(",".join(str(v) for v in row) + "\n")
                    f.flush()
                    print(row, flush=True)

with open(os.path.join(HERE, "..", "data", "levelB_anchors.csv"), "w") as f:
    f.write("anchors,seed,classes,bic_gain,max_gap_clean,flagged_clean\n")
    for n_anchor in (10, 20, 30, 60):
        for s in range(4):
            th, X = dif_generate(6000, 0, 0.9, 1300 + s, proxy=True, n_anchor=n_anchor)
            classes, bic, dif, flags = detect(th, X)
            row = (n_anchor, s, classes, bic, max(dif), sum(flags))
            f.write(",".join(str(v) for v in row) + "\n")
            f.flush()
            print(row, flush=True)
