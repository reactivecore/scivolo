trait Seqs[A] {
  def ++[B >: A](xs: List[B]): List[B] = xs
  def pair[B, C](b: B, c: C): (A, B, C) = (null.asInstanceOf[A], b, c)
}

trait Lists[A] extends Seqs[A] {
  def head: A
  override def ++[B >: A](xs: List[B]): List[B] = xs match {
    case Nil => Nil
    case _ => head :: super.++(xs)
  }
  override def pair[B, C](b: B, c: C): (A, B, C) = super.pair(b, c).copy(_1 = head)
}

class One(val head: Int) extends Lists[Int]

@main def superpoly(): Unit =
  println(new One(1) ++ List(2, 3))
  println(new One(1).pair("b", 'c'))
  println(new Lists[String] { def head = "h" } ++ List("t"))
