package com.aurora.skins;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.catalog.AuroraCatalogProvider;
import com.aurora.skins.storage.AuroraSkinStore;
import com.aurora.skins.storage.LocalSkinProvider;
import com.aurora.skins.storage.SkinValidationException;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.CopyOnWriteArrayList;

public final class AuroraSkinsRuntime {
    public interface Listener { void activeSkinChanged(SkinEntry entry); }
    private static volatile AuroraSkinsRuntime instance;
    private final AuroraSkinStore store;
    private final LocalSkinProvider localProvider;
    private final AuroraCatalogProvider catalogProvider;
    private final CopyOnWriteArrayList<Listener> listeners = new CopyOnWriteArrayList<Listener>();
    private volatile SkinEntry active;

    private AuroraSkinsRuntime(AuroraSkinStore store) throws IOException {
        this.store = store;
        this.localProvider = new LocalSkinProvider(store);
        this.catalogProvider = new AuroraCatalogProvider(store);
        this.active = store.active();
    }

    public static synchronized AuroraSkinsRuntime start() throws IOException {
        if (instance != null) return instance;
        String configured = System.getProperty("aurora.skins.root", "").trim();
        Path root = configured.isEmpty()
            ? Paths.get(System.getProperty("user.dir", "."), ".aurora", "skins")
            : Paths.get(configured);
        AuroraSkinsRuntime created = new AuroraSkinsRuntime(new AuroraSkinStore(root));
        instance = created;
        String activeFile = System.getProperty("aurora.profile.skinFile", "").trim();
        if (!activeFile.isEmpty()) {
            try {
                SkinEntry entry = created.store.importSkin(Paths.get(activeFile), "Skin ativa",
                    SkinModel.fromId(System.getProperty("aurora.profile.skinModel", "classic")));
                created.apply(entry);
            } catch (SkinValidationException error) {
                System.err.println("[Aurora Skins] " + error.getMessage());
            }
        }
        return created;
    }

    public static AuroraSkinsRuntime instance() {
        AuroraSkinsRuntime current = instance;
        if (current == null) throw new IllegalStateException("Aurora Skins is not initialized");
        return current;
    }

    public synchronized void apply(SkinEntry entry) throws IOException {
        store.setActive(entry);
        active = entry;
        for (Listener listener : listeners) listener.activeSkinChanged(entry);
        System.out.println("[Aurora Skins] Skin aplicada: " + entry.name());
    }

    public AuroraSkinStore store() { return store; }
    public LocalSkinProvider localProvider() { return localProvider; }
    public AuroraCatalogProvider catalogProvider() { return catalogProvider; }
    public SkinEntry active() { return active; }
    public CompletableFuture<SkinEntry> materialize(SkinEntry entry) {
        if (entry != null && "local".equals(entry.providerId()) && entry.file() != null) {
            return CompletableFuture.completedFuture(entry);
        }
        return catalogProvider.materialize(entry);
    }
    public CompletableFuture<SkinEntry> applyAsync(SkinEntry entry) {
        return materialize(entry).thenApply(installed -> {
            try {
                apply(installed);
                return installed;
            } catch (IOException error) {
                throw new CompletionException(error);
            }
        });
    }
    public void addListener(Listener listener) { if (listener != null) listeners.addIfAbsent(listener); }
    public void removeListener(Listener listener) { listeners.remove(listener); }
}
