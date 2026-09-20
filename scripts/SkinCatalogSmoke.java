import com.aurora.skins.catalog.AuroraCatalogProvider;
import com.aurora.skins.storage.AuroraSkinStore;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinPage;
import java.nio.file.Paths;
import java.util.concurrent.TimeUnit;

/** Optional real-network probe. Use an isolated scratch directory, never a player's library. */
public final class SkinCatalogSmoke {
    public static void main(String[] args) throws Exception {
        if (args.length != 1) throw new IllegalArgumentException("Provide a scratch directory");
        AuroraCatalogProvider provider = new AuroraCatalogProvider(new AuroraSkinStore(Paths.get(args[0])));
        SkinPage page = provider.search("", 0, 12).get(25, TimeUnit.SECONDS);
        System.out.println("java=" + System.getProperty("java.version") + " online=" + provider.online()
            + " entries=" + page.entries().size());
        if (!provider.online()) {
            java.lang.reflect.Method fetch = AuroraCatalogProvider.class.getDeclaredMethod("fetchPage", String.class, int.class, int.class);
            fetch.setAccessible(true);
            fetch.invoke(provider, "", 0, 12);
            throw new IllegalStateException("Online catalog fell back to offline");
        }
        for (SkinEntry entry : page.entries()) {
            if (!entry.id().startsWith("mcstat-")) continue;
            SkinEntry preview = provider.preview(entry).get(25, TimeUnit.SECONDS);
            System.out.println("MCStat PNG validated=" + (preview.file() != null));
            break;
        }
        SkinEntry profile = provider.getPlayerProfile("Notch").get(25, TimeUnit.SECONDS);
        System.out.println("Player PNG validated=" + (provider.preview(profile).get(25, TimeUnit.SECONDS).file() != null));
    }
}
