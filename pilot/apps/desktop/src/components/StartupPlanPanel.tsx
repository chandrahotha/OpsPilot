/**
 * Startup plan panel: what Pilot can start, and what it cannot (with reasons).
 *
 * The plan comes from the engine, so the GUI never invents commands.
 * Every step has a Start button wired to the same lifecycle as ServiceGrid.
 */

import type { ServiceStatus, StartupPlan } from 'ops-pilot-shared';

interface StartupPlanPanelProps {
  plan: StartupPlan | null;
  busyServices: Record<string, string>;
  services: ServiceStatus[];
  onStart: (service: string) => void;
}

export function StartupPlanPanel({ plan, busyServices, services, onStart }: StartupPlanPanelProps) {
  const runningKeys = new Set(
    services.filter((service) => service.state === 'running').map((service) => service.key),
  );
  if (plan === null) {
    return null;
  }

  return (
    <section className="startup-plan">
      <h3>Startup plan</h3>
      {plan.steps.length === 0 ? (
        <p className="hint">Nothing can be started automatically for this project.</p>
      ) : (
        <ul className="plan-list">
          {plan.steps.map((step) => (
            <li className="plan-step" key={step.service}>
              <div className="plan-step-main">
                <span className="plan-service">{step.service}</span>
                <span className="plan-command">{step.command}</span>
              </div>
              <div className="plan-step-sub">
                <span className="plan-description">{step.description}</span>
                <span className="plan-cwd">{step.workingDirectory}</span>
              </div>
              <button
                type="button"
                className={busyServices[step.service] ? 'btn-busy' : 'btn-start'}
                disabled={busyServices[step.service] !== undefined || runningKeys.has(step.service)}
                title={runningKeys.has(step.service) ? `${step.service} is already running` : `Start ${step.service}`}
                onClick={() => onStart(step.service)}
              >
                {busyServices[step.service] ?? (runningKeys.has(step.service) ? 'Running ✓' : `Start ${step.service}`)}
              </button>
            </li>
          ))}
        </ul>
      )}
      {plan.warnings.length > 0 && (
        <>
          <h4 className="subsection-title">Notes</h4>
          <ul className="evidence-list">
            {plan.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
