package com.aurora.core.runtime;

public final class AuroraLog {
    public enum Level { DEBUG, INFO, WARN, ERROR }

    private final AuroraCorePlatform platform;

    AuroraLog(AuroraCorePlatform platform) {
        this.platform = platform;
    }

    public void info(String message) { write(Level.INFO, message, null); }
    public void warn(String message) { write(Level.WARN, message, null); }
    public void error(String message, Throwable error) { write(Level.ERROR, message, error); }

    public void write(Level level, String message, Throwable error) {
        String safe = message == null ? "Unexpected Aurora Core error" : message.replace('\n', ' ').replace('\r', ' ');
        platform.log(level, "[Aurora Core] " + safe, error);
    }
}
