/**
 * Database panel: migrate / seed / status / backup plus confirmed reset/restore.
 *
 * Destructive operations use a two-step flow: the first call returns
 * `needsConfirmation`, the GUI asks the user explicitly, and only then the
 * operation is re-issued with `confirmed=true`. Nothing destructive ever runs
 * without the user saying so twice.
 */

import { useState } from 'react';
import type { DatabaseOutcome } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import * as api from '../api';
import { toMessage } from '../api';

/** Append the captured process output (if any) so failures show real evidence. */
function withOutput(summary: string, outcome: DatabaseOutcome): string {
  if ('error' in outcome && outcome.error.output) {
    return `${summary}\n\nOutput:\n${outcome.error.output}`;
  }
  return summary;
}

interface DatabasePanelProps {
  projectPath: string;
  ormType?: string;
  databaseType?: string;
  onEvent: (message: string, kind: ActivityEvent['kind']) => void;
}

const OPERATIONS = ['status', 'migrate', 'seed', 'backup'] as const;
const DESTRUCTIVE = ['reset', 'restore'] as const;

export function DatabasePanel({ projectPath, ormType, databaseType, onEvent }: DatabasePanelProps) {
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const report = (text: string, kind: ActivityEvent['kind']) => {
    setMessage(text);
    onEvent(text.split('\n')[0], kind);
  };

  const runOperation = async (operation: string, confirmed: boolean) => {
    setBusy(true);
    setMessage(null);
    onEvent(`Database ${operation}`, 'info');
    try {
      const outcome = await api.databaseOperation(projectPath, operation, confirmed);
      if ('needsConfirmation' in outcome) {
        const ok = window.confirm(
          `Database ${operation} is destructive (risk: ${outcome.needsConfirmation.risk}). Are you sure?`,
        );
        if (ok) {
          const second = await api.databaseOperation(projectPath, operation, true);
          report(
            withOutput(api.describeDatabaseOutcome(second), second),
            'success' in second ? 'success' : 'warning',
          );
        } else {
          report(`${operation} cancelled.`, 'warning');
        }
      } else {
        report(
          withOutput(api.describeDatabaseOutcome(outcome), outcome),
          'success' in outcome
            ? 'success'
            : 'notImplemented' in outcome
              ? 'warning'
              : 'error',
        );
      }
    } catch (err) {
      const text = toMessage(err);
      setMessage(text);
      onEvent(text, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="database-panel">
      <div className="section-header">
        <h3>Database</h3>
        <span className="hint">
          {ormType ? `ORM: ${ormType}` : ''}
          {ormType && databaseType ? ' · ' : ''}
          {databaseType ? `DB: ${databaseType}` : ''}
        </span>
      </div>

      <div className="db-actions">
        {OPERATIONS.map((operation) => (
          <button
            key={operation}
            type="button"
            disabled={busy}
            onClick={() => void runOperation(operation, false)}
          >
            {operation}
          </button>
        ))}
        {DESTRUCTIVE.map((operation) => (
          <button
            key={operation}
            type="button"
            className="btn-danger"
            disabled={busy}
            onClick={() => void runOperation(operation, false)}
          >
            {operation}
          </button>
        ))}
      </div>

      {message && <pre className="banner banner-pre">{message}</pre>}
    </section>
  );
}
