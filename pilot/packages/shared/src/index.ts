/**
 * Shared types for the Pilot monorepo.
 *
 * These types mirror the JSON contract of the Rust engine:
 *  - `ScanResult` / `ProjectModel`     <- crates/core
 *  - `ServiceStatus`                   <- pilot/apps/desktop/src-tauri/src/status.rs
 *  - `DiagnosticsReport`               <- crates/diagnostics
 *  - `StartupPlan`                     <- crates/process-manager (via Tauri StartupPlanResponse)
 *  - `DockerOutcome` / `ContainerStatus` <- crates/docker
 *  - `DatabaseOutcome`                 <- crates/database
 *  - `LogEntry`                        <- crates/process-manager
 *  - `SystemHealth`                    <- Tauri system_health command
 */

/** Frontend framework information */
export interface FrontendInfo {
  framework: string;
  port: number;
}

/** Backend framework information */
export interface BackendInfo {
  framework: string;
  port: number;
}

/** Database information */
export interface DatabaseInfo {
  type: string;
  port: number;
}

/** ORM information */
export interface OrmInfo {
  type: string;
}

/** Docker detection information */
export interface DockerInfo {
  detected: boolean;
  compose: boolean;
}

/** Environment file detection */
export interface EnvironmentInfo {
  envFile: boolean;
  envExample: boolean;
  envLocal: boolean;
}

/** Project identity */
export interface ProjectInfo {
  name: string;
  path: string;
}

/** A declared project command (e.g. package.json scripts) */
export interface CommandInfo {
  name: string;
  command: string;
  source: string;
}

/** Normalized project model produced by the scanner */
export interface ProjectModel {
  project: ProjectInfo;
  frontend?: FrontendInfo;
  backend?: BackendInfo;
  database?: DatabaseInfo;
  orm?: OrmInfo;
  docker?: DockerInfo;
  environment?: EnvironmentInfo;
  commands?: CommandInfo[];
}

/** Result of scanning a project directory */
export interface ScanResult {
  detected: boolean;
  model?: ProjectModel;
  evidence: string[];
}

/** Observed state of a service */
export type ServiceState = 'running' | 'stopped' | 'unknown';

/** Status of a single detected service */
export interface ServiceStatus {
  key: string;
  label: string;
  port?: number | null;
  state: ServiceState;
  detail: string;
  /** PID of the process holding the service port, when known */
  ownerPid?: number | null;
  /** Name of the process holding the service port, when known */
  ownerName?: string | null;
  /** True when Pilot's process manager started the running process; false means the port is held by something Pilot did not start */
  pilotStarted?: boolean;
}

/** A single diagnostic check result */
export interface DiagnosticCheck {
  id: string;
  name: string;
  passed: boolean;
  problem?: string;
  evidence?: string;
  cause?: string;
  recommendedAction?: string;
}

/** Result of a diagnostics run */
export interface DiagnosticsReport {
  checks: DiagnosticCheck[];
  issues: number;
}

/** One ordered step of a startup sequence */
export interface StartupStep {
  service: string;
  description: string;
  command: string;
  workingDirectory: string;
}

/** A startup sequence the user can review before Pilot runs anything */
export interface StartupPlan {
  executable: boolean;
  steps: StartupStep[];
  warnings: string[];
}

/** Status of a single Docker container (serde externally-tagged via DockerOutcome) */
export interface ContainerStatus {
  name: string;
  id: string;
  image: string;
  running: boolean;
  status: string;
  ports: string[];
  composeProject?: string;
  composeService?: string;
}

/**
 * Outcome of a Docker operation.
 * Mirrors the serde externally-tagged JSON: exactly one key is present.
 */
export type DockerOutcome =
  | { status: { available: boolean; version?: string | null } }
  | { containers: ContainerStatus[] }
  | { started: string[] }
  | { stopped: string[] }
  | { restarted: string[] }
  | { logs: { container: string; output: string } }
  | { rebuilt: { image: string } }
  | { unavailable: string }
  | { error: string };

/** Risk level of a database operation */
export type RiskLevel = 'low' | 'medium' | 'high';

/**
 * Outcome of a database operation.
 * Mirrors the serde externally-tagged JSON: exactly one key is present.
 */
export type DatabaseOutcome =
  | { success: { output: string } }
  | { needsConfirmation: { risk: RiskLevel } }
  | { error: { message: string; output?: string | null } }
  | { notImplemented: { reason: string } };

/** Log stream a captured line was read from */
export type LogStream = 'stdout' | 'stderr' | 'system';

/** One captured process output line */
export interface LogEntry {
  timestampMs: number;
  service: string;
  stream: LogStream;
  message: string;
}

/** Snapshot of a process tracked by Pilot */
export interface ProcessSnapshot {
  label: string;
  command: string;
  workingDirectory: string;
  pid?: number | null;
  state: string;
  detail: string;
  exitCode?: number | null;
  startedAtMs?: number | null;
}

/** One execution-readiness row in the system health report */
export interface ReadinessEntry {
  service: string;
  command: string;
  ready: boolean;
  detail: string;
}

/** OpsPilot subsystem health + project execution readiness */
export interface SystemHealth {
  frontend: boolean;
  tauriBridge: boolean;
  rustBackend: boolean;
  projectScanner: boolean;
  projectDetected: boolean;
  projectName?: string | null;
  projectPath?: string | null;
  processManager: boolean;
  trackedProcesses: number;
  dockerAvailable: boolean;
  dockerVersion?: string | null;
  composeAvailable: boolean;
  readiness: ReadinessEntry[];
  warnings: string[];
}