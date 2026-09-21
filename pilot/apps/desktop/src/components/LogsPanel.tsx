/**
 * Live logs / activity center: the right side of the command center.
 *
 * Two real source kinds, one comms panel:
 * - Pilot processes (tracked services and scripts), auto-refreshed.
 * - Docker containers (this project first, others marked foreign), read live
 *   from the daemon. The Docker panel's Logs buttons focus this panel instead
 *   of rendering output inline somewhere else.
 * - Session activity: real detection/lifecycle events pushed by the app.
 *
 * Nothing here is fabricated: an empty state says so explicitly.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { ContainerStatus, LogEntry, LogStream, ProcessSnapshot } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import * as api from '../api';
import { toMessage } from '../api';
import { expectedComposeProject } from './DockerPanel';

const LOG_INTERVAL_MS = 2000;
const REFRESH_INTERVAL_MS = 5000;

type StreamFilter = 'all' | LogStream;

export interface LogFocus {
  /** `proc:<label>` or `docker:<name>` */
  key: string;
  ts: number;
}

interface LogsPanelProps {
  events: ActivityEvent[];
  projectName: string;
  focus: LogFocus | null;
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function procKey(label: string): string {
  return `proc:${label}`;
}

function dockerKey(name: string): string {
  return `docker:${name}`;
}

export function LogsPanel({ events, projectName, focus }: LogsPanelProps) {
  const [processes, setProcesses] = useState<ProcessSnapshot[]>([]);
  const [containers, setContainers] = useState<ContainerStatus[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [query, setQuery] = useState('');
  const [stream, setStream] = useState<StreamFilter>('all');
  const [copied, setCopied] = useState(false);
  const logsRef = useRef<HTMLPreElement>(null);

  const expected = expectedComposeProject(projectName);

  const refreshSources = useCallback(async () => {
    try {
      const list = await api.listProcesses();
      setProcesses(list);
    } catch (err) {
      setMessage(toMessage(err));
      return;
    }
    try {
      const outcome = await api.listDockerContainers();
      if ('containers' in outcome) {
        setContainers(outcome.containers);
      }
    } catch {
      // Container sources are optional; Pilot processes keep working.
    }
    setSelected((current) => {
      if (current) {
        return current;
      }
      return '';
    });
  }, []);

  const refreshLogs = useCallback(async () => {
    if (!selected) {
      return;
    }
    try {
      if (selected.startsWith('docker:')) {
        const name = selected.slice('docker:'.length);
        const outcome = await api.dockerContainerAction(name, 'logs');
        if ('logs' in outcome) {
          const now = Date.now();
          setLogs(
            outcome.logs.output
              .split('\n')
              .map((line) => ({ timestampMs: now, service: name, stream: 'stdout' as LogStream, message: line })),
          );
          setMessage(null);
        } else {
          setMessage(api.describeDockerOutcome(outcome));
        }
      } else {
        const label = selected.slice('proc:'.length);
        setLogs(await api.getServiceLogs(label, 200));
        setMessage(null);
      }
    } catch (err) {
      setMessage(toMessage(err));
    }
  }, [selected]);

  useEffect(() => {
    void refreshSources();
    const timer = window.setInterval(() => void refreshSources(), REFRESH_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [refreshSources]);

  useEffect(() => {
    if (paused || !selected) {
      return;
    }
    void refreshLogs();
    const timer = window.setInterval(() => void refreshLogs(), LOG_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [paused, selected, refreshLogs]);

  useEffect(() => {
    if (focus) {
      setSelected(focus.key);
      setLogs([]);
      setPaused(false);
    }
  }, [focus]);

  useEffect(() => {
    if (!paused && logsRef.current) {
      logsRef.current.scrollTop = logsRef.current.scrollHeight;
    }
  }, [logs, paused]);

  const foreign = (container: ContainerStatus): boolean => {
    if (!container.composeProject) {
      return true;
    }
    return container.composeProject.toLowerCase().replace(/[^a-z0-9]/g, '') !== expected;
  };

  const selectedLabel = (() => {
    if (selected.startsWith('docker:')) {
      return `Container ${selected.slice('docker:'.length)}`;
    }
    if (selected.startsWith('proc:')) {
      return `Process ${selected.slice('proc:'.length)}`;
    }
    return '';
  })();

  const visible = logs.filter((entry) => {
    if (stream !== 'all' && entry.stream !== stream) {
      return false;
    }
    if (query && !`${entry.stream} ${entry.message}`.toLowerCase().includes(query.toLowerCase())) {
      return false;
    }
    return true;
  });

  const copyLogs = async () => {
    try {
      await navigator.clipboard.writeText(
        visible.map((entry) => `[${entry.stream}] ${entry.message}`).join('\n'),
      );
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  const hasSources = processes.length > 0 || containers.length > 0;

  return (
    <section className="logs-panel">
      <div className="section-header">
        <h3>Live logs</h3>
        <div className="db-actions">
          <button type="button" onClick={() => setPaused((value) => !value)} disabled={!selected}>
            {paused ? 'Resume' : 'Pause'}
          </button>
          <button type="button" onClick={() => void copyLogs()} disabled={visible.length === 0}>
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
      </div>

      {message && <p className="banner banner-error">{message}</p>}

      {!hasSources ? (
        <p className="hint">No processes tracked yet. Start a service to capture its output here.</p>
      ) : (
        <>
          <div className="logs-select-row">
            <label htmlFor="log-service">Source</label>
            <select
              id="log-service"
              value={selected}
              onChange={(event) => {
                setSelected(event.target.value);
                setLogs([]);
              }}
            >
              <option value="">Select a source…</option>
              {processes.length > 0 && (
                <optgroup label="Pilot processes">
                  {processes.map((process) => (
                    <option key={procKey(process.label)} value={procKey(process.label)}>
                      {process.label} ({process.state}
                      {process.pid ? `, pid ${process.pid}` : ''})
                    </option>
                  ))}
                </optgroup>
              )}
              {containers.length > 0 && (
                <optgroup label="Docker containers">
                  {containers.map((container) => (
                    <option key={dockerKey(container.name)} value={dockerKey(container.name)}>
                      {container.name}
                      {foreign(container) ? ` (other: ${container.composeProject ?? 'standalone'})` : ''}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
          </div>
          {selectedLabel && <p className="hint">Showing {selectedLabel}. Output refreshes automatically unless paused.</p>}
          <div className="logs-select-row">
            <label htmlFor="log-search">Search</label>
            <input
              id="log-search"
              type="search"
              placeholder="filter output…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <select
              aria-label="Stream filter"
              value={stream}
              onChange={(event) => setStream(event.target.value as StreamFilter)}
            >
              <option value="all">all streams</option>
              <option value="stdout">stdout</option>
              <option value="stderr">stderr</option>
              <option value="system">system</option>
            </select>
          </div>
          <pre ref={logsRef} className="logs-content">
            {paused ? '— paused —\n' : ''}
            {visible.length === 0
              ? '(no output captured yet)'
              : visible.map((entry) => `[${entry.stream}] ${entry.message}`).join('\n')}
          </pre>
        </>
      )}

      <h4 className="subsection-title">Activity</h4>
      {events.length === 0 ? (
        <p className="hint">No activity yet this session.</p>
      ) : (
        <div className="scan-logs-content activity-feed">
          {events.slice(-30).map((event, index) => (
            <div key={`${event.timestamp.getTime()}-${index}`} className={`scan-log-entry ${event.kind}`}>
              <span className="log-time">{formatTime(event.timestamp)}</span>
              <span className="log-message">{event.text}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
