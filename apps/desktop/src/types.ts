export type ActionKind = "task" | "long_running";
export type TerminalMode = "captured" | "interactive" | "hidden";

export interface Action {
  id: string;
  label: string;
  icon?: string;
  program?: string;
  args: string[];
  operation?: string;
  working_directory?: string;
  kind: ActionKind;
  terminal: TerminalMode;
  concurrency: "allow" | "reject" | "replace_same_action";
  timeout_seconds?: number;
  confirm: boolean;
}

export interface ProjectManifest {
  schema_version: number;
  id: string;
  name: string;
  description: string;
  project: { working_directory: string };
  logs: { sources: string[]; open_with_deebugee: boolean };
  artifacts: { paths: string[] };
  actions: Action[];
}

export interface ProjectSummary {
  root: string;
  manifest: ProjectManifest;
}

export interface RunRecord {
  id: string;
  projectId: string;
  actionId: string;
  actionLabel: string;
  startedAt: string;
  finishedAt?: string;
  durationMs?: number;
  status: string;
  exitCode?: number;
  transcriptPath?: string;
}

export interface Artifact {
  name: string;
  path: string;
  size: number;
  modifiedMs: number;
}

export interface RunEvent {
  runId: string;
  projectId: string;
  actionId: string;
  kind: "started" | "output" | "finished";
  stream?: "stdout" | "stderr" | "system";
  line?: string;
  status?: string;
  exitCode?: number;
  durationMs?: number;
}
