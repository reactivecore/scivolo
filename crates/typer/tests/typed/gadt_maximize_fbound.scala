trait Document[Doc <: Document[Doc]]
sealed trait Conversion[Doc, V]
case class C[Doc <: Document[Doc]]() extends Conversion[Doc, Doc]

trait T[U] {
  def x: T[? <: U]
}

object T {
  def unapply[U](t: T[U]): Option[T[? <: U]] = Some(t.x)
}

class Box[A](val a: A)
object Box {
  def unapply[A](b: Box[A]): Some[A] = Some(b.a)
}

class GadtMaximizeFbound {
  def conv[Doc <: Document[Doc], V](c: Conversion[Doc, V]): Unit = c match {
    case C() | C() => ()
  }

  def nested[W](t: T[W]): Unit = t match {
    case T(T(u)) =>
      val v: T[? <: W] = u
      ()
  }

  def boxed[A](b: Box[? <: A]): Unit = b match {
    case Box(a) =>
      val x: A = a
      ()
  }
}
