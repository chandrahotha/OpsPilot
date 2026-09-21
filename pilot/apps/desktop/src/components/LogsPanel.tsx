/**
 * Live logs / activity center: the right side of the command center.
 *
 * - Process output (stdout/stderr/system) for services Pilot started,
 *   auto-refreshed while unpaused. Selection comes from the engine's
 *   tracked-process list, so only real managed processes appear.
 * - Session activity: real detection/lifecycle events pushed by the app
 *   (project detected, service started/stopped/failed, diagnostics, ...).
 *
 * Nothing here is fabricated: an empty state says so explicitly.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { LogEntry, LogStream, ProcessSnapshot } from 'ops-pilot-shared';
import type { ActivityEvent } from '../App';
import * as api from '../api';
import { toMessage } from '../api';

const LOG_INTERVAL_MS = 2000;

type StreamFilter = 'all' | LogStream;

interface LogsPanelProps {
  events: ActivityEvent[];
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

export function LogsPanel({ events }: LogsPanelProps) {
  const [processes, setProcesses] = useState<ProcessSnapshot[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [query, setQuery] = useState('');
  const [stream, setStream] = useState<StreamFilter>('all');
  const [copied, setCopied] = useState(false);
  const logsRef = useRef<HTMLPreElement>(null);

  const refreshProcesses = useCallback(async () => {
    try {
      const list = await api.listProcesses();
      setProcesses(list);
      setSelected((current) => {
        if (list.length === 0) {
          return '';
        }
        if (current && list.some((p) => p.label === current)) {
          return current;
        }
        return list[0].label;
      });
      if (list.length === 0) {
        setLogs([]);
      }
    } catch (err) {
      setMessage(toMessage(err));
    }
  }, []);

  const refreshLogs = useCallback(async () => {
    setSelected((current) => {
      if (current) {
        void api
          .getServiceLogs(current, 200)
          .then((entries) => {
            setLogs(entries);
            setMessage(null);
          })
          .catch((err: unknown) => setMessage(toMessage(err)));
      }
      return current;
    });
  }, []);

  useEffect(() => {
    void refreshProcesses();
    const timer = window.setInterval(() => void refreshProcesses(), 5000);
    return () => window.clearInterval(timer);
  }, [refreshProcesses]);

  useEffect(() => {
    if (paused || !selected) {
      return;
    }
    void refreshLogs();
    const timer = window.setInterval(() => void refreshLogs(), LOG_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [paused, selected, refreshLogs]);

  useEffect(() => {
    if (!paused && logsRef.current) {
      logsRef.current.scrollTop = logsRef.current.scrollHeight;
    }
  }, [logs, paused]);

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

      {processes.length === 0 ? (
        <p className="hint">No processes tracked yet. Start a service to capture its output here.</p>
      ) : (
        <>
          <div className="logs-select-row">
            <label htmlFor="log-service">Service</label>
            <select
              id="log-service"
              value={selected}
              onChange={(event) => {
                setSelected(event.target.value);
                setLogs([]);
              }}
            >
              {processes.map((process) => (
                <option key={process.label} value={process.label}>
                  {process.label} ({process.state}
                  {process.pid ? `, pid ${process.pid}` : ''})
                </option>
              ))}
            </select>
          </div>
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
