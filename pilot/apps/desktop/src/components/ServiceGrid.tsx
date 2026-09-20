/**
 * Service grid: the observed state of every service the project declares.
 *
 * State comes from the engine, which reports `running` only when the service
 * port is really accepting connections.
 */

import type { ServiceState, ServiceStatus } from 'ops-pilot-shared';

interface ServiceGridProps {
  services: ServiceStatus[];
  onStart: (service: string) => void;
  onStop: (service: string) => void;
  onRestart: (service: string) => void;
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

export function ServiceGrid({ services, onStart, onStop, onRestart }: ServiceGridProps) {
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
        {services.map((service) => (
          <div className="service-card" key={service.key}>
            <div className="service-header">
              <span>{service.label}</span>
              <span className="service-state">
                <span className={STATE_INDICATOR[service.state]} />
                <span className="state-label">{STATE_LABEL[service.state]}</span>
              </span>
            </div>
            <div className="service-details">
              <p>{service.port === undefined ? 'Port: —' : `Port: ${service.port}`}</p>
              <p className="detail">{service.detail}</p>
            </div>
            <div className="service-actions">
              <button 
                type="button" 
                onClick={() => onStart(service.label)}
                disabled={service.state === 'running'}
              >
                Start
              </button>
              <button 
                type="button" 
                onClick={() => onStop(service.label)}
                disabled={service.state === 'stopped'}
              >
                Stop
              </button>
              <button 
                type="button" 
                onClick={() => onRestart(service.label)}
              >
                Restart
              </button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}