package com.aurora.core.runtime.session;

import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.event.AuroraEvents;
import com.aurora.core.api.session.AuroraSession;
import com.aurora.core.api.session.AuroraSessionService;
import com.aurora.core.api.session.AuroraSessionState;

import java.util.concurrent.atomic.AtomicReference;

public final class DefaultSessionService implements AuroraSessionService {
    private final AtomicReference<AuroraSession> current = new AtomicReference<AuroraSession>(AuroraSession.offline());
    private final AuroraEventBus events;

    public DefaultSessionService(AuroraEventBus events) { this.events = events; }
    @Override public AuroraSession current() { return current.get(); }

    public void update(AuroraSession session) {
        if (session == null) session = AuroraSession.offline();
        AuroraSession previous = current.getAndSet(session);
        if (session.state() == AuroraSessionState.AUTHENTICATED && previous.state() != AuroraSessionState.AUTHENTICATED) {
            events.publish(new AuroraEvents.Login(session));
        } else if (session.state() != AuroraSessionState.AUTHENTICATED && previous.state() == AuroraSessionState.AUTHENTICATED) {
            events.publish(new AuroraEvents.Logout());
        } else {
            events.publish(new AuroraEvents.ProfileChanged(previous, session));
        }
    }
}
