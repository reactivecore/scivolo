class C {
  def m = this.zork
}
object O { val abcde = 1; class Inner }

object NotAMember {
  type MyC = C
  val c: MyC = new C
  val a = c.qqqq
  val b = O.wwww
  val d = 1.xxxx
  val e = List(1, 2).yyyy
  val f = Some("s").get.length.vvvv
  val g = c.uuuu(1, "x")
  val h = c tttt 1
  val i: O.Ssss = ???
  val j = { class L; (new L).rrrr }
  val k: Any = 1
  val l = k.pppp
}
