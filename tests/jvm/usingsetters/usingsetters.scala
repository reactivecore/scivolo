class Context

class Denot:
  private var myAnnotations: List[String] = List("a")
  final def annotations(using Context): List[String] = myAnnotations
  final def annotations_=(annots: List[String]): Unit = myAnnotations = annots
  final def transformAnnotations(f: String => String)(using Context): Unit =
    annotations = annotations.map(f)
  final def addAnnotation(annot: String): Unit =
    annotations = annot :: myAnnotations

@main def usingsetters =
  given Context = Context()
  val d = Denot()
  d.transformAnnotations(_ + "b")
  d.addAnnotation("c")
  println(d.annotations)
