//> using scala 3.9.0
// Extractors with implicit clauses before and after the scrutinee's, on constant scrutinees.

case class UECfg(n: Int)
case class UETag(s: String)

object UELead {
  def unapply(using c: UECfg)(x: Int): Option[Int] = Some(x + c.n)
}
object UETwo {
  def unapply(x: Int)(using c: UECfg)(using t: UETag): Option[String] = Some(s"${t.s}${x * c.n}")
}
object UEBoth {
  def unapply(using c: UECfg)(x: String)(using t: UETag): Option[(String, Int)] =
    if x.nonEmpty then Some((t.s + x, c.n)) else None
}
object UELeadTwo {
  def unapply(using c: UECfg)(using t: UETag)(x: Int): Boolean = x == c.n && t.s == "t"
}
object UESeq {
  def unapplySeq(x: Int)(using c: UECfg)(using t: UETag): Option[Seq[Int]] = Some(Seq.fill(c.n)(x))
}

object usingextractors {
  def main(args: Array[String]): Unit = {
    given UECfg = UECfg(10)
    given UETag = UETag("t")
    5 match { case UELead(y) => println(y) }
    3 match { case UETwo(s) => println(s) }
    "ab" match { case UEBoth(s, n) => println(s"$s $n") }
    "" match { case UEBoth(s, n) => println("wrong"); case _ => println("empty") }
    10 match { case UELeadTwo() => println("leadtwo"); case _ => println("no") }
    11 match { case UELeadTwo() => println("wrong"); case _ => println("no") }
    2 match { case UESeq(a, rest*) => println(s"$a ${rest.size}") }
    val UETwo(v) = 4: @unchecked
    println(v)
  }
}
