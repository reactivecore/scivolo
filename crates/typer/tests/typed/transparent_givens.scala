trait A
class B extends A:
  def b = "b"

transparent inline given Int = 1
transparent inline given tc: A = B()

val one: 1 = summon[Int]
val bb: B = summon[A]
val s = summon[A].b
def show(using a: A): A = a
val shown = show
