//> using scala 3.9.0

def steps(start: Long): Int =
  var n = start
  var k = 0
  while n != 1L do
    n = if n % 2L == 0L then n / 2L else 3L * n + 1L
    k += 1
  k

@main def collatz(): Unit =
  var best = 0
  var arg = 0
  var i = 1
  while i < 1000000 do
    val s = steps(i)
    if s > best then
      best = s
      arg = i
    i += 1
  println(s"longest below 1e6: $arg ($best steps)")
