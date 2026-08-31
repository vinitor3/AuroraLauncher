package com.aurora.core.runtime;

import com.aurora.core.api.config.AuroraConfig;
import com.aurora.core.api.event.AuroraEvent;
import com.aurora.core.api.event.AuroraEvents;
import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.module.AuroraModuleRegistrationException;
import com.aurora.core.api.ui.AuroraPageOpener;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.core.api.session.AuroraSession;
import com.aurora.core.api.session.AuroraSessionState;
import com.aurora.core.runtime.session.DefaultSessionService;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CoreRuntimeIntegrationTest {
    @TempDir Path gameDirectory;

    @Test void isolatesFailuresRollsBackModulesAndMigratesConfig() throws Exception {
        TestPlatform platform = new TestPlatform(gameDirectory);
        AuroraCoreRuntime runtime = AuroraCoreRuntime.start(platform);

        AtomicInteger delivered = new AtomicInteger();
        runtime.events().subscribe(TestEvent.class, event -> { throw new IllegalStateException("isolated"); });
        runtime.events().subscribe(TestEvent.class, event -> delivered.incrementAndGet());
        runtime.events().publish(new TestEvent());
        assertEquals(1, delivered.get());
        assertEquals(1, platform.errors.get());

        AtomicInteger logins = new AtomicInteger();
        AtomicInteger logouts = new AtomicInteger();
        AtomicInteger profileChanges = new AtomicInteger();
        runtime.events().subscribe(AuroraEvents.Login.class, event -> logins.incrementAndGet());
        runtime.events().subscribe(AuroraEvents.Logout.class, event -> logouts.incrementAndGet());
        runtime.events().subscribe(AuroraEvents.ProfileChanged.class, event -> profileChanges.incrementAndGet());
        DefaultSessionService sessions = new DefaultSessionService(runtime.events());
        UUID minecraftUuid = UUID.randomUUID();
        sessions.update(new AuroraSession(
            "user-1", minecraftUuid, "AuroraPlayer", AuroraSessionState.AUTHENTICATED,
            Collections.singleton("profile.read")));
        sessions.update(new AuroraSession(
            "user-1", minecraftUuid, "AuroraPlayer2", AuroraSessionState.AUTHENTICATED,
            Collections.singleton("profile.read")));
        sessions.update(AuroraSession.offline());
        assertEquals(1, logins.get());
        assertEquals(1, profileChanges.get());
        assertEquals(1, logouts.get());

        AtomicBoolean failedListenerCalled = new AtomicBoolean();
        AtomicBoolean shutdownCalled = new AtomicBoolean();
        AuroraModule broken = new AuroraModule() {
            @Override public AuroraModuleMetadata metadata() {
                return new AuroraModuleMetadata("broken_module", "Broken module", "1.0.0", "warning", "1.0.0");
            }

            @Override public void registerSettings(AuroraSettingsRegistry settings) {
                settings.register(new AuroraSettingsPage(
                    "broken_page", "broken_module", "Broken", "", "warning", 10,
                    new AuroraPageOpener() {
                        @Override public void open(AuroraUiContext context) { }
                    }));
            }

            @Override public void registerEvents(com.aurora.core.api.event.AuroraEventBus events) {
                events.subscribe(TestEvent.class, event -> failedListenerCalled.set(true));
                throw new IllegalStateException("registration failed");
            }

            @Override public void shutdown() { shutdownCalled.set(true); }
        };
        assertThrows(AuroraModuleRegistrationException.class, () -> runtime.modules().register(broken));
        runtime.events().publish(new TestEvent());
        assertFalse(runtime.settings().find("broken_page").isPresent());
        assertFalse(runtime.modules().isInstalled("broken_module"));
        assertFalse(failedListenerCalled.get());
        assertTrue(shutdownCalled.get());

        AuroraConfig first = runtime.configs().open(
            "migration_test", 1, Collections.<String, Object>singletonMap("enabled", true), null);
        first.set("oldName", "Aurora");
        first.save();
        AuroraConfig migrated = runtime.configs().open(
            "migration_test", 2, Collections.<String, Object>emptyMap(),
            (from, to, values) -> {
                values.put("newName", values.remove("oldName"));
                return values;
            });
        assertEquals("Aurora", migrated.getString("newName", ""));
        assertTrue(Files.exists(gameDirectory.resolve(".aurora/config/migration_test.json.v1.backup.json")));

        runtime.close();
    }

    private static final class TestEvent implements AuroraEvent { }

    private static final class TestPlatform implements AuroraCorePlatform {
        private final Path gameDirectory;
        private final AtomicInteger errors = new AtomicInteger();
        private TestPlatform(Path gameDirectory) { this.gameDirectory = gameDirectory; }
        @Override public Path gameDirectory() { return gameDirectory; }
        @Override public String minecraftVersion() { return "test"; }
        @Override public String loader() { return "test"; }
        @Override public void runOnClient(Runnable action) { action.run(); }
        @Override public void openAuroraSettings(Object parentScreen) { }
        @Override public void log(AuroraLog.Level level, String message, Throwable error) {
            if (level == AuroraLog.Level.ERROR) errors.incrementAndGet();
        }
    }
}
