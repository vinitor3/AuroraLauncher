export type LaunchSettings = {
  schemaVersion: 1;
  memoryMode: "manual" | "automatic";
  minMemoryMb: number | null;
  maxMemoryMb: number;
  gcMode: "runtime" | "recommended";
  jvmArgs: string[];
};

export type LaunchSettingsOverview = {
  settings: LaunchSettings;
  totalMemoryMb: number | null;
  modCount: number;
  recommendedMemoryMb: number | null;
};

// Existing instances keep the previous heap and the JVM's collector by default.
export function defaultLaunchSettings(): LaunchSettings {
  return {
    schemaVersion: 1,
    memoryMode: "manual",
    minMemoryMb: null,
    maxMemoryMb: 4096,
    gcMode: "runtime",
    jvmArgs: [],
  };
}
