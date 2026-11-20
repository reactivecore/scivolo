class P:
  def quietly(action: => Unit, disposal: => Unit) =
    try action
    finally disposal
