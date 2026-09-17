import type { CurseForgeUpdateFile } from "../../lib/edge";

export type UpdateContentType = "mod" | "shader" | "resourcepack";

export type ContentUpdateStatus =
  | "updateAvailable"
  | "upToDate"
  | "unmanaged"
  | "blocked"
  | "auroraManaged";

export type ContentUpdateFilter = "all" | ContentUpdateStatus;

export type ContentUpdateItem = {
  id?: string;
  filename: string;
  enabled: boolean;
  contentType?: UpdateContentType;
  provider?: "modrinth" | "curseforge";
  projectId?: string;
  title?: string;
  iconUrl?: string;
  currentSha1?: string;
  currentVersionId?: string;
  currentVersionNumber?: string;
  installedVersionId?: string;
  installedVersionName?: string;
  targetVersionId?: string;
  targetVersionNumber?: string;
  targetVersionName?: string;
  targetFilename?: string;
  targetFile?: CurseForgeUpdateFile;
  status: ContentUpdateStatus;
  reasonCode?: string;
  reason?: string;
};

export type ContentUpdateScanResult = {
  minecraftVersion?: string;
  loader?: string;
  contentType?: UpdateContentType;
  items: ContentUpdateItem[];
  updateCount?: number;
  unmanagedCount?: number;
  blockedCount?: number;
  lastBackupAvailable?: boolean;
  rollbackAvailable?: boolean;
};

export type ContentUpdateScanView = ContentUpdateScanResult & {
  scannedAt: string;
  error?: string;
  warning?: string;
};

export type ContentUpdateSummary = {
  transactionId?: string;
  updatedFiles: Array<{
    previousFilename: string;
    filename: string;
    projectId: string;
    previousVersionNumber: string;
    versionNumber: string;
  }>;
  installedDependencies?: number;
  backupAvailable?: boolean;
  rollbackAvailable?: boolean;
  message?: string;
};

export type ContentRollbackSummary = {
  transactionId?: string;
  restoredFiles: Array<string | {
    replacedFilename: string;
    restoredFilename: string;
    projectId?: string;
  }>;
};

export type ModpackUpdateSummary = {
  transactionId?: string;
  previousVersionNumber?: string;
  versionNumber?: string;
  updatedFileCount: number;
  rollbackAvailable: boolean;
  preservedExtras?: number;
  message?: string;
};

export type ModpackUpdate = {
  status: ContentUpdateStatus;
  source?: "modrinth" | "curseforge";
  provider?: "modrinth" | "curseforge";
  projectId?: string;
  name?: string;
  currentFileId?: string;
  currentVersionId?: string;
  currentVersionNumber?: string;
  installedVersionName?: string;
  targetVersionId?: string;
  targetVersionNumber?: string;
  targetVersionName?: string;
  minecraftVersion?: string;
  loader?: string;
  targetFile?: CurseForgeUpdateFile;
  reasonCode?: string;
  reason?: string;
};

export type UpdateReviewRequest =
  | { kind: "content"; items: ContentUpdateItem[] }
  | { kind: "modpack"; modpack: ModpackUpdate }
  | { kind: "rollback" };

export function currentVersionLabel(item: ContentUpdateItem) {
  return item.currentVersionNumber
    ?? item.installedVersionName
    ?? item.currentVersionId
    ?? item.installedVersionId
    ?? "versão instalada";
}

export function targetVersionLabel(item: ContentUpdateItem) {
  return item.targetVersionNumber
    ?? item.targetVersionName
    ?? item.targetVersionId
    ?? item.targetFilename
    ?? "nova versão";
}

export function modpackCurrentVersionLabel(modpack: ModpackUpdate) {
  return modpack.currentVersionNumber
    ?? modpack.installedVersionName
    ?? modpack.currentVersionId
    ?? "versão instalada";
}

export function modpackTargetVersionLabel(modpack: ModpackUpdate) {
  return modpack.targetVersionNumber
    ?? modpack.targetVersionName
    ?? modpack.targetVersionId
    ?? "nova versão";
}
