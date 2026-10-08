object MissingParamTypeArgs {
  val m = List(1).fold((a, b) => a + b)
  val n = m + undefinedName
  val o = m.foo(undefinedName)
  val p = m(undefinedName)
  val q = m + { val z: String = 1; z }
  val r = m.foo((x, y) => x)
  val s = List(1).fold((a, b) => a.foo(undefinedName))
  val t = List(1).fold((a, b) => { val v: String = a; v })
  val u = List(1).fold((a, b) => a.length + undefinedName)
}
