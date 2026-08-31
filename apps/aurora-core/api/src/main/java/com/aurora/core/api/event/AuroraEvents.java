package com.aurora.core.api.event;

import com.aurora.core.api.session.AuroraSession;

public final class AuroraEvents {
    private AuroraEvents() { }

    public static final class Login implements AuroraEvent {
        private final AuroraSession session;
        public Login(AuroraSession session) { this.session = session; }
        public AuroraSession session() { return session; }
    }

    public static final class Logout implements AuroraEvent { }

    public static final class ProfileChanged implements AuroraEvent {
        private final AuroraSession previous;
        private final AuroraSession current;
        public ProfileChanged(AuroraSession previous, AuroraSession current) {
            this.previous = previous;
            this.current = current;
        }
        public AuroraSession previous() { return previous; }
        public AuroraSession current() { return current; }
    }

    public static final class SkinChanged implements AuroraEvent {
        private final String minecraftUuid;
        public SkinChanged(String minecraftUuid) { this.minecraftUuid = minecraftUuid; }
        public String minecraftUuid() { return minecraftUuid; }
    }

    public static final class LauncherConnected implements AuroraEvent { }
    public static final class LauncherDisconnected implements AuroraEvent { }

    public static final class SettingsChanged implements AuroraEvent {
        private final String owner;
        public SettingsChanged(String owner) { this.owner = owner; }
        public String owner() { return owner; }
    }
}
