def fib(n): return n if n < 2 else fib(n - 1) + fib(n - 2)
print(f"fib(30) = {fib(30)}")
seed, M = 42, (1 << 64) - 1
arr = []
for _ in range(100000):
    seed = (seed * 6364136223846793005 + 1442695040888963407) & M
    v = (seed >> 33) & 0xFFFFFFFF
    arr.append(v - (1 << 32) if v >= (1 << 31) else v)
s = sorted(arr)
print(f"sorted: n={len(s)} min={s[0]} max={s[-1]} mid={s[50000]}")
