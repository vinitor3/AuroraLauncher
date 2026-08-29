export type LocalSkin = {
  id: string;
  ownerId: string;
  name: string;
  skinModel: "classic" | "slim";
  sourceUrl?: string;
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
    request.onsuccess = () => resolve(request.result);
  });
}

function transactionRequest<T>(mode: IDBTransactionMode, operation: (store: IDBObjectStore) => IDBRequest<T>) {
  return openDatabase().then((database) => new Promise<T>((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, mode);
    const request = operation(transaction.objectStore(STORE_NAME));
    request.onerror = () => reject(request.error ?? new Error("A biblioteca local não respondeu."));
    request.onsuccess = () => resolve(request.result);
    transaction.oncomplete = () => database.close();
    transaction.onerror = () => reject(transaction.error ?? new Error("Não foi possível atualizar a biblioteca local."));
  }));
}

export async function listLocalSkins(ownerId: string) {
  const database = await openDatabase();
  return new Promise<LocalSkin[]>((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, "readonly");
    const request = transaction.objectStore(STORE_NAME).index("ownerId").getAll(ownerId);
    request.onerror = () => reject(request.error ?? new Error("Não foi possível carregar as skins locais."));
    request.onsuccess = () => {
      const records = (request.result as StoredLocalSkin[])
        .map(({ key: _key, ...skin }) => skin)
        .sort((left, right) => right.createdAt - left.createdAt);
      resolve(records);
    };
    transaction.oncomplete = () => database.close();
  });
}

export async function saveLocalSkin(input: Omit<LocalSkin, "id" | "createdAt"> & { id?: string }) {
  const skin: LocalSkin = {
    ...input,
    id: input.id ?? crypto.randomUUID(),
    createdAt: Date.now(),
  };
  const record: StoredLocalSkin = { ...skin, key: `${skin.ownerId}:${skin.id}` };
  await transactionRequest("readwrite", (store) => store.put(record));
  return skin;
}

export async function deleteLocalSkin(ownerId: string, id: string) {
  await transactionRequest("readwrite", (store) => store.delete(`${ownerId}:${id}`));
}
