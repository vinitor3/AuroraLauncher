import { isRetiredSkinSource } from "./skinCatalogSources.ts";

export type LocalSkin = {
  id: string;
  ownerId: string;
  name: string;
  skinModel: "classic" | "slim";
  sourceUrl?: string;
  publicCatalogUrl?: string;
  image?: Blob;
  createdAt: number;
};

type StoredLocalSkin = LocalSkin & { key: string };

const DATABASE_NAME = "aurora-launcher-local";
const DATABASE_VERSION = 1;
const STORE_NAME = "skins";

function openDatabase() {
  return new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    request.onerror = () => reject(request.error ?? new Error("Não foi possível abrir a biblioteca local de skins."));
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORE_NAME)) {
        const store = database.createObjectStore(STORE_NAME, { keyPath: "key" });
        store.createIndex("ownerId", "ownerId", { unique: false });
      }
    };
    request.onsuccess = () => {
      const database = request.result;
      database.onversionchange = () => database.close();
      resolve(database);
    };
  });
}

// A successful request can still be rolled back by a failed transaction. Only
// publish a result once IndexedDB confirms the entire transaction committed.
function transactionRequest<T>(
  mode: IDBTransactionMode,
  operation: (store: IDBObjectStore, result: (value: T) => void, fail: (error: unknown) => void) => void,
) {
  return openDatabase().then((database) => new Promise<T>((resolve, reject) => {
    let transaction: IDBTransaction | undefined;
    let value: T;
    let hasResult = false;
    let settled = false;
    const fail = (error: unknown) => {
      if (settled) return;
      settled = true;
      try { transaction?.abort(); } catch { /* It may already have finished. */ }
      database.close();
      reject(error);
    };
    try {
      transaction = database.transaction(STORE_NAME, mode);
      transaction.onerror = () => fail(transaction?.error ?? new Error("Não foi possível atualizar a biblioteca local."));
      transaction.onabort = () => fail(transaction?.error ?? new Error("A atualização da biblioteca local foi cancelada."));
      transaction.oncomplete = () => {
        database.close();
        if (settled) return;
        settled = true;
        if (hasResult) resolve(value);
        else reject(new Error("A biblioteca local não retornou um resultado."));
      };
      operation(transaction.objectStore(STORE_NAME), (result) => {
        value = result;
        hasResult = true;
      }, fail);
    } catch (error) {
      fail(error);
    }
  }));
}

function observeRequest<T>(request: IDBRequest<T>, success: (value: T) => void, fail: (error: unknown) => void) {
  request.onerror = () => fail(request.error ?? new Error("A biblioteca local não respondeu."));
  request.onsuccess = () => {
    try { success(request.result); } catch (error) { fail(error); }
  };
}

function persistentSourceUrl(sourceUrl?: string) {
  if (!sourceUrl) return undefined;
  try {
    const url = new URL(sourceUrl);
    return url.protocol === "https:" && !url.username && !url.password ? url.href : undefined;
  } catch {
    return undefined;
  }
}

function readLocalSkin(record: StoredLocalSkin): LocalSkin {
  const { key: _key, sourceUrl, ...skin } = record;
  // Legacy object/data URLs only lived for the old renderer session. Ignore
  // them without deleting the record; callers can recover its PNG later.
  const persistentUrl = persistentSourceUrl(sourceUrl);
  return { ...skin, ...(persistentUrl ? { sourceUrl: persistentUrl } : {}) };
}

export async function listLocalSkins(ownerId: string, workerUrl?: string) {
  return transactionRequest<LocalSkin[]>("readonly", (store, result, fail) => {
    observeRequest(store.index("ownerId").getAll(ownerId), (records: StoredLocalSkin[]) => {
      result(records.filter((skin) => !isRetiredSkinSource(skin.sourceUrl, workerUrl))
        .map(readLocalSkin).sort((left, right) => right.createdAt - left.createdAt));
    }, fail);
  });
}

export async function isRetiredLocalSkin(ownerId: string, id: string, workerUrl?: string) {
  return transactionRequest<boolean>("readonly", (store, result, fail) => {
    observeRequest(store.get(`${ownerId}:${id}`), (skin: StoredLocalSkin | undefined) => {
      result(isRetiredSkinSource(skin?.sourceUrl, workerUrl));
    }, fail);
  });
}

export async function saveLocalSkin(input: Omit<LocalSkin, "id" | "createdAt"> & { id?: string }) {
  const id = input.id ?? crypto.randomUUID();
  const key = `${input.ownerId}:${id}`;
  return transactionRequest<LocalSkin>("readwrite", (store, result, fail) => {
    // Read and merge in the same transaction so an upload/recovery cannot
    // overwrite another update with an older URL or remove an existing PNG.
    observeRequest(store.get(key), (stored: StoredLocalSkin | undefined) => {
      const existing = stored ? readLocalSkin(stored) : undefined;
      const sourceUrl = persistentSourceUrl(input.sourceUrl) ?? existing?.sourceUrl;
      const publicCatalogUrl = input.publicCatalogUrl ?? existing?.publicCatalogUrl;
      const image = input.image?.size ? input.image : existing?.image;
      const skin: LocalSkin = {
        id,
        ownerId: input.ownerId,
        name: existing?.name || input.name,
        skinModel: input.skinModel,
        createdAt: existing?.createdAt ?? Date.now(),
        ...(sourceUrl ? { sourceUrl } : {}),
        ...(publicCatalogUrl ? { publicCatalogUrl } : {}),
        ...(image ? { image } : {}),
      };
      observeRequest(store.put({ ...skin, key }), () => result(skin), fail);
    }, fail);
  });
}

export async function updateLocalSkinSource(ownerId: string, id: string, sourceUrl: string) {
  const persistentUrl = persistentSourceUrl(sourceUrl);
  if (!persistentUrl) throw new Error("O endereço permanente da skin deve usar HTTPS.");
  const key = `${ownerId}:${id}`;
  return transactionRequest<LocalSkin | undefined>("readwrite", (store, result, fail) => {
    observeRequest(store.get(key), (stored: StoredLocalSkin | undefined) => {
      if (!stored) {
        result(undefined);
        return;
      }
      const skin = { ...readLocalSkin(stored), sourceUrl: persistentUrl };
      observeRequest(store.put({ ...skin, key }), () => result(skin), fail);
    }, fail);
  });
}

export async function cacheLocalSkinImage(ownerId: string, id: string, image: Blob, sourceUrl?: string) {
  if (!(image instanceof Blob) || !image.size) throw new Error("Não foi possível guardar uma imagem de skin vazia.");
  const key = `${ownerId}:${id}`;
  return transactionRequest<LocalSkin | undefined>("readwrite", (store, result, fail) => {
    observeRequest(store.get(key), (stored: StoredLocalSkin | undefined) => {
      // A background fetch can finish after the user removed the skin. Repair
      // only a still-existing record, never restore something they deleted.
      if (!stored) {
        result(undefined);
        return;
      }
      const existing = readLocalSkin(stored);
      const persistentUrl = persistentSourceUrl(sourceUrl) ?? existing.sourceUrl;
      const skin = { ...existing, image, ...(persistentUrl ? { sourceUrl: persistentUrl } : {}) };
      observeRequest(store.put({ ...skin, key }), () => result(skin), fail);
    }, fail);
  });
}

export async function deleteLocalSkin(ownerId: string, id: string) {
  await transactionRequest<undefined>("readwrite", (store, result, fail) => {
    observeRequest(store.delete(`${ownerId}:${id}`), () => result(undefined), fail);
  });
}
