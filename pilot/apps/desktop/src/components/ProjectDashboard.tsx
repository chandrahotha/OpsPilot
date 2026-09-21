/**
 * Project dashboard: everything the detected project supports, and nothing else.
 *
 * Sections are rendered from the normalized project model, so two different
 * projects produce two different dashboards (phase 3).
 */

import { useState } from 'react';
import type { DiagnosticsReport, ProjectModel, ServiceStatus, StartupPlan } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import { Capabilities } from './Capabilities';
import { DatabasePanel } from './DatabasePanel';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { DockerPanel } from './DockerPanel';
import { LogsPanel } from './LogsPanel';
import type { LogFocus } from './LogsPanel';
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
  onStopExternalProject: (service: string) => void;
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
  onStopExternalProject,
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
  const [logFocus, setLogFocus] = useState<LogFocus | null>(null);

  // Flight summary: what is running, and — critically — whose engines they are.
  const running = services.filter((service) => service.state === 'running');
  const pilotRunning = running.filter((service) => service.pilotStarted).length;
  const externalRunning = running.length - pilotRunning;
  const stoppedCount = services.filter((service) => service.state === 'stopped').length;
  const unknownCount = services.filter((service) => service.state === 'unknown').length;
  const composePresent = model.docker?.compose ?? false;
  const canStopAll = bulkBusy === null && (running.length > 0 || composePresent);

  // Next action: the cockpit tells the pilot what to do instead of leaving
  // them to walk around the aircraft looking for controls.
  const nextAction = (() => {
    if (externalRunning > 0) {
      return `${externalRunning} service${externalRunning > 1 ? 's are' : ' is'} running outside Pilot — stop ${externalRunning > 1 ? 'them' : 'it'} from ${externalRunning > 1 ? 'their' : 'its'} card before starting here.`;
    }
    if (running.length > 0 && stoppedCount === 0 && unknownCount === 0) {
      return 'All engines running.';
    }
    if (running.length > 0) {
      return `${running.length} of ${services.length} engine(s) running.`;
    }
    if (hasStartable) {
      return 'All engines stopped. Press Start all, or start a single service below.';
    }
    return 'No startable services — check the plan notes below.';
  })();

  return (
    <div className="dashboard">
      <header>
        <h2>{model.project.name}</h2>
        <p>{model.project.path || '—'}</p>
      </header>

      <div className="dashboard-main">
        <section className="cockpit-bar">
          <div className="flight-summary">
            <span className="chip chip-running">{running.length} running</span>
            {externalRunning > 0 && (
              <span className="chip chip-external">{externalRunning} external</span>
            )}
            <span className="chip chip-stopped">{stoppedCount} stopped</span>
            {unknownCount > 0 && <span className="chip chip-unknown">{unknownCount} unknown</span>}
          </div>
          <p className="next-action">{nextAction}</p>
          <div className="action-buttons">
            <button
              type="button"
              className="btn-start"
              onClick={onStartAll}
              disabled={!hasStartable || bulkBusy !== null}
              title={hasStartable ? 'Start the compose stack, then every plan step, in order' : 'No startable services were detected'}
            >
              {bulkBusy ?? 'Start all'}
            </button>
            <button
              type="button"
              className="btn-stop"
              onClick={onStopAll}
              disabled={!canStopAll}
              title={canStopAll ? 'Stop everything Pilot started for this project, then bring the compose stack down' : 'Nothing is running that Pilot can stop'}
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
            <button type="button" onClick={onRunDiagnostics} title="Run the deterministic diagnostics checks">
              Diagnostics
            </button>
            <button type="button" onClick={onRescan} title="Re-scan the project directory">
              Rescan
            </button>
          </div>

          {message && <pre className="banner banner-pre">{message}</pre>}
        </section>

        <ServiceGrid
          services={services}
          startableKeys={plan?.steps.map((step) => step.service) ?? []}
          busyServices={busyServices}
          frontendUrl={frontendUrl}
          onStart={onStartProject}
          onStop={onStopProject}
          onStopExternal={onStopExternalProject}
          onRestart={onRestartProject}
          onOpenFrontend={onOpenFrontend}
          onServiceLogs={(service) => setLogFocus({ key: `proc:${service}`, ts: Date.now() })}
        />

        <StartupPlanPanel plan={plan} busyServices={busyServices} onStart={onStartProject} />

        {showDocker && (
          <DockerPanel
            projectPath={projectPath}
            projectName={model.project.name}
            hasCompose={model.docker?.compose ?? false}
            onEvent={onEvent}
            onViewLogs={(name) => setLogFocus({ key: `docker:${name}`, ts: Date.now() })}
          />
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
        <LogsPanel events={events} projectName={model.project.name} focus={logFocus} />
      </aside>
    </div>
  );
}