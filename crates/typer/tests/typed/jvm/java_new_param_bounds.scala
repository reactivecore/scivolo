import java.util.concurrent.atomic.AtomicReference

object JavaNewParamBounds {
  final case class Pointer[A](value: A, subscribers: Int)
  val ref: AtomicReference[Pointer[String]] = new AtomicReference(Pointer(null, 0))
}
