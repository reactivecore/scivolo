object Indent:
  class Box(val x: Int):
    def twice: Int =
      val y = x * 2
      y
    end twice
  end Box

  def classify(n: Int): String =
    if n < 0 then "neg"
    else if n == 0 then "zero"
    else
      val s = "pos"
      s

  def loop(xs: List[Int]): Int =
    var acc = 0
    for x <- xs if x > 0 do acc += x
    while acc > 100 do acc -= 1
    val ys =
      for
        x <- xs
        y <- xs
      yield x + y
    try ys.sum
    catch case e: Exception => -1
    finally println("done")

  val xs = List(1, 2, 3).map: x =>
    x + 1
  val ys = xs
    .filter: y =>
      y > 1
    .map(_ * 2)
  val total = xs.foldLeft(0): (acc, x) =>
    acc + x
end Indent
