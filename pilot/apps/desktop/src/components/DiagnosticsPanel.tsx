/**
 * Diagnostics panel: every deterministic check with its problem, evidence,
 * possible cause and recommended action.
 */

import type { DiagnosticsReport } from 'ops-pilot-shared';

interface DiagnosticsPanelProps {
  report: DiagnosticsReport | null;
  onRun: () => void;
}

export function DiagnosticsPanel({ report, onRun }: DiagnosticsPanelProps) {
  return (
    <section className="diagnostics">
      <div className="section-header">
        <h3>Diagnostics</h3>
        <button type="button" onClick={onRun}>
          Run diagnostics
        </button>
      </div>

      {report === null ? (
        <p className="hint">
          Checks runtimes, installed dependencies and environment configuration.
        </p>
      ) : (
        <>
          <p className="hint">
            {report.issues === 0
              ? `No issues found (${report.checks.length} checks).`
              : `${report.issues} issue(s) found in ${report.checks.length} checks.`}
          </p>

          <ul className="check-list">
            {report.checks.map((check) => (
              <li
                className={check.passed ? 'check check-passed' : 'check check-failed'}
                key={check.id}
              >
                <div className="check-header">
                  <span className="check-mark">{check.passed ? '✓' : '✗'}</span>
                  <span className="check-name">{check.name}</span>
                </div>

                {check.problem && <p className="check-problem">{check.problem}</p>}
                {check.evidence && <p className="check-detail">Evidence: {check.evidence}</p>}
                {check.cause && <p className="check-detail">Possible cause: {check.cause}</p>}
                {check.recommendedAction && (
                  <p className="check-action">Recommended: {check.recommendedAction}</p>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}