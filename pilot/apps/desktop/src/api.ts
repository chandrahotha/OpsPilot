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

/** Start the project (process lifecycle - phase 4) */
export function startProject(): Promise<string> {
  return invoke<string>('start_project');
}

/** Stop the project (process lifecycle - phase 4) */
export function stopProject(): Promise<string> {
  return invoke<string>('stop_project');
}

/** Restart the project (process lifecycle - phase 4) */
export function restartProject(): Promise<string> {
  return invoke<string>('restart_project');
}