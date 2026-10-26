"""The rows that differ between two snapshots, grouped into worsened, improved and other changes."""

PASSED, REFUSED, FAILED = 2, 1, 0
SYNTAX = {"passed": PASSED, "differ": FAILED, "errors differ": FAILED, "messages differ": FAILED,
          "warnings differ": FAILED, "crashed": FAILED}
RANKS = {
    "typer": {"passed": PASSED, "unsupported": REFUSED, "differ": FAILED, "warnings differ": FAILED,
              "rejected": FAILED, "accepted": FAILED, "syntax errors": FAILED, "crashed": FAILED},
    "lower": {"passed": PASSED, "lowering refused": REFUSED, "typer refused": REFUSED, "parser refused": REFUSED,
              "wrong output": FAILED, "crashed": FAILED, "timeout": FAILED, "frontend crashed": FAILED},
    "tokens": SYNTAX,
    "trees": SYNTAX,
    "desugar": SYNTAX,
    "programs": {"ok": PASSED, "differs": FAILED, "timeout": FAILED},
    "lib": {"same": 3, "differs": 2, "refused": 1, "timeout": 0, "crashed": 0},
}
GROUPS = ["worsened", "improved", "other"]


def direction(oracle, before, after):
    """`worsened`, `improved` or `other` for a row whose value went from `before` to `after`."""
    ranks = RANKS.get(oracle, {})
    b, a = (ranks.get(v.split("\t", 1)[0]) for v in (before, after))
    if b is None or a is None or a == b:
        return "other"
    return "improved" if a > b else "worsened"


def grouped(oracle, before, after):
    """{group: lines} of each row that differs between the {name: value} maps `before` and `after`."""
    groups = {g: [] for g in GROUPS}
    for name in sorted(before.keys() | after.keys()):
        b, a = before.get(name, "missing"), after.get(name, "missing")
        if b != a:
            shown = (v.replace("\t", " ").strip()[:120] for v in (b, a))
            groups[direction(oracle, b, a)].append("    {}: {} -> {}".format(name, *shown))
    return groups


def lines(groups, shown):
    """The heading and at most `shown` rows of each non-empty group, worsened first."""
    out = []
    for g in GROUPS:
        rows = groups[g]
        if rows:
            more = [f"    ... {len(rows) - shown} more"] if len(rows) > shown else []
            out += [f"  {g} {len(rows)}", *rows[:shown], *more]
    return out
