/**
 * OpsPilot - Cross-Platform Project Operations Launcher
 *
 * The window renders whatever the Pilot engine detected. It holds no project
 * knowledge of its own: detection, service status and diagnostics all come from
 * the Rust engine through typed commands (see `api.ts`).
 */

import { useCallback, useEffect, useState } from 'react';
import type { DiagnosticsReport, ScanResult, ServiceStatus } from 'ops-pilot-shared';
import * as api from './api';
import { toMessage } from './api';
import { ProjectDashboard } from './components/ProjectDashboard';
import './style.css';

/** How often the observed service state is refreshed */
const STATUS_INTERVAL_MS = 5000;

function App() {
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [services, setServices] = useState<ServiceStatus[]>([]);
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const detected = scan?.detected ?? false;

  const refreshStatus = useCallback(async () => {
    try {
      setServices(await api.getStatus());
    } catch (statusError) {
      setError(toMessage(statusError));
    }
  }, []);

  useEffect(() => {
    api
      .detectProject()
      .then(setScan)
      .catch((detectError: unknown) => setError(toMessage(detectError)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!detected) {
      setServices([]);
      return;
    }

    void refreshStatus();
    const timer = window.setInterval(() => void refreshStatus(), STATUS_INTERVAL_MS);

    return () => window.clearInterval(timer);
  }, [detected, refreshStatus]);

  const runAction = async (action: () => Promise<string>) => {
    setMessage(null);
    setError(null);

    try {
      setMessage(await action());
    } catch (actionError) {
      setMessage(toMessage(actionError));
    }
  };

  const runDiagnostics = async () => {
    setError(null);

    try {
      setReport(await api.runDiagnostics());
    } catch (diagnosticsError) {
      setError(toMessage(diagnosticsError));
    }
  };

  if (loading) {
    return (
      <div className="app">
        <p className="banner">Scanning this directory…</p>
      </div>
    );
  }

  return (
    <div className="app">
      <header>
        <h1>OpsPilot</h1>
        <p>Cross-Platform Project Operations Launcher</p>
      </header>

      <main>
        {error && <p className="banner banner-error">{error}</p>}

        {scan?.model ? (
          <ProjectDashboard
            model={scan.model}
            evidence={scan.evidence}
            services={services}
            report={report}
            message={message}
            onStartProject={() => void runAction(api.startProject)}
            onStopProject={() => void runAction(api.stopProject)}
            onRestartProject={() => void runAction(api.restartProject)}
            onRunDiagnostics={() => void runDiagnostics()}
          />
        ) : (
          <div className="placeholder">
            <p>No project detected in this directory.</p>
            <p className="placeholder-hint">
              Start Pilot from a project folder to see its services and operations.
            </p>
          </div>
        )}
      </main>
    </div>
  );
}

export default App;