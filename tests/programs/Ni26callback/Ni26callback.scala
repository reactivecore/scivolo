import scala.scalanative.unsafe.*

object Ni26callback {
  def twice(x: CInt): CInt = x * 2

  def main(args: Array[String]): Unit = {
    val inc: CFuncPtr1[CInt, CInt] = (x: CInt) => x + 1
    println(inc(41))
    val dbl: CFuncPtr1[CInt, CInt] = (x: CInt) => twice(x)
    println(dbl(21))
    val sum: CFuncPtr2[CLongLong, CInt, CLongLong] = (a: CLongLong, b: CInt) => a + b
    println(sum(40L, 2))
    val hello: CFuncPtr0[Unit] = () => println("hello")
    hello()
    val first: CFuncPtr1[CString, CChar] = (s: CString) => !s
    println(first(c"xyz"))
  }
}
