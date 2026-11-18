class Outer[T] { class In(val t: T) }
class Box[T](var v: T)
class Sub[A](init: Outer[A]#In) extends Box[Outer[A]#In](init)

object WildcardSelection {
  def bad(s: Sub[?]): Box[Outer[?]#In] = s
}
