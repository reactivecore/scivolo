//> using scala 3.9.0

def sum(n: Int): Long =
  var acc = 0L
  var i = 1
  while i <= n do
    acc += i
    i += 1
  acc

def collatz(limit: Int): (Int, Int) =
  var best, bestLen = 1
  var start = 1
  while start < limit do
    var x = start.toLong
    var len = 1
    while x != 1 do
      x = if x % 2 == 0 then x / 2 else 3 * x + 1
      len += 1
    if len > bestLen then
      best = start
      bestLen = len
    start += 1
  (best, bestLen)

@main def loops(args: String*): Unit =
  val scale = args.headOption.fold(1)(_.toInt)
  println(s"sum = ${sum(100000 * scale)}")
  println(s"collatz = ${collatz(10000 * scale)}")
