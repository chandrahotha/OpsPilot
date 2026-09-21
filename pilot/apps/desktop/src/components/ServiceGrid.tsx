/**
 * Service grid: the observed state of every service the project declares.
 *
 * State comes from the engine, which reports `running` only when the service
 * port is really accepting connections.
 */

import type { ServiceState, ServiceStatus } from 'ops-pilot-shared';

interface ServiceGridProps {
  services: ServiceStatus[];
  /** Service keys the startup plan can actually start (e.g. frontend, backend, app) */
  startableKeys: string[];
  /** Services with an action currently in flight (service key -> action label) */
  busyServices: Record<string, string>;
  /** Frontend URL when the frontend is actually running, otherwise null */
  frontendUrl: string | null;
  onStart: (service: string) => void;
  onStop: (service: string) => void;
  onStopExternal: (service: string) => void;
  onRestart: (service: string) => void;
  onOpenFrontend: () => void;
  onServiceLogs: (service: string) => void;
}

const STATE_INDICATOR: Record<ServiceState, string> = {
  running: 'status-indicator online',
  stopped: 'status-indicator offline',
  unknown: 'status-indicator unknown',
};

const STATE_LABEL: Record<ServiceState, string> = {
  running: 'Running',
  stopped: 'Stopped',
  unknown: 'Not observable yet',
};

/**
 * Hint shown instead of lifecycle buttons for services Pilot cannot start
 * directly (they are operated from the Docker/Database panels instead).
 */
function nonStartableHint(key: string): string {
  if (key === 'docker') {
    return 'Manage containers from the Docker panel below.';
  }
  if (key === 'database') {
    return 'Operate the database from the Database panel below.';
  }
  return 'No start command declared for this service.';
}

export function ServiceGrid({
  services,
  startableKeys,
  busyServices,
  frontendUrl,
  onStart,
  onStop,
  onStopExternal,
  onRestart,
  onOpenFrontend,
  onServiceLogs,
}: ServiceGridProps) {
  if (services.length === 0) {
    return (
      <section className="services">
        <h3>Services</h3>
        <p className="hint">No service was detected in this project.</p>
      </section>
    );
  }

  return (
    <section className="services">
      <h3>Services</h3>
      <div className="service-grid">
        {services.map((service) => {
          const busy = busyServices[service.key];
          // A port is being held by something Pilot did not start: the
          // engine-stop switch must reach it anyway.
          const externalRunning = service.state === 'running' && service.pilotStarted === false;
          const canStopExternal = externalRunning && service.ownerPid != null;
          return (
          <div className="service-card" key={service.key}>
            <div className="service-header">
              <span>{service.label}</span>
              <span className="service-state">
                <span className={STATE_INDICATOR[service.state]} />
                <span className="state-label">{busy ? busy : STATE_LABEL[service.state]}</span>
              </span>
            </div>
            <div className="service-details">
              <p>{service.port == null ? 'Port: —' : `Port: ${service.port}`}</p>
              <p className="detail">{service.detail}</p>
            </div>
            {startableKeys.includes(service.key) ? (
              <div className="service-actions">
                <button
                  type="button"
                  className={busy === 'Starting…' ? 'btn-busy' : 'btn-start'}
                  onClick={() => onStart(service.key)}
                  disabled={service.state === 'running' || busy !== undefined}
                  title={service.state === 'unknown' ? 'Service state is not observable yet' : `Start ${service.label}`}
                >
                  {busy === 'Starting…' ? 'Starting…' : 'Start'}
                </button>
                {canStopExternal ? (
                  <button
                    type="button"
                    className={busy === 'Stopping…' ? 'btn-busy' : 'btn-stop'}
                    onClick={() => onStopExternal(service.key)}
                    disabled={busy !== undefined}
                    title={`Force-stop ${service.ownerName ?? 'the external process'} (pid ${service.ownerPid}) holding port ${service.port}. It was not started by Pilot.`}
                  >
                    {busy === 'Stopping…' ? 'Stopping…' : 'Stop External'}
                  </button>
                ) : (
                  <button
                    type="button"
                    className={busy === 'Stopping…' ? 'btn-busy' : 'btn-stop'}
                    onClick={() => onStop(service.key)}
                    disabled={service.state === 'stopped' || busy !== undefined || externalRunning}
                    title={
                      externalRunning
                        ? 'Running outside Pilot, and the owning process could not be identified — stop it from the terminal you started it in'
                        : `Stop ${service.label}`
                    }
                  >
                    {busy === 'Stopping…' ? 'Stopping…' : 'Stop'}
                  </button>
                )}
                <button
                  type="button"
                  className={busy === 'Restarting…' ? 'btn-busy' : 'btn-restart'}
                  onClick={() => onRestart(service.key)}
                  disabled={busy !== undefined || externalRunning}
                  title={externalRunning ? 'Only services started by Pilot can be restarted' : `Restart ${service.label}`}
                >
                  {busy === 'Restarting…' ? 'Restarting…' : 'Restart'}
                </button>
                {service.key === 'frontend' && frontendUrl !== null && (
                  <button
                    type="button"
                    className="btn-open"
                    onClick={onOpenFrontend}
                    title={`Open ${frontendUrl} in the default browser`}
                  >
                    Open
                  </button>
                )}
                <button
                  type="button"
                  disabled={busy !== undefined}
                  onClick={() => onServiceLogs(service.key)}
                  title={`Show ${service.label} output in Live logs`}
                >
                  Logs
                </button>
              </div>
            ) : (
              <p className="hint">{nonStartableHint(service.key)}</p>
            )}
          </div>
          );
        })}
      </div>
    </section>
  );
}