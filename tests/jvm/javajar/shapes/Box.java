package shapes;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.util.function.Function;

public class Box<T> {
    public static final String NAME = "box";
    public static int created = 0;

    private final T value;

    public Box(T value) {
        this.value = value;
        created++;
    }

    public T get() {
        return value;
    }

    public <U> Box<U> map(Function<T, U> f) {
        return new Box<>(f.apply(value));
    }

    public static <T> Box<T> of(T value) {
        return new Box<>(value);
    }

    public static int sum(int... xs) {
        int s = 0;
        for (int x : xs) s += x;
        return s;
    }

    public static class Pair {
        public final int left;
        public int right;

        public Pair(int left, int right) {
            this.left = left;
            this.right = right;
        }

        public int total() {
            return left + right;
        }
    }

    protected static int sprot() {
        return 7;
    }

    public class Inner {
        public T value() {
            return value;
        }
    }

    public Inner inner() {
        return new Inner();
    }

    protected static class Hidden {
        public String name() {
            return "hidden";
        }
    }

    public interface Shape {
        double area();

        static Shape square(double side) {
            return () -> side * side;
        }
    }

    public static class Crate extends Shelf {}

    @Retention(RetentionPolicy.RUNTIME)
    public @interface Tag {
        String value();
        int level() default 1;
        Class<?> kind() default Object.class;
        String[] tags() default {};
        ElementType where() default ElementType.TYPE;
    }
}

class Shelf {
    public int shelf() {
        return 8;
    }
}

class Counter {
    static final String PREFIX = "count ";
    int count = 0;

    String next() {
        count++;
        return PREFIX + count;
    }
}
