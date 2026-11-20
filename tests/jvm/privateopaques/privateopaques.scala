private opaque type TracedEventId <: String = String
private object TracedEventId:
  val Empty: TracedEventId = ""
  def apply(s: String): TracedEventId = s

@main def privateopaques = println(TracedEventId("x") + TracedEventId.Empty.length)
