import language.experimental.captureChecking

class Label[T]

final class Break[T](val label: Label[T]^{}, val value: T)
