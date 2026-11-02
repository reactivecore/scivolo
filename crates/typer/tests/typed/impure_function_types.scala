import language.experimental.captureChecking

object Impure:
  def map[A, B](xs: List[A], f: A => B): List[B] = xs.map(f)
  def pure[A, B](xs: List[A], f: A -> B): List[B] = xs.map(f)
  def ctx(f: Int ?=> String): String = f(using 1)
  def two(f: (Int, String) => Boolean): Boolean = f(1, "a")
  def thunk(f: () => Int): Int = f()
  def run: Int = map(List(1), x => x + 1).head + pure(List(2), _ * 2).head + thunk(() => 3)
