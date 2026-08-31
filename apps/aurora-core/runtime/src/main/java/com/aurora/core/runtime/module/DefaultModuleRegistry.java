package com.aurora.core.runtime.module;

import com.aurora.core.api.AuroraServices;
import com.aurora.core.api.AuroraVersion;
import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleContext;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.module.AuroraModuleRegistry;
import com.aurora.core.api.module.AuroraModuleRegistrationException;
import com.aurora.core.runtime.AuroraLog;
import com.aurora.core.runtime.event.DefaultEventBus;
import com.aurora.core.runtime.event.ScopedEventBus;
import com.aurora.core.runtime.ui.DefaultSettingsRegistry;
import com.aurora.core.runtime.ui.ScopedSettingsRegistry;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

public final class DefaultModuleRegistry implements AuroraModuleRegistry {
    private final AuroraServices services;
    private final DefaultSettingsRegistry settings;
    private final DefaultEventBus events;
    private final AuroraLog log;
    private final Map<String, RegisteredModule> modules = new LinkedHashMap<String, RegisteredModule>();

    public DefaultModuleRegistry(AuroraServices services, DefaultSettingsRegistry settings,
                                 DefaultEventBus events, AuroraLog log) {
        this.services = services;
        this.settings = settings;
        this.events = events;
        this.log = log;
    }

    @Override public synchronized void register(final AuroraModule module) throws AuroraModuleRegistrationException {
        if (module == null) throw new AuroraModuleRegistrationException("An Aurora module tried to register a null implementation.");
        final AuroraModuleMetadata metadata;
        try {
            metadata = module.metadata();
        } catch (Throwable error) {
            throw new AuroraModuleRegistrationException("An Aurora module has invalid metadata.", error);
        }
        if (metadata == null) throw new AuroraModuleRegistrationException("An Aurora module has no metadata.");
        if (modules.containsKey(metadata.id())) {
            throw new AuroraModuleRegistrationException("Aurora module '" + metadata.id() + "' is already registered.");
        }
        if (AuroraVersion.parse(services.coreVersion()).compareTo(metadata.minimumCoreVersion()) < 0) {
            throw new AuroraModuleRegistrationException(metadata.name() + " " + metadata.version()
                + " requires Aurora Core " + metadata.minimumCoreVersion() + " or newer.");
        }

        final ScopedEventBus scopedEvents = new ScopedEventBus(events);
        try {
            module.initialize(new AuroraModuleContext() {
                @Override public AuroraServices services() { return services; }
                @Override public String moduleId() { return metadata.id(); }
            });
            module.registerSettings(new ScopedSettingsRegistry(metadata.id(), settings));
            module.registerEvents(scopedEvents);
            modules.put(metadata.id(), new RegisteredModule(module, metadata, scopedEvents));
            log.info(metadata.name() + " " + metadata.version() + " registered.");
        } catch (Throwable error) {
            settings.removeOwner(metadata.id());
            scopedEvents.close();
            try { module.shutdown(); } catch (Throwable ignored) { }
            throw new AuroraModuleRegistrationException(
                "Aurora module '" + metadata.name() + "' failed to initialize; other modules remain available.", error);
        }
    }

    @Override public synchronized Optional<AuroraModuleMetadata> find(String id) {
        RegisteredModule module = modules.get(id);
        return Optional.ofNullable(module == null ? null : module.metadata);
    }

    @Override public synchronized List<AuroraModuleMetadata> modules() {
        List<AuroraModuleMetadata> result = new ArrayList<AuroraModuleMetadata>();
        for (RegisteredModule module : modules.values()) result.add(module.metadata);
        Collections.sort(result, Comparator.comparing(AuroraModuleMetadata::name));
        return Collections.unmodifiableList(result);
    }

    @Override public synchronized boolean isInstalled(String id) { return modules.containsKey(id); }

    public synchronized void shutdown() {
        List<RegisteredModule> reverse = new ArrayList<RegisteredModule>(modules.values());
        Collections.reverse(reverse);
        for (RegisteredModule registered : reverse) {
            registered.events.close();
            try { registered.module.shutdown(); }
            catch (Throwable error) { log.error("Aurora module shutdown failed: " + registered.metadata.id(), error); }
        }
        modules.clear();
    }

    private static final class RegisteredModule {
        private final AuroraModule module;
        private final AuroraModuleMetadata metadata;
        private final ScopedEventBus events;
        private RegisteredModule(AuroraModule module, AuroraModuleMetadata metadata, ScopedEventBus events) {
            this.module = module;
            this.metadata = metadata;
            this.events = events;
        }
    }
}
