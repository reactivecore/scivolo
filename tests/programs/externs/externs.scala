import scala.scalanative.unsafe.*

@extern object cmath {
  def abs(x: CInt): CInt = extern
  @name("llabs") def magnitude(x: CLongLong): CLongLong = extern
}

@extern trait cmemory {
  @blocking def free(p: Ptr[Byte]): Unit = extern
}

@extern object cmemory extends cmemory

@main def externs(): Unit = {
  println(cmath.abs(-3))
  println(cmath.magnitude(-4L))
  cmemory.free(null)
  println("freed")
}
