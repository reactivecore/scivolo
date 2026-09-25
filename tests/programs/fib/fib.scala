//> using scala 3.9.0

def fibo(n: Int): Int = if n < 2 then n else fibo(n - 1) + fibo(n - 2)

@main def fib(args: String*): Unit =
  val n = args.headOption.fold(20)(_.toInt)
  println(s"fib($n) = ${fibo(n)}")
