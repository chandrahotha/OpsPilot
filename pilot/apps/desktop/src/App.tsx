import { useCallback, useEffect, useState } from 'react';
import type { DiagnosticsReport, ScanResult, ServiceStatus, StartupPlan } from 'ops-pilot-shared';
import * as api from './api';
import { toMessage } from './api';
import { ProjectDashboard } from './components/ProjectDashboard';
import './style.css';

const STATUS_INTERVAL_MS = 5000;
const LAST_PROJECT_KEY = 'opspilot:lastProject';

interface ScanLogEntry {
  timestamp: Date;
  message: string;
  type: 'info' | 'success' | 'warning' | 'error';
}

export interface ActivityEvent {
  timestamp: Date;
  kind: 'info' | 'success' | 'warning' | 'error';
  text: string;
}

type OverallStatus = 'NO PROJECT' | 'READY' | 'STARTING' | 'RUNNING' | 'STOPPING' | 'DEGRADED' | 'FAILED';

const STATUS_CLASS: Record<OverallStatus, string> = {
  'NO PROJECT': 'status-chip status-off',
  READY: 'status-chip status-ready',
  STARTING: 'status-chip status-busy',
  RUNNING: 'status-chip status-running',
  STOPPING: 'status-chip status-busy',
  DEGRADED: 'status-chip status-degraded',
  FAILED: 'status-chip status-failed',
};

function App() {
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [services, setServices] = useState<ServiceStatus[]>([]);
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [currentPath, setCurrentPath] = useState<string>('');
  const [scanLogs, setScanLogs] = useState<ScanLogEntry[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [showLogs, setShowLogs] = useState(false);
  const [plan, setPlan] = useState<StartupPlan | null>(null);
  const [busyServices, setBusyServices] = useState<Record<string, string>>({});
  const [bulkBusy, setBulkBusy] = useState<string | null>(null);
  const [events, setEvents] = useState<ActivityEvent[]>([]);

  const detected = scan?.detected ?? false;

  const pushEvent = useCallback((text: string, kind: ActivityEvent['kind'] = 'info') => {
    setEvents((prev) => [...prev.slice(-199), { timestamp: new Date(), kind, text }]);
  }, []);

  const addScanLog = useCallback((message: string, type: ScanLogEntry['type'] = 'info') => {
    setScanLogs(prev => [...prev, { timestamp: new Date(), message, type }]);
  }, []);

  const clearScanLogs = useCallback(() => {
    setScanLogs([]);
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      setServices(await api.getStatus(currentPath));
    } catch (statusError) {
      setError(toMessage(statusError));
    }
  }, [currentPath]);

  const doScan = useCallback(async (path?: string) => {
    setIsScanning(true);
    setLoading(true);
    clearScanLogs();
    addScanLog('Starting project scan...', 'info');

    try {
      const effectivePath = path ?? undefined;
      if (effectivePath) {
        setCurrentPath(effectivePath);
        addScanLog('Scanning: ' + effectivePath, 'info');
      } else {
        addScanLog('Scanning current directory...', 'info');
      }

      const result = await api.detectProject(effectivePath);
      setScan(result);

      if (result.detected) {
        const name = result.model?.project?.name || 'Unknown';
        addScanLog('Project detected: ' + name, 'success');
        pushEvent(`Project detected: ${name}`, 'success');
        addScanLog('  Path: ' + (result.model?.project?.path || 'Unknown'), 'info');
        if (result.evidence && result.evidence.length > 0) {
          result.evidence.forEach(e => addScanLog('  Evidence: ' + e, 'info'));
        }
        try {
          const startupPlan = await api.getStartupPlan(effectivePath);
          setPlan(startupPlan);
          startupPlan.steps.forEach(s =>
            addScanLog(`  Plan: start ${s.service} via \`${s.command}\``, 'info'),
          );
          startupPlan.warnings.forEach(w => addScanLog('  Plan note: ' + w, 'warning'));
        } catch (planError) {
          addScanLog('Startup plan unavailable: ' + toMessage(planError), 'warning');
          setPlan(null);
        }
      } else {
        addScanLog('No project detected in this directory', 'warning');
        pushEvent('No project detected in this directory', 'warning');
        setPlan(null);
      }

      setError(null);
    } catch (detectError) {
      const errMsg = toMessage(detectError);
      addScanLog('Scan failed: ' + errMsg, 'error');
      pushEvent(`Scan failed: ${errMsg}`, 'error');
      setError(errMsg);
      setScan(null);
      setPlan(null);
    } finally {
      setIsScanning(false);
      setLoading(false);
    }
  }, [addScanLog, clearScanLogs]);

  useEffect(() => {
    // Restore the last opened project, if the user picked one before.
    // Reading localStorage can throw in locked-down environments, so fall
    // back to scanning the current directory instead of failing silently.
    let saved: string | null = null;
    try {
      saved = window.localStorage.getItem(LAST_PROJECT_KEY);
    } catch {
      saved = null;
    }
    doScan(saved ?? undefined);
  }, [doScan]);

  useEffect(() => {
    if (!detected) {
      setServices([]);
      return;
    }

    void refreshStatus();
    const timer = window.setInterval(() => void refreshStatus(), STATUS_INTERVAL_MS);

    return () => window.clearInterval(timer);
  }, [detected, refreshStatus]);

  const handleSelectDirectory = async () => {
    try {
      setError(null);
      const path = await api.selectDirectory();
      if (path) {
        try {
          window.localStorage.setItem(LAST_PROJECT_KEY, path);
        } catch {
          // Persistence is a convenience; a locked-down storage must not
          // break project selection.
        }
        await doScan(path);
      }
    } catch (err) {
      setError(toMessage(err));
    }
  };

  const runAction = async (action: () => Promise<string>): Promise<string> => {
    setMessage(null);
    setError(null);

    try {
      const result = await action();
      setMessage(result);
      return result;
    } catch (actionError) {
      const text = toMessage(actionError);
      setMessage(text);
      return text;
    }
  };

  /**
   * Lifecycle action for one service: marks the card busy (orange
   * "Starting…/Stopping…/Restarting…"), runs the command, then refreshes
   * service state immediately instead of waiting for the next poll.
   */
  const runServiceAction = async (service: string, label: string, action: () => Promise<string>) => {
    setBusyServices((prev) => ({ ...prev, [service]: label }));
    pushEvent(`${label} ${service}`, 'info');
    try {
      const result = await runAction(action);
      pushEvent(result, /fail|error|cannot|not found|skipped|timed out/i.test(result) ? 'warning' : 'success');
    } finally {
      setBusyServices((prev) => {
        const next = { ...prev };
        delete next[service];
        return next;
      });
      void refreshStatus();
    }
  };

  const runBulkAction = async (label: string, action: () => Promise<string>) => {
    setBulkBusy(label);
    pushEvent(label, 'info');
    try {
      const result = await runAction(action);
      pushEvent(result, /fail|error|could not|timed out/i.test(result) ? 'warning' : 'success');
    } finally {
      setBulkBusy(null);
      void refreshStatus();
    }
  };

  const openFrontend = async () => {
    pushEvent('Opening frontend in browser', 'info');
    try {
      const result = await api.openFrontend(currentPath);
      setMessage(result);
      pushEvent(result, 'success');
    } catch (err) {
      const text = toMessage(err);
      setMessage(text);
      pushEvent(text, 'error');
    }
  };

  const runDiagnostics = async () => {
    setError(null);
    pushEvent('Running diagnostics', 'info');

    try {
      const diagnostics = await api.runDiagnostics(currentPath || undefined);
      setReport(diagnostics);
      pushEvent(
        diagnostics.issues === 0
          ? `Diagnostics clean (${diagnostics.checks.length} checks)`
          : `${diagnostics.issues} issue(s) in ${diagnostics.checks.length} checks`,
        diagnostics.issues === 0 ? 'success' : 'warning',
      );
    } catch (diagnosticsError) {
      const text = toMessage(diagnosticsError);
      setError(text);
      pushEvent(`Diagnostics failed: ${text}`, 'error');
    }
  };

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  };

  const frontendService = services.find((service) => service.key === 'frontend');
  const frontendUrl =
    frontendService && frontendService.state === 'running' && frontendService.port != null
      ? `http://localhost:${frontendService.port}`
      : null;

  const overallStatus: OverallStatus = (() => {
    if (!detected) {
      return 'NO PROJECT';
    }
    if (bulkBusy !== null) {
      return /stop/i.test(bulkBusy) ? 'STOPPING' : 'STARTING';
    }
    if (Object.keys(busyServices).length > 0) {
      return 'STARTING';
    }
    const anyRunning = services.some((service) => service.state === 'running');
    const issues = report?.issues ?? 0;
    if (anyRunning) {
      return issues > 0 ? 'DEGRADED' : 'RUNNING';
    }
    return issues > 0 ? 'FAILED' : 'READY';
  })();

  if (loading) {
    return (
      <div className='app'>
        <div className='loading-screen'>
          <div className='spinner'></div>
          <p className='banner'>Scanning this directory</p>
          <div className='scan-logs-compact'>
            {scanLogs.map((log, i) => (
              <div key={i} className={'scan-log-entry ' + log.type}>
                <span className='log-time'>{formatTime(log.timestamp)}</span>
                <span className='log-message'>{log.message}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className='app'>
      <header className='command-header'>
        <div className='header-left'>
          <h1>OpsPilot</h1>
          <p>Project Command Center</p>
        </div>
        <div className='header-center'>
          <div className='header-project' title={currentPath || 'No project selected'}>
            {scan?.model?.project.name ?? 'No project'}
          </div>
          <span className={STATUS_CLASS[overallStatus]}>{overallStatus}</span>
        </div>
        <div className='header-right'>
          <button
            type='button'
            className='btn-open'
            onClick={() => void openFrontend()}
            disabled={frontendUrl === null}
            title={
              frontendUrl === null
                ? 'Start the frontend first, then open it here'
                : `Open ${frontendUrl} in the default browser`
            }
          >
            {frontendUrl === null ? 'Frontend Not Running' : 'Open Frontend'}
          </button>
          <div className='current-path' title={currentPath || 'Current directory'}>
            {currentPath || 'Current directory'}
          </div>
          <button
            className='btn btn-secondary select-dir-btn'
            onClick={handleSelectDirectory}
            disabled={isScanning}
          >
            {isScanning ? 'Scanning...' : 'Select Project'}
          </button>
          <button
            className='btn btn-ghost log-toggle'
            onClick={() => setShowLogs(!showLogs)}
            title={showLogs ? 'Hide scan logs' : 'Show scan logs'}
          >
            {showLogs ? 'Hide Logs' : 'Show Logs'}
          </button>
        </div>
      </header>

      <main>
        {error && <p className='banner banner-error'>{error}</p>}

        {showLogs && (
          <div className='scan-logs-panel'>
            <div className='logs-header'>
              <h3>Scan Logs</h3>
              <button className='btn btn-ghost btn-sm' onClick={clearScanLogs}>Clear</button>
            </div>
            <div className='scan-logs-content'>
              {scanLogs.length === 0 ? (
                <p className='no-logs'>
                  {scan !== null
                    ? 'Scan logs were cleared. Rescan to repopulate them.'
                    : 'No scan logs yet. Click Select Project to scan a directory.'}
                </p>
              ) : (
                scanLogs.map((log, i) => (
                  <div key={i} className={'scan-log-entry ' + log.type}>
                    <span className='log-time'>{formatTime(log.timestamp)}</span>
                    <span className='log-message'>{log.message}</span>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {scan?.model ? (
          <ProjectDashboard
            model={scan.model}
            evidence={scan.evidence}
            services={services}
            report={report}
            message={message}
            projectPath={currentPath}
            plan={plan}
            busyServices={busyServices}
            bulkBusy={bulkBusy}
            frontendUrl={frontendUrl}
            events={events}
            onStartProject={(service) =>
              void runServiceAction(service, 'Starting…', () => api.startProject(currentPath, service))
            }
            onStopProject={(service) =>
              void runServiceAction(service, 'Stopping…', () => api.stopProject(service))
            }
            onRestartProject={(service) =>
              void runServiceAction(service, 'Restarting…', () => api.restartProject(service))
            }
            onStartAll={() =>
              void runBulkAction('Starting everything…', () => api.startAllProject(currentPath))
            }
            onStopAll={() =>
              void runBulkAction('Stopping everything…', () => api.stopAllProject(currentPath))
            }
            onKillAll={() => {
              const ok = window.confirm(
                'Force-terminate every process Pilot started for this project? Use this only when Stop All did not work.',
              );
              if (ok) {
                void runBulkAction('Force-terminating everything…', () => api.killAllProject(currentPath));
              }
            }}
            onRescan={() => void doScan(currentPath || undefined)}
            onOpenFrontend={() => void openFrontend()}
            onEvent={pushEvent}
            onRunDiagnostics={() => void runDiagnostics()}
          />
        ) : (
          <div className='placeholder'>
            <p>No project detected in this directory.</p>
            <p className='placeholder-hint'>
              Click <strong>Select Project</strong> to choose a project folder, 
              or start Pilot from a project folder to see its services and operations.
            </p>
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
