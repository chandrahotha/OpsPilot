/**
 * Service grid: the observed state and operational controls of every service
 * the project declares.
 *
 * State comes from the engine, reporting `running` only when the service
 * port is actively accepting connections.
 */

import { useState } from 'react';
import type { CommandInfo, ServiceState, ServiceStatus } from 'ops-pilot-shared';

interface ServiceGridProps {
  services: ServiceStatus[];
  /** Service keys the startup plan can start directly (e.g. frontend, backend, app) */
  startableKeys: string[];
  /** Services with an action currently in flight (service key -> action label) */
  busyServices: Record<string, string>;
  /** Declared commands from project scanner (e.g. package.json scripts) */
  commands?: CommandInfo[];
  /** Whether Docker / Docker Compose is present */
  hasDocker?: boolean;
  onStart: (service: string, command?: string) => void;
  onStop: (service: string) => void;
  onStopExternal: (service: string) => void;
  onRestart: (service: string) => void;
  onServiceLogs: (service: string) => void;
  onDockerComposeUp?: () => void;
  onDockerComposeDown?: () => void;
}

const STATE_BADGE: Record<ServiceState, { label: string; className: string; dotClass: string }> = {
  running: { label: 'ONLINE', className: 'badge-status-online', dotClass: 'dot-online' },
  stopped: { label: 'OFFLINE', className: 'badge-status-offline', dotClass: 'dot-offline' },
  unknown: { label: 'STANDBY', className: 'badge-status-unknown', dotClass: 'dot-unknown' },
};

function getServiceIcon(key: string, label: string): string {
  const lower = (key + ' ' + label).toLowerCase();
  if (lower.includes('docker') || lower.includes('compose')) return '🐳';
  if (lower.includes('database') || lower.includes('db') || lower.includes('postgres') || lower.includes('mysql') || lower.includes('mongo')) return '🗄️';
  if (lower.includes('front') || lower.includes('web') || lower.includes('ui') || lower.includes('client')) return '🌐';
  if (lower.includes('back') || lower.includes('api') || lower.includes('server')) return '⚙️';
  return '📦';
}

export function ServiceGrid({
  services,
  startableKeys,
  busyServices,
  commands = [],
  onStart,
  onStop,
  onStopExternal,
  onRestart,
  onServiceLogs,
  onDockerComposeUp,
  onDockerComposeDown,
}: ServiceGridProps) {
  const [selectedScript, setSelectedScript] = useState<Record<string, string>>({});

  // Words that mark a script as destructive/one-off rather than something
  // that brings a service up — never auto-offer these as a service's Start.
  const DESTRUCTIVE_SCRIPT_HINTS = ['reset', 'drop', 'delete', 'destroy', 'wipe', 'purge', 'remove', 'uninstall', 'clean'];

  // Find any declared run scripts that can start an unassigned service.
  // Matches whole tokens (split on non-alphanumeric characters) rather than
  // raw substrings, so e.g. "serverless-deploy" does not match "backend" via
  // "server", and never offers a destructive script (e.g. "reset-database")
  // as a service's Start action.
  const findMatchingCommand = (serviceKey: string): CommandInfo | undefined => {
    const key = serviceKey.toLowerCase();
    return commands.find((c) => {
      const name = c.name.toLowerCase();
      const tokens = name.split(/[^a-z0-9]+/).filter(Boolean);
      if (tokens.some((t) => DESTRUCTIVE_SCRIPT_HINTS.includes(t))) return false;
      if (key === 'frontend') return tokens.some((t) => ['front', 'frontend', 'web', 'client', 'ui'].includes(t));
      if (key === 'backend') return tokens.some((t) => ['back', 'backend', 'api', 'server'].includes(t));
      return tokens.includes(key);
    });
  };

  if (services.length === 0) {
    return (
      <section className="services-section">
        <div className="section-title-bar">
          <h3>Engine Control Deck</h3>
        </div>
        <div className="empty-services-card">
          <span className="empty-icon">🛰️</span>
          <h4>No Services Currently Detected</h4>
          <p className="hint">
            Pilot scans for frontend, backend, database, and container services.
            Check that this folder has a valid manifest (such as <code>package.json</code>, <code>pyproject.toml</code>, or <code>docker-compose.yml</code>).
          </p>
        </div>
      </section>
    );
  }

  return (
    <section className="services-section">
      <div className="section-title-bar">
        <div className="title-left">
          <h3>Engine Control Deck</h3>
          <span className="services-count-tag">{services.length} Monitored</span>
        </div>

        {/* Quick Scripts Launcher Strip */}
        {commands.length > 0 && (
          <div className="quick-scripts-strip">
            <span className="quick-label">⚡ Quick Run:</span>
            <div className="quick-scripts-list">
              {commands.slice(0, 6).map((cmd) => (
                <button
                  key={cmd.name}
                  type="button"
                  className="btn-quick-script"
                  onClick={() => onStart(`script-${cmd.name}`, cmd.name)}
                  title={`Run \`${cmd.command}\` (${cmd.source})`}
                >
                  <span className="script-icon">▶</span> {cmd.name}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="service-grid">
        {services.map((service) => {
          const busy = busyServices[service.key];
          const isDocker = service.key === 'docker' || service.key.includes('compose');
          const isStartable = startableKeys.includes(service.key);
          const fallbackCommand = !isStartable ? findMatchingCommand(service.key) : undefined;
          const currentScript = selectedScript[service.key] || (fallbackCommand ? fallbackCommand.name : (commands[0]?.name ?? ''));

          // External process detection
          const externalRunning = service.state === 'running' && service.pilotStarted === false;
          const canStopExternal = externalRunning && service.ownerPid != null;

          const isOnline = service.state === 'running';
          const badge = STATE_BADGE[service.state];
          const icon = getServiceIcon(service.key, service.label);

          return (
            <div
              className={`service-card ${isOnline ? 'card-online' : 'card-offline'} ${busy ? 'card-busy' : ''}`}
              key={service.key}
            >
              {/* Card Header */}
              <div className="service-card-header">
                <div className="service-identity">
                  <span className="service-type-icon">{icon}</span>
                  <div className="service-title-wrap">
                    <span className="service-label">{service.label}</span>
                    <span className="service-key-tag">{service.key}</span>
                  </div>
                </div>

                <div className={`status-badge-pill ${badge.className}`}>
                  <span className={`status-beacon ${badge.dotClass}`} />
                  <span className="status-badge-text">
                    {busy ? busy : externalRunning ? 'EXT RUNNING' : badge.label}
                  </span>
                </div>
              </div>

              {/* Telemetry Bar */}
              <div className="telemetry-bar">
                {service.port != null ? (
                  <div className="telemetry-item">
                    <span className="telemetry-label">PORT</span>
                    {isOnline ? (
                      <a
                        href={`http://localhost:${service.port}`}
                        target="_blank"
                        rel="noreferrer"
                        className="telemetry-value-link"
                        title="Open in browser"
                      >
                        {service.port} ↗
                      </a>
                    ) : (
                      <span className="telemetry-value">{service.port}</span>
                    )}
                  </div>
                ) : (
                  <div className="telemetry-item">
                    <span className="telemetry-label">PORT</span>
                    <span className="telemetry-value-dim">—</span>
                  </div>
                )}

                {service.ownerPid != null && (
                  <div className="telemetry-item">
                    <span className="telemetry-label">PID</span>
                    <span className="telemetry-value pid-badge">{service.ownerPid}</span>
                  </div>
                )}

                {service.ownerName && (
                  <div className="telemetry-item">
                    <span className="telemetry-label">PROCESS</span>
                    <span className="telemetry-value process-name">{service.ownerName}</span>
                  </div>
                )}
              </div>

              {/* Status Details */}
              <div className="service-details-row">
                <p className="detail-text" title={service.detail}>
                  {service.detail}
                </p>
              </div>

              {/* Action Deck */}
              <div className="service-actions-deck">
                {/* 1. Docker Compose Card Buttons */}
                {isDocker ? (
                  <div className="actions-cluster">
                    <button
                      type="button"
                      className="btn-engine btn-start-engine"
                      onClick={() => onDockerComposeUp?.()}
                      disabled={busy !== undefined}
                      title="Run docker compose up -d"
                    >
                      ▶ Compose Up
                    </button>
                    <button
                      type="button"
                      className="btn-engine btn-stop-engine"
                      onClick={() => onDockerComposeDown?.()}
                      disabled={busy !== undefined}
                      title="Run docker compose down"
                    >
                      ■ Compose Down
                    </button>
                    <button
                      type="button"
                      className="btn-engine btn-logs-engine"
                      onClick={() => onServiceLogs(service.key)}
                      title="View Docker logs"
                    >
                      📋 Logs
                    </button>
                  </div>
                ) : (
                  /* 2. Standard Service or Custom Service */
                  <div className="actions-cluster">
                    {/* External stop button is ALWAYS visible if external process is holding the port */}
                    {canStopExternal ? (
                      <button
                        type="button"
                        className="btn-engine btn-danger-engine"
                        onClick={() => onStopExternal(service.key)}
                        disabled={busy !== undefined}
                        title={`Force-stop ${service.ownerName ?? 'external process'} (pid ${service.ownerPid}) holding port ${service.port}. It was started outside Pilot.`}
                      >
                        ⚡ Stop External (PID {service.ownerPid})
                      </button>
                    ) : (
                      <>
                        {/* Start Button */}
                        {isStartable || fallbackCommand ? (
                          <button
                            type="button"
                            className={`btn-engine ${busy?.includes('Start') ? 'btn-busy' : 'btn-start-engine'}`}
                            onClick={() => onStart(service.key, isStartable ? undefined : fallbackCommand?.name)}
                            disabled={isOnline || busy !== undefined}
                            title={isStartable ? `Start ${service.label}` : `Start via \`${fallbackCommand?.name}\``}
                          >
                            {busy?.includes('Start') ? 'Starting…' : isStartable ? '▶ Start' : `▶ Start (${fallbackCommand?.name})`}
                          </button>
                        ) : commands.length > 0 ? (
                          <div className="assign-script-wrap">
                            <select
                              className="script-select"
                              value={currentScript}
                              onChange={(e) =>
                                setSelectedScript((prev) => ({ ...prev, [service.key]: e.target.value }))
                              }
                              title="Select script to assign and run"
                            >
                              {commands.map((cmd) => (
                                <option key={cmd.name} value={cmd.name}>
                                  Run {cmd.name} ({cmd.command})
                                </option>
                              ))}
                            </select>
                            <button
                              type="button"
                              className="btn-engine btn-start-engine"
                              onClick={() => onStart(service.key, currentScript)}
                              disabled={isOnline || busy !== undefined || !currentScript}
                              title={`Execute ${currentScript}`}
                            >
                              ▶ Run
                            </button>
                          </div>
                        ) : (
                          <button
                            type="button"
                            className="btn-engine btn-disabled-hint"
                            disabled
                            title="No runnable command declared in package.json or manifests"
                          >
                            No Script
                          </button>
                        )}

                        {/* Stop Button */}
                        <button
                          type="button"
                          className={`btn-engine ${busy?.includes('Stop') ? 'btn-busy' : 'btn-stop-engine'}`}
                          onClick={() => onStop(service.key)}
                          disabled={!isOnline || busy !== undefined || externalRunning}
                          title={
                            externalRunning
                              ? 'Process started outside Pilot — click Stop External instead'
                              : `Stop ${service.label}`
                          }
                        >
                          {busy?.includes('Stop') ? 'Stopping…' : '■ Stop'}
                        </button>

                        {/* Restart Button */}
                        <button
                          type="button"
                          className={`btn-engine ${busy?.includes('Restart') ? 'btn-busy' : 'btn-restart-engine'}`}
                          onClick={() => onRestart(service.key)}
                          disabled={busy !== undefined || externalRunning || !isOnline}
                          title={externalRunning ? 'Only Pilot-started processes can be restarted' : `Restart ${service.label}`}
                        >
                          {busy?.includes('Restart') ? 'Restarting…' : '⟳ Restart'}
                        </button>

                        {/* Logs Button */}
                        <button
                          type="button"
                          className="btn-engine btn-logs-engine"
                          disabled={busy !== undefined}
                          onClick={() => onServiceLogs(service.key)}
                          title={`Inspect ${service.label} logs in telemetry viewer`}
                        >
                          📋 Logs
                        </button>
                      </>
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}