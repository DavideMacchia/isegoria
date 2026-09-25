#!/usr/bin/env python3
"""Comment budget (`docs/CLAUDE.md`, "Code style and docs"): `comment_budget.py [PATH...]` checks
files or directories (default: crates, sim, scripts); `--hook` is the Claude Code PostToolUse
entry point and reads the tool call on stdin. Exit 0 within budget, 1 otherwise (2 as a hook)."""
import ast
import io
import json
import re
import sys
import tokenize
from pathlib import Path

MODULE_DOC_MAX = 3  # consecutive `//!` lines, or the module docstring
ITEM_DOC_MAX = 3  # consecutive `///` lines, or a def/class docstring
INLINE_MAX = 2  # consecutive `//` or `#` lines
LINE_MAX = 100  # characters in a comment line, rustfmt's max_width
SHARE_MAX = 0.10  # comment lines over all lines, for files with more than SHARE_FLOOR comment lines
SHARE_FLOOR = 20
DEFAULT_PATHS = ("crates", "sim", "scripts")
ROOT = Path(__file__).resolve().parent.parent


def rust_lines(text):
    kinds = []
    for line in text.split("\n"):
        s = line.lstrip()
        if s.startswith("//!"):
            kinds.append("module")
        elif s.startswith("///"):
            kinds.append("item")
        elif s.startswith("//"):
            kinds.append("inline")
        else:
            kinds.append(None)
    return kinds


def python_lines(text):
    kinds = [None] * (text.count("\n") + 1)
    try:
        for tok in tokenize.generate_tokens(io.StringIO(text).readline):
            if tok.type == tokenize.COMMENT and tok.line[: tok.start[1]].strip() == "":
                kinds[tok.start[0] - 1] = "inline"
        tree = ast.parse(text)
    except (SyntaxError, tokenize.TokenError):
        return kinds
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            body = node.body
            if body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant) and isinstance(body[0].value.value, str):
                kind = "module" if isinstance(node, ast.Module) else "item"
                for i in range(body[0].lineno - 1, body[0].end_lineno):
                    kinds[i] = kind
    return kinds


def check(path):
    text = path.read_text(encoding="utf-8")
    kinds = rust_lines(text) if path.suffix == ".rs" else python_lines(text)
    limits = {"module": MODULE_DOC_MAX, "item": ITEM_DOC_MAX, "inline": INLINE_MAX}
    problems = []
    i = 0
    while i < len(kinds):
        kind = kinds[i]
        j = i
        while j < len(kinds) and kinds[j] == kind:
            j += 1
        if kind and j - i > limits[kind]:
            problems.append(f"{path}:{i + 1}: {kind} comment of {j - i} lines (max {limits[kind]})")
        i = j
    for n, (kind, line) in enumerate(zip(kinds, text.split("\n"))):
        if kind and len(line) > LINE_MAX:
            problems.append(f"{path}:{n + 1}: comment line of {len(line)} characters (max {LINE_MAX})")
    total = len(kinds)
    comments = sum(1 for k in kinds if k)
    if comments > SHARE_FLOOR and comments > SHARE_MAX * total:
        problems.append(f"{path}: {comments} comment lines of {total} ({100 * comments / total:.0f}%, max {100 * SHARE_MAX:.0f}%)")
    return problems


def files(paths):
    for p in paths:
        p = Path(p)
        if p.is_dir():
            yield from (f for f in sorted(p.rglob("*")) if f.suffix in (".rs", ".py") and "target" not in f.parts)
        elif p.suffix in (".rs", ".py"):
            yield p


def main(argv):
    if argv[:1] == ["--hook"]:
        call = json.load(sys.stdin)
        raw = call.get("tool_input", {}).get("file_path")
        if not raw:
            return 0
        path = Path(raw).resolve()
        try:
            rel = path.relative_to(ROOT)
        except ValueError:
            return 0
        if rel.parts[0] not in DEFAULT_PATHS or path.suffix not in (".rs", ".py") or not path.exists():
            return 0
        problems = check(rel if Path.cwd() == ROOT else path)
        if problems:
            print("Comment budget exceeded (docs/CLAUDE.md, Code style and docs):", *problems, sep="\n", file=sys.stderr)
            return 2
        return 0
    if argv[:1] in (["-h"], ["--help"]):
        print(__doc__)
        return 0
    problems = [p for f in files(argv or DEFAULT_PATHS) for p in check(f)]
    print(*problems, sep="\n") if problems else None
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
