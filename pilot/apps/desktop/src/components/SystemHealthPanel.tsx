/**
 * System diagnostics panel: OpsPilot's own health plus execution readiness.
 *
 * Every subsystem reports its state, and every startup step reports whether
 * it can run right now and why not. When something fails, the actual reason
 * is shown instead of a generic error.
 */

import { useCallback, useEffect, useState } from 'react';
import type { SystemHealth } from 'ops-pilot-shared';
import * as api from '../api';
import { toMessage } from '../api';

interface SystemHealthPanelProps {
  projectPath: string;
}

function Row({ label, ok, detail }: { label: string; ok: boolean; detail?: string }) {
  return (
    <div className="capability">
      <dt>{label}</dt>
      <dd>
        <span className={ok ? 'check-mark' : ''}>{ok ? '✓' : '✗'}</span>
        {detail ? ` ${detail}` : ''}
      </dd>
    </div>
  );
}

export function SystemHealthPanel({ projectPath }: SystemHealthPanelProps) {
  const [health, setHealth] = useState<SystemHealth | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setHealth(await api.systemHealth(projectPath));
      setError(null);
    } catch (err) {
      setError(toMessage(err));
    } finally {
      setBusy(false);
    }
  }, [projectPath]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <section className="system-health">
      <div className="section-header">
        <h3>System diagnostics</h3>
        <button type="button" onClick={() => void refresh()} disabled={busy}>
          {busy ? 'Checking…' : 'Re-check'}
        </button>
      </div>

      {error && <p className="banner banner-error">{error}</p>}

      {health === null ? (
        <p className="hint">No health data yet.</p>
      ) : (
        <>
          <h4 className="subsection-title">OpsPilot health</h4>
          <dl className="capability-list">
            <Row label="Frontend" ok={health.frontend} />
            <Row label="Tauri bridge" ok={health.tauriBridge} />
            <Row label="Rust backend" ok={health.rustBackend} />
            <Row label="Project scanner" ok={health.projectScanner} />
            <Row
              label="Project state"
              ok={health.projectDetected}
              detail={health.projectName ?? 'no project detected'}
            />
            <Row
              label="Process manager"
              ok={health.processManager}
              detail={`${health.trackedProcesses} tracked process(es)`}
            />
            <Row
              label="Docker integration"
              ok={health.dockerAvailable}
              detail={
                health.dockerAvailable
                  ? (health.dockerVersion ?? 'daemon running')
                  : 'daemon not running'
              }
            />
            <Row label="Docker compose" ok={health.composeAvailable} />
          </dl>

          <h4 className="subsection-title">Current project</h4>
          <dl className="capability-list">
            <div className="capability">
              <dt>Path</dt>
              <dd>{health.projectPath ?? '—'}</dd>
            </div>
            <div className="capability">
              <dt>Type</dt>
              <dd>{health.projectDetected ? 'detected' : 'not detected'}</dd>
            </div>
          </dl>

          <h4 className="subsection-title">Execution readiness</h4>
          {health.readiness.length === 0 ? (
            <p className="hint">Nothing is startable for this project.</p>
          ) : (
            <ul className="check-list">
              {health.readiness.map((entry) => (
                <li
                  className={entry.ready ? 'check check-passed' : 'check check-failed'}
                  key={`${entry.service}-${entry.command}`}
                >
                  <div className="check-header">
                    <span className="check-mark">{entry.ready ? '✓' : '✗'}</span>
                    <span className="check-name">
                      {entry.service}: {entry.command}
                    </span>
                  </div>
                  <p className="check-detail">{entry.detail}</p>
                </li>
              ))}
            </ul>
          )}

          {health.warnings.length > 0 && (
            <>
              <h4 className="subsection-title">Warnings</h4>
              <ul className="evidence-list">
                {health.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
    </section>
  );
}
