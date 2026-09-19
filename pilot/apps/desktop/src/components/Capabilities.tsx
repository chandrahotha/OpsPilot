/**
 * Detected capabilities: what the scanner found, with the evidence behind it.
 *
 * The evidence list is intentionally visible: Pilot shows why it believes
 * something was detected instead of only claiming it.
 */

import type { ProjectModel } from 'ops-pilot-shared';

interface CapabilitiesProps {
  model: ProjectModel;
  evidence: string[];
}

interface CapabilityRow {
  label: string;
  value: string;
}

function capabilityRows(model: ProjectModel): CapabilityRow[] {
  const rows: CapabilityRow[] = [];

  if (model.frontend) {
    rows.push({
      label: 'Frontend',
      value: `${model.frontend.framework} · port ${model.frontend.port}`,
    });
  }

  if (model.backend) {
    rows.push({
      label: 'Backend',
      value: `${model.backend.framework} · port ${model.backend.port}`,
    });
  }

  if (model.database) {
    rows.push({
      label: 'Database',
      value: `${model.database.type} · port ${model.database.port}`,
    });
  }

  if (model.orm) {
    rows.push({ label: 'ORM', value: model.orm.type });
  }

  if (model.docker) {
    rows.push({
      label: 'Docker',
      value: model.docker.compose ? 'detected · compose' : 'detected',
    });
  }

  if (model.environment) {
    const files = [
      model.environment.envFile ? '.env' : null,
      model.environment.envExample ? '.env.example' : null,
      model.environment.envLocal ? '.env.local' : null,
    ].filter((file): file is string => file !== null);

    rows.push({ label: 'Environment', value: files.join(', ') });
  }

  return rows;
}

export function Capabilities({ model, evidence }: CapabilitiesProps) {
  const rows = capabilityRows(model);

  if (rows.length === 0 && evidence.length === 0) {
    return null;
  }

  return (
    <section className="capabilities">
      <h3>Detected capabilities</h3>

      {rows.length > 0 && (
        <dl className="capability-list">
          {rows.map((row) => (
            <div className="capability" key={row.label}>
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      )}

      {evidence.length > 0 && (
        <>
          <h4 className="subsection-title">Evidence</h4>
          <ul className="evidence-list">
            {evidence.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}