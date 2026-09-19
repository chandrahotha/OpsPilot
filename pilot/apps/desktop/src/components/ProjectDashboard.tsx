/**
 * Project dashboard: everything the detected project supports, and nothing else.
 *
 * Sections are rendered from the normalized project model, so two different
 * projects produce two different dashboards (phase 3).
 */

import type { DiagnosticsReport, ProjectModel, ServiceStatus } from 'ops-pilot-shared';
import { Capabilities } from './Capabilities';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { ServiceGrid } from './ServiceGrid';

interface ProjectDashboardProps {
  model: ProjectModel;
  evidence: string[];
  services: ServiceStatus[];
  report: DiagnosticsReport | null;
  message: string | null;
  onStartProject: () => void;
  onStopProject: () => void;
  onRestartProject: () => void;
  onRunDiagnostics: () => void;
}

export function ProjectDashboard({
  model,
  evidence,
  services,
  report,
  message,
  onStartProject,
  onStopProject,
  onRestartProject,
  onRunDiagnostics,
}: ProjectDashboardProps) {
  return (
    <div className="dashboard">
      <header>
        <h2>{model.project.name}</h2>
        <p>{model.project.path || '—'}</p>
      </header>

      <ServiceGrid services={services} />

      <section className="actions">
        <div className="action-buttons">
          <button type="button" onClick={onStartProject}>
            Start Project
          </button>
          <button type="button" onClick={onStopProject}>
            Stop
          </button>
          <button type="button" onClick={onRestartProject}>
            Restart
          </button>
          <button type="button" onClick={onRunDiagnostics}>
            Diagnostics
          </button>
        </div>

        {message && <p className="banner">{message}</p>}
      </section>

      <Capabilities model={model} evidence={evidence} />

      <DiagnosticsPanel report={report} onRun={onRunDiagnostics} />
    </div>
  );
}