public class Greeter {
    public static String greet(String greeting, String name) {
        return greeting + ", " + name;
    }

    public String twice(int times, String word) {
        return String.join(" ", java.util.Collections.nCopies(times, word));
    }

    public enum Color { RED, GREEN }
}
