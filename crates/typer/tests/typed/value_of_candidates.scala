trait Enc[A]
object Enc {
  implicit def encodeLiteralInt[L <: Int](implicit L: ValueOf[L]): Enc[L] = new Enc[L] {}
  implicit val encodeInt: Enc[Int] = new Enc[Int] {}
}
object ValueOfCandidates {
  val e = summon[Enc[Int]]
  val l = summon[Enc[1]]
}
