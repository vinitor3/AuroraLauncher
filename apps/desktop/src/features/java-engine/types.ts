export type LaunchSettings = {
  schemaVersion: 1;
  memoryRevision?: string | null;
  memoryMode: "manual" | "automatic" | "launcher";
  minMemoryMb: number | null;
  maxMemoryMb: number;
  gcMode: "runtime" | "recommended";
  jvmArgs: string[];
};

export type GlobalMemoryOverview = { maxMemoryMb: number; totalMemoryMb: number | null; memoryRevision?: string | null };

export type LaunchSettingsOverview = {
  settings: LaunchSettings;
  globalMaxMemoryMb: number;
  totalMemoryMb: number | null;
  modCount: number;
  recommendedMemoryMb: number | null;
};

export function defaultLaunchSettings(globalMaxMemoryMb = 4096): LaunchSettings {
  return {
    schemaVersion: 1,
    memoryMode: "launcher",
    minMemoryMb: null,
    maxMemoryMb: globalMaxMemoryMb,
    gcMode: "runtime",
    jvmArgs: [],
  };
}
