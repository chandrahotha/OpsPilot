/**
 * Docker panel: compose stack controls plus per-container operations.
 *
 * Containers are attributed to projects: the current project's own compose
 * containers render fully, while containers belonging to other projects are
 * grouped separately with a clear warning. Every action delegates to the
 * Docker engine via Tauri commands and reports the real outcome (including
 * "daemon not running") instead of failing silently.
 *
 * Container output is never shown here: the Logs button focuses the Live
 * logs side panel, so all comms live in the cockpit, not in a side room.
 */

import { useCallback, useEffect, useState } from 'react';
import type { ContainerStatus } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import * as api from '../api';
import { toMessage } from '../api';

interface DockerPanelProps {
  projectPath: string;
  projectName: string;
  hasCompose: boolean;
  onEvent: (message: string, kind: ActivityEvent['kind']) => void;
  onViewLogs: (containerName: string) => void;
}

/**
 * Expected compose project name for a directory: Docker Compose defaults to
 * the lowercased directory basename. Valid project names are lowercase letters,
 * digits, dashes and underscores.
 */
export function expectedComposeProject(projectPath: string): string {
  const base = projectPath.split(/[\\/]/).filter((part) => part.length > 0).pop() ?? '';
  return base.toLowerCase().replace(/[^a-z0-9_-]/g, '');
}

function normalizeProject(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9_-]/g, '');
}

function belongsToProject(container: ContainerStatus, expected: string): boolean {
  if (!container.composeProject || expected.length === 0) {
    return false;
  }
  return normalizeProject(container.composeProject) === expected;
}

export function DockerPanel({ projectPath, projectName, hasCompose, onEvent, onViewLogs }: DockerPanelProps) {
  const [containers, setContainers] = useState<ContainerStatus[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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
      await refresh();
    } catch (err) {
      const text = toMessage(err);
      setMessage(text);
      onEvent(text, 'error');
    } finally {
      setBusy(false);
    }
  };

  const viewLogs = (container: ContainerStatus) => {
    onEvent(`Viewing logs for ${container.name} in Live logs`, 'info');
    onViewLogs(container.name);
  };

  const expected = expectedComposeProject(projectPath);
  const mine = containers.filter((container) => belongsToProject(container, expected));
  const foreign = containers.filter((container) => !belongsToProject(container, expected));

  const renderContainer = (container: ContainerStatus, foreign: boolean) => (
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
          {container.composeProject ? ` · project ${container.composeProject}` : ' · standalone'}
        </span>
      </div>
      {foreign && (
        <p className="hint">
          Belongs to {container.composeProject ?? 'another setup'}, not to {projectName}. Stopping it
          affects that project.
        </p>
      )}
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
          onClick={() => viewLogs(container)}
        >
          Logs
        </button>
      </div>
    </li>
  );

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
        <>
          {mine.length > 0 && (
            <>
              <h4 className="subsection-title">This project ({projectName})</h4>
              <ul className="plan-list">
                {mine.map((container) => renderContainer(container, false))}
              </ul>
            </>
          )}
          {mine.length === 0 && (
            <p className="hint">None of the running containers belong to {projectName}.</p>
          )}
          {foreign.length > 0 && (
            <details className="foreign-containers">
              <summary>
                {foreign.length} container(s) from other projects
              </summary>
              <p className="hint">
                These were not started for {projectName}. You can still stop them or read their
                logs, but that affects their own project.
              </p>
              <ul className="plan-list">
                {foreign.map((container) => renderContainer(container, true))}
              </ul>
            </details>
          )}
        </>
      )}
    </section>
  );
}
