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
  onRestart: (service: string) => void;
  onOpenFrontend: () => void;
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

export function ServiceGrid({ services, startableKeys, busyServices, frontendUrl, onStart, onStop, onRestart, onOpenFrontend }: ServiceGridProps) {
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
                <button
                  type="button"
                  className={busy === 'Stopping…' ? 'btn-busy' : 'btn-stop'}
                  onClick={() => onStop(service.key)}
                  disabled={service.state === 'stopped' || busy !== undefined}
                  title={`Stop ${service.label}`}
                >
                  {busy === 'Stopping…' ? 'Stopping…' : 'Stop'}
                </button>
                <button
                  type="button"
                  className={busy === 'Restarting…' ? 'btn-busy' : 'btn-restart'}
                  onClick={() => onRestart(service.key)}
                  disabled={busy !== undefined}
                  title={`Restart ${service.label}`}
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