import { writable } from 'svelte/store';
import { printer, events, showToast } from './stores';
import type { FullStatus, AppEvent, DetectionPoint } from './stores';

export const wsConnected = writable(false);
export const wsError = writable<string | null>(null);
export const wsStale = writable(false);

let ws: WebSocket | null = null;
let reconnectTimer: number | null = null;
let reconnectAttempts = 0;
let lastPongAt = 0;
let lastStateTs = 0;
// skip transition toasts until first state
let prevCameraConnected: boolean | null = null;
let prevMachineStatus: number | null = null;

export function connect() {
  if (ws) return;

  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${proto}//${window.location.host}/ws`;

  wsConnected.set(false);
  wsError.set(null);

  try {
    ws = new WebSocket(url);
  } catch {
    wsError.set('Failed to connect');
    return;
  }

  ws.onopen = () => {
    wsConnected.set(true);
    wsError.set(null);
    wsStale.set(false);
    reconnectAttempts = 0;
    lastPongAt = Date.now();
    lastStateTs = 0;
    prevCameraConnected = null;
  };

  ws.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data);
      if (msg.type === 'state' && msg.data) {
        const HISTORY_MAX = 60;
        const nowCameraConnected = msg.camera_connected === true;
        lastStateTs = Date.now();
        wsStale.set(false);

        printer.update((s) => {
          const newNozzle = msg.data?.extruder?.temperature ?? s.state?.extruder?.temperature;
          const newBed = msg.data?.heater_bed?.temperature ?? s.state?.heater_bed?.temperature;
          const nozzle_history = newNozzle != null
            ? [...s.nozzle_history, newNozzle].slice(-HISTORY_MAX)
            : s.nozzle_history;
          const bed_history = newBed != null
            ? [...s.bed_history, newBed].slice(-HISTORY_MAX)
            : s.bed_history;
          const incoming = (msg.detection_history as DetectionPoint[] | undefined) ?? [];
          let mergedHistory: DetectionPoint[];
          if (incoming.length === 0) {
            mergedHistory = s.detection_history;
          } else {
            const lastTs = s.detection_history.length
              ? s.detection_history[s.detection_history.length - 1].ts
              : 0;
            const newPts = incoming.filter((p) => p.ts > lastTs);
            mergedHistory = newPts.length
              ? [...s.detection_history, ...newPts]
              : s.detection_history;
          }

          return {
            ...s,
            state: msg.data as FullStatus,
            connected: msg.connected === true,
            printer_ws_status: msg.printer_ws_status ?? s.printer_ws_status,
            printer_ip: msg.printer_ip ?? s.printer_ip,
            camera_connected: nowCameraConnected,
            phase: msg.phase ?? s.phase,
            detection_score: msg.detection_score ?? s.detection_score,
            detection_history: mergedHistory,
            files: msg.files ?? s.files,
            nozzle_history,
            bed_history,
          };
        });

        if (Array.isArray(msg.events)) {
          events.set((msg.events as AppEvent[]).slice().reverse());
        }

        if (prevCameraConnected !== null) {
          if (prevCameraConnected && !nowCameraConnected) {
            showToast('Camera feed lost', 'warn', 6000);
          } else if (!prevCameraConnected && nowCameraConnected) {
            showToast('Camera feed restored', 'info');
          }
        }
        prevCameraConnected = nowCameraConnected;

        const nowMachineStatus = (msg.data as FullStatus)?.machine_status?.status;
        if (nowMachineStatus !== undefined) {
          const phaseInfo = msg.phase as { label: string; variant: string } | undefined;
          if (msg.connected === true && prevMachineStatus !== null && nowMachineStatus !== prevMachineStatus && phaseInfo?.variant === 'error') {
            showToast(`Printer: ${phaseInfo.label}`, 'error', 8000);
          }
          prevMachineStatus = nowMachineStatus;
        }

      } else if (msg.type === 'event' && msg.data) {
        const evt = msg.data as AppEvent;
        events.update((evts) => [evt, ...evts].slice(0, 20));
      } else if (msg.type === 'pong') {
        lastPongAt = Date.now();
      }
    } catch (err) {
      wsError.set(`Malformed server payload: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  ws.onclose = () => {
    ws = null;
    wsConnected.set(false);
    scheduleReconnect();
  };

  ws.onerror = () => {
    wsError.set('Connection error');
  };
}

function scheduleReconnect() {
  if (reconnectTimer !== null) return;

  reconnectAttempts++;
  const delay = Math.min(1000 * Math.pow(1.5, reconnectAttempts), 30000);

  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, delay);
}

export function disconnect() {
  if (reconnectTimer !== null) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  ws?.close();
  ws = null;
  wsConnected.set(false);
}

export function sendPing() {
  if (!ws) return;
  // state frames count as liveness, not just explicit pong
  const lastActivity = Math.max(lastPongAt, lastStateTs);
  if (lastActivity > 0 && Date.now() - lastActivity > 55_000) {
    ws.close();
    return;
  }
  if (lastStateTs > 0 && Date.now() - lastStateTs > 20_000) {
    wsStale.set(true);
  }
  ws.send(JSON.stringify({ type: 'ping' }));
}
