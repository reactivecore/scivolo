abstract class NameKind:
  type ThisInfo <: Info
  class Info { this: ThisInfo =>
    def kind: NameKind = NameKind.this
  }

object SignedName extends NameKind:
  case class SignedInfo(sig: String) extends Info
  type ThisInfo = SignedInfo

class Other extends NameKind:
  class OInfo extends Info
  type ThisInfo = OInfo

@main def laterselfaliases =
  val o = Other()
  println((SignedName.SignedInfo("s").kind eq SignedName, o.OInfo().kind eq o))
