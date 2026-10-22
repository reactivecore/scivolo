//> using scala 3.9.0
object tupleops {
  def cons(t: (String, Double)): (Int, String, Double) = 1 *: t
  def consLong(t: (Int, Int, Int, Int, Int)): (String, Int, Int, Int, Int, Int) = "a" *: t
  def consEmpty(x: Int): Tuple1[Int] = x *: EmptyTuple

  def tail(t: (Int, String, Boolean)): (String, Boolean) = t.tail
  def tailOne(t: Tuple1[Int]): EmptyTuple = t.tail
  def tailLong(t: (Int, Int, Int, Int, Int, Int, Int)): (Int, Int, Int, Int, Int, Int) = t.tail

  def size(t: (Int, String, Long)): Int = t.size

  def concat(a: (Int, Int), b: Tuple1[String]): (Int, Int, String) = a ++ b
  def concatLong(a: (Int, Int, Int), b: (String, String)): (Int, Int, Int, String, String) = a ++ b
  def concatEmpty(a: (Int, Int)): (Int, Int) = a ++ EmptyTuple
  def emptyConcat(a: (Int, Int)): (Int, Int) = EmptyTuple ++ a

  def array(t: (Int, String)): Array[Object] = t.toArray
  def emptyArray(t: EmptyTuple): Array[Object] = t.toArray

  def pair(): (Int, String) = (9, "h")
  def concatCall(b: Tuple1[Boolean]): (Int, String, Boolean) = pair() ++ b
  def tailCall(): Tuple1[String] = pair().tail

  def main(args: Array[String]): Unit = {
    println(concatCall(Tuple1(true)))
    println(tailCall())
    println(cons(("x", 2.5)))
    println(consLong((1, 2, 3, 4, 5)))
    println(consEmpty(7))
    println(tail((1, "b", true)))
    println(tailOne(Tuple1(3)))
    println(tailLong((1, 2, 3, 4, 5, 6, 7)))
    println(size((1, "c", 2L)))
    println(concat((1, 2), Tuple1("d")))
    println(concatLong((1, 2, 3), ("e", "f")))
    println(concatEmpty((4, 5)))
    println(emptyConcat((6, 7)))
    println(array((8, "g")).mkString(","))
    println(emptyArray(EmptyTuple).length)
  }
}
