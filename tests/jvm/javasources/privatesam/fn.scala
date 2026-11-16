package hidden

@main def fn(): Unit = {
  val f: Fn = () => 1
  println(f.apply())
}
