package com.aurora.core.runtime;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.AuroraServices;
import com.aurora.core.api.config.AuroraConfigManager;
import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.ipc.AuroraIpc;
import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.module.AuroraModuleRegistry;
import com.aurora.core.api.module.AuroraModuleRegistrationException;
import com.aurora.core.api.session.AuroraSessionService;
import com.aurora.core.api.ui.AuroraPageOpener;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.core.runtime.config.JsonConfigManager;
import com.aurora.core.runtime.event.DefaultEventBus;
import com.aurora.core.runtime.ipc.LauncherIpcClient;
import com.aurora.core.runtime.module.DefaultModuleRegistry;
import com.aurora.core.runtime.session.DefaultSessionService;
import com.aurora.core.runtime.ui.DefaultSettingsRegistry;

import java.nio.file.Path;
import java.util.ServiceLoader;
import java.util.concurrent.atomic.AtomicBoolean;

public final class AuroraCoreRuntime implements AuroraServices, AutoCloseable {
    public static final String VERSION = "1.0.0";
    private static volatile AuroraCoreRuntime instance;

    private final AuroraCorePlatform platform;
    private final AuroraLog log;
    private final DefaultEventBus events;
    private final DefaultSettingsRegistry settings;
    private final DefaultSessionService sessions;
    private final JsonConfigManager configs;
    private final DefaultModuleRegistry modules;
    private final LauncherIpcClient ipc;
    private final AtomicBoolean closed = new AtomicBoolean(false);

    private AuroraCoreRuntime(AuroraCorePlatform platform) {
        this.platform = platform;
        this.log = new AuroraLog(platform);
        this.events = new DefaultEventBus(log);
        this.settings = new DefaultSettingsRegistry(events);
        this.sessions = new DefaultSessionService(events);
        Path configRoot = platform.gameDirectory().resolve(".aurora").resolve("config");
        this.configs = new JsonConfigManager(configRoot, events, log);
        this.modules = new DefaultModuleRegistry(this, settings, events, log);
        this.ipc = new LauncherIpcClient(platform, events, sessions, log);
    }

    public static synchronized AuroraCoreRuntime start(AuroraCorePlatform platform) {
        if (platform == null) throw new IllegalArgumentException("platform");
        if (instance != null) return instance;
        AuroraCoreRuntime runtime = new AuroraCoreRuntime(platform);
        instance = runtime;
        Aurora.install(runtime);
        runtime.registerBuiltInModule();
        runtime.discoverModules();
        runtime.ipc.connectIfConfigured();
        runtime.log.info("Aurora Core " + VERSION + " loaded for "
            + platform.minecraftVersion() + " / " + platform.loader() + ".");
        return runtime;
    }

    public static AuroraCoreRuntime instance() {
        AuroraCoreRuntime current = instance;
        if (current == null) throw new IllegalStateException("Aurora Core is not initialized");
        return current;
    }

    public AuroraCorePlatform platform() { return platform; }
    public AuroraLog log() { return log; }

    private void registerBuiltInModule() {
        try {
            modules.register(new AuroraModule() {
                @Override public AuroraModuleMetadata metadata() {
                    return new AuroraModuleMetadata("aurora_core", "Geral", VERSION, "aurora", VERSION);
                }

                @Override public void registerSettings(AuroraSettingsRegistry registry) {
                    registry.register(new AuroraSettingsPage(
                        "aurora_general", "aurora_core", "Geral",
                        "Sessão, integração com o Launcher e módulos instalados.",
                        "settings", 0, new AuroraPageOpener() {
                            @Override public void open(AuroraUiContext context) {
                                context.openAuroraHome();
                            }
                        }));
                }
            });
        } catch (AuroraModuleRegistrationException error) {
            log.error(error.userMessage(), error);
        }
    }

    private void discoverModules() {
        try {
            for (AuroraModule module : ServiceLoader.load(AuroraModule.class)) {
                try {
                    modules.register(module);
                } catch (AuroraModuleRegistrationException error) {
                    log.error(error.userMessage(), error.getCause());
                }
            }
        } catch (Throwable error) {
            log.error("Module discovery failed; Aurora Core will continue with available modules.", error);
        }
    }

    @Override public String coreVersion() { return VERSION; }
    @Override public AuroraModuleRegistry modules() { return modules; }
    @Override public AuroraSettingsRegistry settings() { return settings; }
    @Override public AuroraEventBus events() { return events; }
    @Override public AuroraSessionService sessions() { return sessions; }
    @Override public AuroraIpc ipc() { return ipc; }
    @Override public AuroraConfigManager configs() { return configs; }

    @Override public void close() {
        if (!closed.compareAndSet(false, true)) return;
        ipc.close();
        modules.shutdown();
    }
}
