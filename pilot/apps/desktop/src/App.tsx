import { useCallback, useEffect, useState } from 'react';
import type { DiagnosticsReport, ScanResult, ServiceStatus } from 'ops-pilot-shared';
import * as api from './api';
import { toMessage } from './api';
import { ProjectDashboard } from './components/ProjectDashboard';
import './style.css';

const STATUS_INTERVAL_MS = 5000;

interface ScanLogEntry {
  timestamp: Date;
  message: string;
  type: 'info' | 'success' | 'warning' | 'error';
}

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

  const detected = scan?.detected ?? false;

  const addScanLog = useCallback((message: string, type: ScanLogEntry['type'] = 'info') => {
    setScanLogs(prev => [...prev, { timestamp: new Date(), message, type }]);
  }, []);

  const clearScanLogs = useCallback(() => {
    setScanLogs([]);
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      setServices(await api.getStatus());
    } catch (statusError) {
      setError(toMessage(statusError));
    }
  }, []);

  const doScan = useCallback(async (path?: string) => {
    setIsScanning(true);
    setLoading(true);
    clearScanLogs();
    addScanLog('Starting project scan...', 'info');
    
    try {
      if (path) {
        setCurrentPath(path);
        addScanLog('Scanning: ' + path, 'info');
      } else {
        addScanLog('Scanning current directory...', 'info');
      }

      const result = await api.detectProject(path);
      setScan(result);
      
      if (result.detected) {
        addScanLog('Project detected: ' + (result.model?.project?.name || 'Unknown'), 'success');
        addScanLog('  Path: ' + (result.model?.project?.path || 'Unknown'), 'info');
        if (result.evidence && result.evidence.length > 0) {
          result.evidence.forEach(e => addScanLog('  Evidence: ' + e, 'info'));
        }
      } else {
        addScanLog('No project detected in this directory', 'warning');
      }
      
      setError(null);
    } catch (detectError) {
      const errMsg = toMessage(detectError);
      addScanLog('Scan failed: ' + errMsg, 'error');
      setError(errMsg);
      setScan(null);
    } finally {
      setIsScanning(false);
      setLoading(false);
    }
  }, [addScanLog, clearScanLogs]);

  useEffect(() => {
    doScan();
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
        await doScan(path);
      }
    } catch (err) {
      setError(toMessage(err));
    }
  };

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

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  };

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
      <header>
        <div className='header-left'>
          <h1>OpsPilot</h1>
          <p>Cross-Platform Project Operations Launcher</p>
        </div>
        <div className='header-right'>
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
                <p className='no-logs'>No scan logs yet. Click Select Project to scan a directory.</p>
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
            onStartProject={() => void runAction(api.startProject)}
            onStopProject={() => void runAction(api.stopProject)}
            onRestartProject={() => void runAction(api.restartProject)}
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
