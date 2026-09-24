//> using scala 3.9.0

def fib(n: Int): Int = if n < 2 then n else fib(n - 1) + fib(n - 2)

@main def fibMain(): Unit = println(s"fib(30) = ${fib(30)}")
