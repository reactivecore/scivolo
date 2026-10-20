// Function values of more than 22 parameters: closures implementing `FunctionXXL`, calls passing
// the arguments as one array, captured values, context functions, a `Unit` result.

object functionsxxl {
  type F23 = (Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int,
    Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, String) => String

  val label: F23 = (a1: Int, a2: Int, a3: Int, a4: Int, a5: Int, a6: Int, a7: Int, a8: Int,
    a9: Int, a10: Int, a11: Int, a12: Int, a13: Int, a14: Int, a15: Int, a16: Int, a17: Int,
    a18: Int, a19: Int, a20: Int, a21: Int, a22: Int, s: String) => s + (a1 + a22)

  def scaled(k: Int): F23 = (a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15,
    a16, a17, a18, a19, a20, a21, a22, s) => s * (a1 * k)

  type C23 = (Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, Int,
    Int, Int, Int, Int, Int, Int, Int, Int, Int, Int, String) ?=> Int

  def sized: C23 = summon[String].length

  def main(args: Array[String]): Unit = {
    println(label(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, "sum "))
    println(scaled(2)(2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, "ab"))
    var total = 0
    val add = (a1: Int, a2: Int, a3: Int, a4: Int, a5: Int, a6: Int, a7: Int, a8: Int, a9: Int,
      a10: Int, a11: Int, a12: Int, a13: Int, a14: Int, a15: Int, a16: Int, a17: Int, a18: Int,
      a19: Int, a20: Int, a21: Int, a22: Int, a23: Int) => total += a1 + a23
    add(1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 41)
    println(total)
    given Int = 3
    given String = "four"
    println(sized)
  }
}
