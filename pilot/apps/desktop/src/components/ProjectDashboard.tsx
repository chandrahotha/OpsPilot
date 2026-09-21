/**
 * Project dashboard: everything the detected project supports, and nothing else.
 *
 * Sections are rendered from the normalized project model, so two different
 * projects produce two different dashboards (phase 3).
 */

import type { DiagnosticsReport, ProjectModel, ServiceStatus, StartupPlan } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import { Capabilities } from './Capabilities';
import { DatabasePanel } from './DatabasePanel';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { DockerPanel } from './DockerPanel';
import { LogsPanel } from './LogsPanel';
import { ScriptsPanel } from './ScriptsPanel';
import { ServiceGrid } from './ServiceGrid';
import { StartupPlanPanel } from './StartupPlanPanel';
import { SystemHealthPanel } from './SystemHealthPanel';

interface ProjectDashboardProps {
  model: ProjectModel;
  evidence: string[];
  services: ServiceStatus[];
  report: DiagnosticsReport | null;
  message: string | null;
  projectPath: string;
  plan: StartupPlan | null;
  busyServices: Record<string, string>;
  bulkBusy: string | null;
  frontendUrl: string | null;
  events: ActivityEvent[];
  onStartProject: (service: string) => void;
  onStopProject: (service: string) => void;
  onRestartProject: (service: string) => void;
  onStartAll: () => void;
  onStopAll: () => void;
  onKillAll: () => void;
  onRescan: () => void;
  onOpenFrontend: () => void;
  onEvent: (message: string, kind: ActivityEvent['kind']) => void;
  onRunDiagnostics: () => void;
}

export function ProjectDashboard({
  model,
  evidence,
  services,
  report,
  message,
  projectPath,
  plan,
  busyServices,
  bulkBusy,
  frontendUrl,
  events,
  onStartProject,
  onStopProject,
  onRestartProject,
  onStartAll,
  onStopAll,
  onKillAll,
  onRescan,
  onOpenFrontend,
  onEvent,
  onRunDiagnostics,
}: ProjectDashboardProps) {
  const showDocker = model.docker !== undefined;
  const showDatabase = model.orm !== undefined || model.database !== undefined;
  const hasStartable = (plan?.steps.length ?? 0) > 0;

  return (
    <div className="dashboard">
      <header>
        <h2>{model.project.name}</h2>
        <p>{model.project.path || '—'}</p>
      </header>

      <div className="dashboard-main">
        <ServiceGrid
          services={services}
          startableKeys={plan?.steps.map((step) => step.service) ?? []}
          busyServices={busyServices}
          frontendUrl={frontendUrl}
          onStart={onStartProject}
          onStop={onStopProject}
          onRestart={onRestartProject}
          onOpenFrontend={onOpenFrontend}
        />

        <StartupPlanPanel plan={plan} busyServices={busyServices} onStart={onStartProject} />

        <section className="actions">
          <div className="action-buttons">
            <button
              type="button"
              className="btn-start"
              onClick={onStartAll}
              disabled={!hasStartable || bulkBusy !== null}
            >
              {bulkBusy ?? 'Start all'}
            </button>
            <button
              type="button"
              className="btn-stop"
              onClick={onStopAll}
              disabled={bulkBusy !== null}
            >
              {bulkBusy ?? 'Stop all'}
            </button>
            <button
              type="button"
              className="btn-danger"
              onClick={onKillAll}
              disabled={bulkBusy !== null}
              title="Force-terminate stuck processes started by Pilot for this project. Only for use when Stop All did not work."
            >
              Kill All Nodes
            </button>
            <button type="button" onClick={onRunDiagnostics}>
              Diagnostics
            </button>
            <button type="button" onClick={onRescan} title="Re-scan the project directory">
              Rescan
            </button>
          </div>

          {message && <pre className="banner banner-pre">{message}</pre>}
        </section>

        {showDocker && (
          <DockerPanel projectPath={projectPath} hasCompose={model.docker?.compose ?? false} onEvent={onEvent} />
        )}

        {showDatabase && (
          <DatabasePanel
            projectPath={projectPath}
            ormType={model.orm?.type}
            databaseType={model.database?.type}
            onEvent={onEvent}
          />
        )}

        <ScriptsPanel projectPath={projectPath} commands={model.commands} />

        <Capabilities model={model} evidence={evidence} />

        <DiagnosticsPanel report={report} onRun={onRunDiagnostics} />

        <SystemHealthPanel projectPath={projectPath} />
      </div>

      <aside className="dashboard-side">
        <LogsPanel events={events} />
      </aside>
    </div>
  );
}