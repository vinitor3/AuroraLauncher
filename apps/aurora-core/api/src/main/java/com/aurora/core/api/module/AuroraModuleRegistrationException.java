package com.aurora.core.api.module;

public final class AuroraModuleRegistrationException extends Exception {
    private final String userMessage;

    public AuroraModuleRegistrationException(String userMessage) {
        super(userMessage);
        this.userMessage = userMessage;
    }

    public AuroraModuleRegistrationException(String userMessage, Throwable cause) {
        super(userMessage, cause);
        this.userMessage = userMessage;
    }

    public String userMessage() { return userMessage; }
}
