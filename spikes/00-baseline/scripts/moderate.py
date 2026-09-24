import math
from dataclasses import dataclass, replace
from collections import Counter
from itertools import groupby

@dataclass(frozen=True)
class Employee:
    name: str; age: int; dept: str; salary: float

@dataclass(frozen=True)
class Circle: r: float
@dataclass(frozen=True)
class Rect: w: float; h: float
@dataclass(frozen=True)
class Tri: a: float; b: float; c: float

def area(s):
    match s:
        case Circle(r): return math.pi * r * r
        case Rect(w, h): return w * h
        case Tri(a, b, c):
            p = (a + b + c) / 2
            return math.sqrt(p * (p - a) * (p - b) * (p - c))

def describe(x):
    match x:
        case int(i) if i < 0: return f"negative int {i}"
        case int(i): return f"int {i}"
        case str(s): return f"string of length {len(s)}"
        case (a, b): return f"pair({a}, {b})"
        case list(xs): return f"list of {len(xs)}"
        case _: return "unknown"

DEPTS = ["Engineering", "Sales", "Support"]
staff = [Employee("Alice", 34, "Engineering", 120000), Employee("Bob", 45, "Sales", 90000),
         Employee("Carol", 29, "Engineering", 110000), Employee("Dave", 52, "Support", 70000),
         Employee("Eve", 38, "Sales", 95000), Employee("Frank", 41, "Support", 72000)]
by = {}
for e in staff: by.setdefault(e.dept, []).append(e)
for d in sorted(by, key=DEPTS.index):
    es = by[d]; avg = sum(e.salary for e in es) / len(es)
    print(f"{d:<12} n={len(es)} avg={avg:.2f} names={','.join(e.name for e in es)}")
seniors = [replace(e, salary=e.salary * 1.1) for e in staff if e.age > 40]
print("Seniors: " + ", ".join(f"{e.name}:{int(e.salary)}" for e in seniors))
shapes = [Circle(1.0), Rect(2, 3), Tri(3, 4, 5), Circle(2.5)]
print(f"Total area: {sum(map(area, shapes)):.3f}; largest: {max(shapes, key=area)}")
for x in [-3, 7, "hello", (1, "x"), [1, 2, 3], 3.14]: print(describe(x))
freq = sorted(Counter("the quick brown fox jumps over the lazy dog the end".split()).items(), key=lambda t: (-t[1], t[0]))
print("top: " + ", ".join(f"{w}={c}" for w, c in freq[:3]))
print("Oldest: " + max(staff, key=lambda e: e.age).name)
