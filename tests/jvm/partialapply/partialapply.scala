import language.`3.3`

class A:
  def f(x: Int)(y: Int): Int = x + y

  val sum = f(22).apply(33)

@main def Test =
  val grouped = Map(-1 -> 1, -2 -> 2, 0 -> 3, 1 -> 4, 2 -> 5)
    .groupMapReduce: (k, v) =>
      (k + 3) % 3
    .apply: (k, v) =>
      v * 2
    .apply: (x, y) =>
      x + y
  println(s"${new A().sum} $grouped")
