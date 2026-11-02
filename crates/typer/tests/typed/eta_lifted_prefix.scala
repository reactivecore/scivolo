object EtaLiftedPrefix {
  def run(hook: Option[Thread]): Unit = hook.foreach(Runtime.getRuntime().addShutdownHook)
}
