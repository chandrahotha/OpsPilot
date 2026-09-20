/**
 * Typed bridge to the Pilot engine.
 *
 * Every value comes from a Tauri command that delegates to the Rust engine, so
 * the GUI never invents state. Errors are surfaced as readable messages instead
 * of being swallowed.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  DiagnosticsReport,
  ScanResult,
  ServiceStatus,
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

/** Restart a project service (process lifecycle - phase 4) */
export function restartProject(service: string): Promise<string> {
  return invoke<string>('restart_project', { service });
}

/** Open a directory picker to select a project folder */
export function selectDirectory(): Promise<string | null> {
  return invoke<string | null>('select_directory');
}