package mixed;

/** A sealed interface, its subclasses in its unit and in another one. */
public sealed interface Animal permits Animal.Cat, Dog {
    String sound();

    final class Cat implements Animal {
        public String sound() { return "meow"; }
    }
}
