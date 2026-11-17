object Test {
  final val Alias = E.A
  val a: E.A.type = Alias
  final val Default = E.DEFAULT
  val b: E.DEFAULT.type = Default
}
