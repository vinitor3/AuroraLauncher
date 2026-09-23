import type { DoctorAdvice } from "../../../../../shared/doctorAdvice";

export type DoctorExplanation = (report: DoctorReport, signal: AbortSignal) => Promise<DoctorAdvice>;

export type DoctorRun = {
  instanceId: string;
  runId: string;
  outcome: "preparing" | "running" | "success" | "failed" | "cancelled" | "unknown";
};

export type DoctorIssue = {
  id: string;
  title: string;
  severity: "error" | "warning" | "info";
  explanation: string;
  evidence: Array<{ source: string; summary: string }>;
  confidence: "high" | "medium";
  kind: "fact" | "hypothesis";
  nextStep: string;
};

export type DoctorReport = {
  schemaVersion: number;
  status: "issues" | "no_issues" | "no_logs" | "incomplete";
  summary: string;
  instance: {
    minecraftVersion: string;
    loader: string;
    javaMajor: number | null;
    requiredJavaMajor: number;
    maxMemoryMb: number;
  };
  lastRun: {
    outcome: DoctorRun["outcome"];
    exitCode: number | null;
    runId: string | null;
    startedAtMs: number | null;
    endedAtMs: number | null;
  } | null;
  issues: DoctorIssue[];
  logs: Array<{
    source: string;
    association: "last_run" | "historical" | "unassociated";
    bytesRead: number;
    truncated: boolean;
  }>;
  limitations: string[];
  sanitizedJson: string;
};
