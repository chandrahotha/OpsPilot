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
  onClearMessage: () => void;
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
  onDockerComposeUp?: () => void;
  onDockerComposeDown?: () => void;
}

export function ProjectDashboard({
  model,
  evidence,
  services,
  report,
  message,
  onClearMessage,
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
  onDockerComposeUp,
  onDockerComposeDown,
}: ProjectDashboardProps) {
  const showDocker = model.docker !== undefined;
  const showDatabase = model.orm !== undefined || model.database !== undefined;
  const hasStartable = (plan?.steps.length ?? 0) > 0;
  const hasFrontend = model.frontend !== undefined;
  const [logFocus, setLogFocus] = useState<LogFocus | null>(null);
  const [activePanel, setActivePanel] = useState<string>('services');
  const [showLogsSidebar, setShowLogsSidebar] = useState<boolean>(true);

  // Flight summary
  const running = services.filter((service) => service.state === 'running');
  const pilotRunning = running.filter((service) => service.pilotStarted).length;
  const externalRunning = running.length - pilotRunning;
  const stoppedCount = services.filter((service) => service.state === 'stopped').length;
  const unknownCount = services.filter((service) => service.state === 'unknown').length;
  const composePresent = model.docker?.compose ?? false;
  const canStopAll = bulkBusy === null && (running.length > 0 || composePresent);

  const nextAction = (() => {
    if (externalRunning > 0) {
      return `${externalRunning} external service${externalRunning > 1 ? 's' : ''} detected on declared ports. You can stop them with Stop External.`;
    }
    if (running.length > 0 && stoppedCount === 0 && unknownCount === 0) {
      return 'All systems operational. All declared engines are running.';
    }
    if (running.length > 0) {
      return `${running.length} of ${services.length} engine(s) running.`;
    }
    if (hasStartable) {
      return 'All engines idle. Click Start All or launch individual engines below.';
    }
    return 'Project scanned. Select any declared command or inspect Docker/Database panels.';
  })();

  const panels = [
    { id: 'services', label: `⚡ Services (${services.length})`, always: true },
    { id: 'plan', label: `🚀 Plan (${plan?.steps.length ?? 0})`, condition: hasStartable },
    { id: 'docker', label: '🐳 Docker', condition: showDocker },
    { id: 'database', label: '🗄️ Database', condition: showDatabase },
    { id: 'scripts', label: `📜 Scripts (${model.commands?.length ?? 0})`, condition: (model.commands?.length ?? 0) > 0 },
    { id: 'capabilities', label: '💡 Capabilities', condition: true },
    { id: 'diagnostics', label: `🔍 Diag${report?.issues ? ` (${report.issues})` : ''}`, condition: report !== null },
    { id: 'health', label: '🩺 Health', condition: true },
  ].filter(p => p.always || p.condition);

  return (
    <div className="dashboard-compact">
      {/* Cockpit Bar - Master Deck Controls */}
      <header className="cockpit-bar-compact">
        <div className="cockpit-left">
          <div className="project-title-row">
            <span className="project-icon">📂</span>
            <span className="project-title">{model.project.name}</span>
          </div>
          <div className="flight-summary">
            <span className="chip chip-running">{running.length} Online</span>
            {externalRunning > 0 && <span className="chip chip-external">{externalRunning} Ext</span>}
            <span className="chip chip-stopped">{stoppedCount} Offline</span>
            {unknownCount > 0 && <span className="chip chip-unknown">{unknownCount} Standby</span>}
          </div>
        </div>

        <div className="cockpit-center">
          <div className="next-action-pill" title={nextAction}>
            <span className="action-pill-icon">ℹ️</span>
            <span className="next-action-compact">{nextAction}</span>
          </div>
        </div>

        <div className="cockpit-right">
          <div className="action-group">
            <button
              className="btn-cockpit btn-start"
              onClick={onStartAll}
              disabled={!hasStartable || bulkBusy !== null}
              title="Start compose stack + all plan steps"
            >
              {bulkBusy ?? '▶ Start All'}
            </button>
            <button
              className="btn-cockpit btn-stop"
              onClick={onStopAll}
              disabled={!canStopAll}
              title="Stop all Pilot processes + compose down"
            >
              {bulkBusy ?? '■ Stop All'}
            </button>
            <button
              className="btn-cockpit btn-danger"
              onClick={onKillAll}
              disabled={bulkBusy !== null}
              title="Force-kill stuck Pilot processes"
            >
              ☠ Kill All
            </button>
            {hasFrontend && (
              <button
                className="btn-cockpit btn-open"
                onClick={onOpenFrontend}
                disabled={frontendUrl === null}
                title={frontendUrl ? `Open ${frontendUrl}` : 'Start frontend first'}
              >
                {frontendUrl ? '🌐 Open App' : '🌐 Frontend'}
              </button>
            )}
            <button className="btn-cockpit btn-diag" onClick={onRunDiagnostics} title="Run diagnostics">
              🔍 Diag
            </button>
            <button className="btn-cockpit btn-rescan" onClick={onRescan} title="Rescan project">
              ⟳ Rescan
            </button>
            <button
              className={`btn-cockpit btn-toggle-logs ${showLogsSidebar ? 'active' : ''}`}
              onClick={() => setShowLogsSidebar(!showLogsSidebar)}
              title={showLogsSidebar ? 'Collapse live logs sidebar' : 'Expand live logs sidebar'}
            >
              {showLogsSidebar ? '◨ Hide Logs' : '◧ Logs'}
            </button>
          </div>
        </div>
      </header>

      {message && (
        <div className="banner-compact">
          <span>{message}</span>
          <button type="button" className="banner-dismiss" onClick={onClearMessage} title="Dismiss">
            ✕
          </button>
        </div>
      )}

      {/* Main Grid - Responsive Cockpit Layout */}
      <div className={`cockpit-grid ${!showLogsSidebar ? 'logs-hidden' : ''}`}>
        {/* Left: Tabbed Panels */}
        <main className="cockpit-main">
          <nav className="panel-tabs" role="tablist">
            {panels.map((panel) => (
              <button
                key={panel.id}
                role="tab"
                aria-selected={activePanel === panel.id}
                className={`panel-tab ${activePanel === panel.id ? 'active' : ''}`}
                onClick={() => setActivePanel(panel.id)}
              >
                {panel.label}
              </button>
            ))}
          </nav>
          <div className="panel-content" role="tabpanel">
            {activePanel === 'services' && (
              <ServiceGrid
                services={services}
                startableKeys={plan?.steps.map((step) => step.service) ?? []}
                busyServices={busyServices}
                commands={model.commands}
                hasDocker={showDocker}
                onStart={onStartProject}
                onStop={onStopProject}
                onStopExternal={onStopExternalProject}
                onRestart={onRestartProject}
                onServiceLogs={(service) => setLogFocus({ key: `proc:${service}`, ts: Date.now() })}
                onDockerComposeUp={onDockerComposeUp}
                onDockerComposeDown={onDockerComposeDown}
              />
            )}
            {activePanel === 'plan' && hasStartable && (
              <StartupPlanPanel plan={plan} busyServices={busyServices} services={services} onStart={onStartProject} />
            )}
            {activePanel === 'docker' && showDocker && (
              <DockerPanel
                projectPath={projectPath}
                projectName={model.project.name}
                hasCompose={model.docker?.compose ?? false}
                onEvent={onEvent}
                onViewLogs={(name) => setLogFocus({ key: `docker:${name}`, ts: Date.now() })}
              />
            )}
            {activePanel === 'database' && showDatabase && (
              <DatabasePanel
                projectPath={projectPath}
                ormType={model.orm?.type}
                databaseType={model.database?.type}
                onEvent={onEvent}
              />
            )}
            {activePanel === 'scripts' && (model.commands?.length ?? 0) > 0 && (
              <ScriptsPanel projectPath={projectPath} commands={model.commands} />
            )}
            {activePanel === 'capabilities' && (
              <Capabilities model={model} evidence={evidence} />
            )}
            {activePanel === 'diagnostics' && report && (
              <DiagnosticsPanel report={report} onRun={onRunDiagnostics} />
            )}
            {activePanel === 'health' && (
              <SystemHealthPanel projectPath={projectPath} />
            )}
          </div>
        </main>

        {/* Right: Logs Panel (Collapsible) */}
        {showLogsSidebar && (
          <aside className="cockpit-logs">
            <LogsPanel events={events} projectName={model.project.name} focus={logFocus} />
          </aside>
        )}
      </div>
    </div>
  );
}