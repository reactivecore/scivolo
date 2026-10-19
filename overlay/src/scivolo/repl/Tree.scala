// Derived from PPrint (MIT, see licenses/pprint.txt); translated and changed:
//   pprint/src/pprint/Walker.scala
// Copyright (c) 2019 Li Haoyi.
package scivolo.repl

/** A value as pretty-printable text, its children computed lazily. */
enum Tree {

  /** `prefix(a, b)` */
  case Apply(prefix: String, body: Iterator[Tree])

  /** `lhs op rhs` */
  case Infix(lhs: Tree, op: String, rhs: Tree)

  /** Text never broken across lines. */
  case Literal(body: String)

  /** `key = value` */
  case KeyValue(key: String, value: Tree)

  /** Text computed while rendering. */
  case Lazy(body: () => String)
}
