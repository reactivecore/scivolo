import language.`3.3`

type E[X] = X match {
  case List[t] => t
  case Option[t] => t
}

class Box[T](val value: T)

def elem[A <: List[Int]](a: A): E[A] = a.head.asInstanceOf[E[A]]
def viaBound[A >: List[Int] <: List[Int]]: E[A] = 1.asInstanceOf[E[A]]
def id[B](b: B): B = b
def mk[A](a: A): Box[E[A]] = new Box(a.asInstanceOf[List[Any]].head.asInstanceOf[E[A]])

object Test {
  def a1 = elem(List(1))
  def a2 = id(elem(List(2)))
  def a3 = viaBound
  def a4 = id(viaBound)
  def a5 = mk(List(5)).value

  def main(args: Array[String]): Unit = {
    val sum: Int = a1 + a2 + a3 + a4 + a5
    println(sum)
  }
}
