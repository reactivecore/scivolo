//> using scala 3.9.0

def fib(n: Int): Int = if n < 2 then n else fib(n - 1) + fib(n - 2)

@main def cpu(): Unit =
  val f = fib(30)
  println(s"fib(30) = $f")
  // deterministic LCG so all runtimes sort the same data
  var seed = 42L
  val arr = Array.fill(100000) {
    seed = (seed * 6364136223846793005L + 1442695040888963407L)
    (seed >>> 33).toInt
  }
  val sorted = arr.toVector.sorted
  println(s"sorted: n=${sorted.length} min=${sorted.head} max=${sorted.last} mid=${sorted(50000)}")
