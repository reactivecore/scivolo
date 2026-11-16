package settings;

class Builder<T extends Builder<T>> {
    private int attempts = 3;

    @SuppressWarnings("unchecked")
    public T setAttempts(int attempts) {
        this.attempts = attempts;
        return (T) this;
    }

    public int attempts() { return attempts; }
}

public class Settings extends Builder<Settings> {}
