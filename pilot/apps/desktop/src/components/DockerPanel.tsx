/**
 * Docker panel: compose stack controls plus per-container operations.
 *
 * Every action delegates to the Docker engine via Tauri commands and reports
 * the real outcome (including "daemon not running") instead of failing silently.
 */

import { useCallback, useEffect, useState } from 'react';
import type { ContainerStatus } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import * as api from '../api';
import { toMessage } from '../api';

interface DockerPanelProps {
  projectPath: string;
  hasCompose: boolean;
  onEvent: (message: string, kind: ActivityEvent['kind']) => void;
}

export function DockerPanel({ projectPath, hasCompose, onEvent }: DockerPanelProps) {
  const [containers, setContainers] = useState<ContainerStatus[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [logsFor, setLogsFor] = useState<string | null>(null);
  const [logsText, setLogsText] = useState<string>('');

  const refresh = useCallback(async () => {
    try {
      const outcome = await api.listDockerContainers();
      if ('containers' in outcome) {
        setContainers(outcome.containers);
        setMessage(null);
      } else {
        setContainers([]);
        setMessage(api.describeDockerOutcome(outcome));
      }
    } catch (err) {
      setMessage(toMessage(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runOutcome = async (label: string, action: () => Promise<import('ops-pilot-shared').DockerOutcome>) => {
    setBusy(true);
    setMessage(null);
    onEvent(label, 'info');
    try {
      const outcome = await action();
      const summary = api.describeDockerOutcome(outcome);
      setMessage(summary);
      onEvent(summary, 'unavailable' in outcome || 'error' in outcome ? 'warning' : 'success');
      if ('logs' in outcome) {
        setLogsFor(outcome.logs.container);
        setLogsText(outcome.logs.output);
      }
      await refresh();
    } catch (err) {
      const text = toMessage(err);
      setMessage(text);
      onEvent(text, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="docker-panel">
      <div className="section-header">
        <h3>Docker</h3>
        <button type="button" onClick={() => void refresh()} disabled={busy}>
          Refresh
        </button>
      </div>

      {hasCompose && (
        <div className="db-actions">
          <button
            type="button"
            className="btn-start"
            disabled={busy}
            onClick={() => void runOutcome('Compose up', () => api.dockerCompose(projectPath, 'up'))}
          >
            Compose up
          </button>
          <button
            type="button"
            className="btn-stop"
            disabled={busy}
            onClick={() => void runOutcome('Compose down', () => api.dockerCompose(projectPath, 'down'))}
          >
            Compose down
          </button>
        </div>
      )}

      {message && <p className="banner">{message}</p>}

      {containers.length === 0 ? (
        <p className="hint">No containers found. Start the compose stack or check that the Docker daemon is running.</p>
      ) : (
        <ul className="plan-list">
          {containers.map((container) => (
            <li className="plan-step" key={container.id || container.name}>
              <div className="plan-step-main">
                <span className="plan-service">{container.name}</span>
                <span className={container.running ? 'check-mark' : ''}>
                  {container.running ? 'Running' : 'Stopped'}
                </span>
              </div>
              <div className="plan-step-sub">
                <span className="plan-description">
                  {container.image} · {container.status}
                  {container.ports.length > 0 ? ` · ${container.ports.join(', ')}` : ''}
                </span>
              </div>
              <div className="db-actions">
                <button
                  type="button"
                  className="btn-start"
                  disabled={busy || container.running}
                  onClick={() => void runOutcome(`Start container ${container.name}`, () => api.dockerContainerAction(container.name, 'start'))}
                >
                  Start
                </button>
                <button
                  type="button"
                  className="btn-stop"
                  disabled={busy || !container.running}
                  onClick={() => void runOutcome(`Stop container ${container.name}`, () => api.dockerContainerAction(container.name, 'stop'))}
                >
                  Stop
                </button>
                <button
                  type="button"
                  className="btn-restart"
                  disabled={busy}
                  onClick={() => void runOutcome(`Restart container ${container.name}`, () => api.dockerContainerAction(container.name, 'restart'))}
                >
                  Restart
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void runOutcome(`Fetch logs for ${container.name}`, () => api.dockerContainerAction(container.name, 'logs'))}
                >
                  Logs
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}

      {logsFor && (
        <div className="logs-output">
          <div className="logs-header">
            <h4>Logs: {logsFor}</h4>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setLogsFor(null)}>
              Close
            </button>
          </div>
          <pre className="logs-content">{logsText || '(no output)'}</pre>
        </div>
      )}
    </section>
  );
}
