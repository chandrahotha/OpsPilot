/**
 * Typed bridge to the Pilot engine.
 *
 * Every value comes from a Tauri command that delegates to the Rust engine, so
 * the GUI never invents state. Errors are surfaced as readable messages instead
 * of being swallowed.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  DatabaseOutcome,
  DiagnosticsReport,
  DockerOutcome,
  LogEntry,
  ProcessSnapshot,
  ScanResult,
  ServiceStatus,
  StartupPlan,
  SystemHealth,
} from 'ops-pilot-shared';

/** Convert an unknown command rejection into a readable message */
export function toMessage(error: unknown): string {
  if (typeof error === 'string') {
    return error;
  }

  if (error instanceof Error) {
    return error.message;
  }

  return 'Unknown error';
}

/** Scan a project directory (defaults to the process working directory) */
export function detectProject(path?: string): Promise<ScanResult> {
  return invoke<ScanResult>('detect_project', { path: path ?? null });
}

/** Observed state of every service the project declares */
export function getStatus(path?: string): Promise<ServiceStatus[]> {
  return invoke<ServiceStatus[]>('get_status', { path: path ?? null });
}

/** Run the deterministic diagnostics for a project */
export function runDiagnostics(path?: string): Promise<DiagnosticsReport> {
  return invoke<DiagnosticsReport>('run_diagnostics', { path: path ?? null });
}

/** Start a project service (process lifecycle - phase 4) */
export function startProject(path: string | undefined, service: string): Promise<string> {
  return invoke<string>('start_project', { path: path ?? null, service });
}

/** Stop a project service (process lifecycle - phase 4) */
export function stopProject(service: string): Promise<string> {
  return invoke<string>('stop_project', { service });
}

/**
 * Force-stop the external process holding a service's port.
 * The GUI confirms first: this terminates a process Pilot did not start.
 */
export function stopExternalService(path: string | undefined, service: string): Promise<string> {
  return invoke<string>('stop_external_service', { path: path ?? null, service });
}

/** Restart a project service (process lifecycle - phase 4) */
export function restartProject(service: string): Promise<string> {
  return invoke<string>('restart_project', { service });
}

/** Open a directory picker to select a project folder */
export function selectDirectory(): Promise<string | null> {
  return invoke<string | null>('select_directory');
}

/** Startup plan: what Pilot can start, plus warnings for what it cannot */
export function getStartupPlan(path?: string): Promise<StartupPlan> {
  return invoke<StartupPlan>('get_startup_plan', { path: path ?? null });
}

/** All processes currently tracked by Pilot */
export function listProcesses(): Promise<ProcessSnapshot[]> {
  return invoke<ProcessSnapshot[]>('list_processes');
}

/** Captured stdout/stderr lines for a service started by Pilot */
export function getServiceLogs(service: string, limit?: number): Promise<LogEntry[]> {
  return invoke<LogEntry[]>('get_service_logs', { service, limit: limit ?? null });
}

/** Docker daemon status and CLI version */
export function dockerStatus(): Promise<DockerOutcome> {
  return invoke<DockerOutcome>('docker_status');
}

/** All Docker containers visible to the daemon */
export function listDockerContainers(): Promise<DockerOutcome> {
  return invoke<DockerOutcome>('list_docker_containers');
}

/** Docker container action: start | stop | restart | logs | rebuild */
export function dockerContainerAction(name: string, action: string): Promise<DockerOutcome> {
  return invoke<DockerOutcome>('docker_container_action', { name, action });
}

/** Compose stack action for the current project: up | down */
export function dockerCompose(path: string | undefined, action: string): Promise<DockerOutcome> {
  return invoke<DockerOutcome>('docker_compose', { path: path ?? null, action });
}

/**
 * Database operation: status | migrate | seed | reset | backup | restore.
 * Destructive operations return needsConfirmation unless confirmed=true.
 */
export function databaseOperation(
  path: string | undefined,
  operation: string,
  confirmed: boolean,
): Promise<DatabaseOutcome> {
  return invoke<DatabaseOutcome>('database_operation', { path: path ?? null, operation, confirmed });
}

/** OpsPilot subsystem health + project execution readiness */
export function systemHealth(path?: string): Promise<SystemHealth> {
  return invoke<SystemHealth>('system_health', { path: path ?? null });
}

/** Start everything the project requires (compose first, then plan steps) */
export function startAllProject(path: string | undefined): Promise<string> {
  return invoke<string>('start_all_project', { path: path ?? null });
}

/** Stop everything Pilot started for the project */
export function stopAllProject(path: string | undefined): Promise<string> {
  return invoke<string>('stop_all_project', { path: path ?? null });
}

/** Force-terminate everything Pilot started for the project (stuck processes) */
export function killAllProject(path: string | undefined): Promise<string> {
  return invoke<string>('kill_all_project', { path: path ?? null });
}

/** Run a declared project script as a tracked process (label script-<name>) */
export function runScript(path: string | undefined, name: string): Promise<string> {
  return invoke<string>('run_script', { path: path ?? null, name });
}

/** Open the running frontend in the default browser */
export function openFrontend(path: string | undefined): Promise<string> {
  return invoke<string>('open_frontend', { path: path ?? null });
}

/** Human-readable one-line summary of a Docker outcome */
export function describeDockerOutcome(outcome: DockerOutcome): string {
  if ('status' in outcome) {
    return outcome.status.available
      ? `Docker daemon is running (${outcome.status.version ?? 'unknown version'})`
      : 'Docker daemon is not running';
  }
  if ('containers' in outcome) {
    return `${outcome.containers.length} container(s) found`;
  }
  if ('started' in outcome) {
    return `Started: ${outcome.started.join(', ') || 'done'}`;
  }
  if ('stopped' in outcome) {
    return `Stopped: ${outcome.stopped.join(', ') || 'done'}`;
  }
  if ('restarted' in outcome) {
    return `Restarted: ${outcome.restarted.join(', ') || 'done'}`;
  }
  if ('logs' in outcome) {
    return `Logs for ${outcome.logs.container}`;
  }
  if ('rebuilt' in outcome) {
    return `Rebuilt image for ${outcome.rebuilt.image}`;
  }
  if ('unavailable' in outcome) {
    return `Docker unavailable: ${outcome.unavailable}`;
  }
  return `Docker error: ${outcome.error}`;
}

/** Human-readable one-line summary of a database outcome */
export function describeDatabaseOutcome(outcome: DatabaseOutcome): string {
  if ('success' in outcome) {
    return outcome.success.output;
  }
  if ('needsConfirmation' in outcome) {
    return `This operation is destructive (risk: ${outcome.needsConfirmation.risk}) and needs confirmation.`;
  }
  if ('notImplemented' in outcome) {
    return `Not supported: ${outcome.notImplemented.reason}`;
  }
  return `Database error: ${outcome.error.message}`;
}