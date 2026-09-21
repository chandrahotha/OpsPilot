/**
 * Scripts panel: every command the scanner declared for the project
 * (npm/pnpm/yarn scripts, Django runners, ...), each runnable for real.
 *
 * Only declared commands can run, under the tracked label `script-<name>`,
 * so stopping works through the regular process lifecycle.
 */

import { useCallback, useEffect, useState } from 'react';
import type { CommandInfo } from 'ops-pilot-shared';
import * as api from '../api';
import { toMessage } from '../api';

interface ScriptsPanelProps {
  projectPath: string;
  commands?: CommandInfo[];
}

export function ScriptsPanel({ projectPath, commands }: ScriptsPanelProps) {
  const [running, setRunning] = useState<string[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshRunning = useCallback(async () => {
    try {
      const list = await api.listProcesses();
      setRunning(
        list
          .filter((process) => process.state === 'Running')
          .map((process) => process.label),
      );
    } catch {
      // The panel stays usable without the running set; Run just reports.
    }
  }, []);

  useEffect(() => {
    void refreshRunning();
  }, [refreshRunning]);

  if (!commands || commands.length === 0) {
    return null;
  }

  const run = async (name: string) => {
    setBusy(true);
    setMessage(null);
    try {
      setMessage(await api.runScript(projectPath, name));
      await refreshRunning();
    } catch (err) {
      setMessage(toMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const stop = async (name: string) => {
    setBusy(true);
    setMessage(null);
    try {
      setMessage(await api.stopProject(`script-${name}`));
      await refreshRunning();
    } catch (err) {
      setMessage(toMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="scripts-panel">
      <div className="section-header">
        <h3>Scripts</h3>
        <button type="button" onClick={() => void refreshRunning()} disabled={busy}>
          Refresh
        </button>
      </div>
      <ul className="plan-list">
        {commands.map((command) => {
          const label = `script-${command.name}`;
          const isRunning = running.includes(label);
          return (
            <li className="plan-step" key={command.name}>
              <div className="plan-step-main">
                <span className="plan-service">{command.name}</span>
                <span className="plan-command">{command.command}</span>
              </div>
              <div className="plan-step-sub">
                <span className="plan-description">{command.source}</span>
                {isRunning && <span className="check-mark">Running</span>}
              </div>
              {isRunning ? (
                <button type="button" className="btn-stop" disabled={busy} onClick={() => void stop(command.name)}>
                  Stop
                </button>
              ) : (
                <button type="button" className="btn-start" disabled={busy} onClick={() => void run(command.name)}>
                  Run
                </button>
              )}
            </li>
          );
        })}
      </ul>
      {message && <pre className="banner banner-pre">{message}</pre>}
    </section>
  );
}
