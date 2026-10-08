object MissingParamTypeUses {
  val m = List(1).fold((a, b) => a + b)
  val n = m + 1
  val s = List(1, 2).fold((a, b) => a).toString
}
