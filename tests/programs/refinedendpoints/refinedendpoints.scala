trait Show[T] { def show(t: T): String }

final class Tagged[+T](val tag: Int, val value: T)

trait Endpoint {
  type Req
  type Resp
  implicit val respShow: Show[Resp]
}

object Endpoints {
  type Of[T] = Endpoint { type Req = Tagged[T] }
}
import Endpoints.Of

class Core {
  def call(ep: Endpoint)(req: ep.Req)(resp: ep.Resp)(implicit s: Show[ep.Resp]): String =
    s"${req.getClass.getName} ${s.show(resp)}"
}

class Mux(core: Core) {
  def call[T](ep: Of[T], tag: Int)(req: T)(resp: ep.Resp): String =
    core.call(ep)(new Tagged(tag, req))(resp)
}

trait Message { type Msg }

trait Bound {
  val endpoint: Message
  val exec: endpoint.Msg => Unit
}

class Attach {
  var bound: Bound = null
  def attach(ep: Message)(ex: ep.Msg => Unit): Unit =
    bound = new Bound {
      val endpoint: ep.type = ep
      val exec = ex
    }
}

object refinedendpoints {
  def main(args: Array[String]): Unit = {
    val ep = new Endpoint {
      type Req = Tagged[Int]
      type Resp = String
      implicit val respShow: Show[Resp] = s => s"resp $s"
    }
    println(new Mux(new Core).call[Int](ep, 1)(2)("r"))

    val a = new Attach
    a.attach(new Message { type Msg = Int })(i => println(i + 1))
    println(a.bound != null)
  }
}
