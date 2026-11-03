class Resource(val name: String) {
  def toURI: java.net.URI = new java.net.URI("file", "", "/" + name, null)
  def toURL: java.net.URL = toURI.toURL
}

@main def jdkurls(): Unit = println(new Resource("a/b").toURI)
