/**
 * Shared types for the Pilot monorepo.
 *
 * These types mirror the JSON contract of the Rust engine:
 *  - `ScanResult` / `ProjectModel`     <- crates/core
 *  - `ServiceStatus`                   <- pilot/apps/desktop/src-tauri/src/status.rs
 *  - `DiagnosticsReport`               <- crates/diagnostics
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

/** Normalized project model produced by the scanner */
export interface ProjectModel {
  project: ProjectInfo;
  frontend?: FrontendInfo;
  backend?: BackendInfo;
  database?: DatabaseInfo;
  orm?: OrmInfo;
  docker?: DockerInfo;
  environment?: EnvironmentInfo;
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
  port?: number;
  state: ServiceState;
  detail: string;
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