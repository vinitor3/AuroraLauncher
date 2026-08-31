package com.aurora.core.api.session;

import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.UUID;

/** Public session projection. It deliberately contains no access or refresh token. */
public final class AuroraSession {
    private final String auroraUserId;
    private final UUID minecraftUuid;
    private final String username;
    private final AuroraSessionState state;
    private final Set<String> scopes;

    public AuroraSession(String auroraUserId, UUID minecraftUuid, String username,
                         AuroraSessionState state, Set<String> scopes) {
        this.auroraUserId = auroraUserId == null ? "" : auroraUserId;
        this.minecraftUuid = minecraftUuid;
        this.username = username == null ? "" : username;
        this.state = state == null ? AuroraSessionState.OFFLINE : state;
        this.scopes = Collections.unmodifiableSet(new LinkedHashSet<String>(
            scopes == null ? Collections.<String>emptySet() : scopes));
    }

    public static AuroraSession offline() {
        return new AuroraSession("", null, "", AuroraSessionState.OFFLINE, Collections.<String>emptySet());
    }

    public String auroraUserId() { return auroraUserId; }
    public UUID minecraftUuid() { return minecraftUuid; }
    public String username() { return username; }
    public AuroraSessionState state() { return state; }
    public Set<String> scopes() { return scopes; }
    public boolean hasScope(String scope) { return scopes.contains(scope); }
}
