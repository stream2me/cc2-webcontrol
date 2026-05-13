# cc2-openwebui  -  Full Diagnosis & Remediation Plan

Cross-referenced against ElegooSlicer source (`src/slic3r/Utils/Elegoo/`) and the Python reference impl.
Status as of 2026-05-02.

---

## 0. Executive Summary

The stack has six structural defects that cascade into everything else:

1. **Both MQTT clients share one state object**  -  concurrent merges, double-seeding, stale fields
2. **No normalization layer**  -  raw `machine_status.status + sub_status` integers go straight to UI/events; Elegoo normalizes first
3. **`recursive_merge` never clears fields**  -  print task data survives idle transitions
4. **Commands are fire-and-forget with id:0**  -  silently dropped before registration, no ack correlation
5. **`event_tx` is never populated**  -  real-time events never reach the browser; `events` omitted from WS payload
6. **Phase logic is split across three surfaces**  -  WS toasts, PrintHeader, Controls each do their own thing

The result: flicker at phase transitions, stale filename/progress after a print ends, phantom pause states, dropped commands on reconnect, a dead event feed in the UI, and inconsistent error badge colors.

---

## 1. Dual-Stream State Merging

**Files:** `src/printer/client_raw.rs`, `src/printer/client_ws.rs`, `src/printer/manager.rs`

### What's wrong

Both clients subscribe to `elegoo/{printer_id}/api_status` (method 6000 status push) and both call `state.write().await.merge_delta(result)` concurrently. Both also call `seed()` on `METHOD_GET_FULL_STATUS` response  -  raw client on registration (lines 163–166 of `client_raw.rs`), WS client on registration (line 204–205 of `client_ws.rs`). Two races:

- Two concurrent `merge_delta()` calls on the same `PrinterState` lock → whichever wins last sets the final value; the other's delta is applied to possibly-stale intermediate state
- Two `seed()` calls → the second always wins and discards any in-between deltas the first client had already merged

### Reference: what Elegoo does

Single network layer (`ElegooLink` SDK singleton). One event pipeline: `PrinterStatusEvent` → `parseElegooStatus()` → `PrinterCache::updatePrinterStatus()`. No concurrent writers.

### Fix

Make one client own telemetry. Correct split:

| Client | Port | Owns |
|--------|------|------|
| `client_raw` | 1883 | Registration handshake only; `start_print` TX only; **no** `api_status` subscription, **no** `merge_delta`, **no** `seed` |
| `client_ws` | 9001 | All telemetry: `api_status` subscribe → `merge_delta`; `api_response` → `seed`, thumbnail, AMS; all interactive commands |

Remove from `client_raw.rs`:
- `api_status_topic` subscribe
- `merge_delta` on `METHOD_STATUS_PUSH`
- `seed` on `METHOD_GET_FULL_STATUS`
- `METHOD_GET_AMS_INFO` on registration

Raw client only needs: register → get_device_info → expose raw_cmd_tx for start_print.

---

## 2. Missing Normalization Layer

**Files:** `src/printer/state.rs`, `src/printer/models.rs`, `frontend/src/lib/printerStatus.ts`

### What's wrong

Our codebase pipes raw `machine_status.status` (i64) and `machine_status.sub_status` (i64) everywhere. `machine_phase_label_ctx()` is a partial workaround but:

- `sub_status` is **never read** outside the model struct. It is captured but never used anywhere in state logic, phase badges, or event logging
- `machine_phase_label_ctx` only handles two edge cases (code 1 and 10)  -  it ignores that `sub_status` overrides the main status for pausing/paused/completed
- `print_status.state` ("printing"/"paused") and `machine_status.status` (1/2/3) can diverge; there is no single canonical view

### Reference: `ElegooLink.cpp` lines 70–105

```cpp
PrinterStatus parseElegooStatus(elink::PrinterState mainStatus, elink::PrinterSubState subStatus) {
    // ...map mainStatus to PrinterStatus...
    switch (subStatus) {
    case elink::PrinterSubState::P_PAUSING:           printerStatus = PRINTER_STATUS_PAUSING; break;
    case elink::PrinterSubState::P_PAUSED:            printerStatus = PRINTER_STATUS_PAUSED; break;
    case elink::PrinterSubState::P_PRINTING_COMPLETED:printerStatus = PRINTER_STATUS_PRINT_COMPLETED; break;
    default: break;
    }
    return printerStatus;
}
```

`sub_status` is the final authority for pausing/paused/completed  -  it overrides the main status enum.

### Fix: Add a `normalize_machine_status` function to `state.rs`

```rust
pub enum NormalizedStatus {
    Offline, Idle, Printing, Pausing, Paused, PrintCompleted, Canceled,
    SelfChecking, AutoLeveling, PidCalibrating, ResonanceTesting, Updating,
    FileCopying, FileTransferring, Homing, Preheating, FilamentOperating,
    ExtruderOperating, RfidRecognizing, VideoComposing, EmergencyStop,
    PowerLossRecovery, Initializing, Busy, Error, IdNotMatch, AuthError, Unknown,
}

pub fn normalize_machine_status(status: i64, sub_status: i64) -> NormalizedStatus {
    // Apply sub_status overrides first (matches Elegoo's parseElegooStatus logic)
    let from_sub = match sub_status {
        1 => Some(NormalizedStatus::Pausing),
        2 => Some(NormalizedStatus::Paused),
        3 => Some(NormalizedStatus::PrintCompleted),
        _ => None,
    };
    if let Some(s) = from_sub {
        return s;
    }
    match status {
        -1   => NormalizedStatus::Offline,
        0    => NormalizedStatus::Idle,
        1    => NormalizedStatus::Printing,
        // ... etc
    }
}
```

`NormalizedStatus` replaces every raw `i64` code passed around. `machine_phase_label_ctx` becomes `NormalizedStatus::label()`. The phase table in `printerStatus.ts` is derived from this same enum.

Frontend `printerStatus.ts` needs a matching `SubStatus` constant map and must call `resolvePhase` with `sub_status` included.

---

## 3. Stale Field Retention After Print End

**Files:** `src/printer/state.rs` (`recursive_merge`, `merge_delta`)

### What's wrong

`recursive_merge` only overwrites keys present in the delta. When the printer goes idle, the firmware sends a delta with `machine_status.status = 0` but often omits `print_status.filename`, `print_status.state`, `print_status.progress`, etc. The old print task data lingers in `PrinterState.full.print_status`.

Symptoms:
- PrintHeader still shows the last filename after a print finishes
- Thumbnail image persists
- Pause/Stop buttons remain visible after print ends (they gate on `print_status.state`)
- `events.log` shows "Print started: <old_filename>" on next boot because stale filename survives restart via seed

### Reference: `PrinterCache.cpp` lines 152–168

```cpp
void PrinterCache::updatePrinterStatus(const std::string& printerId, const PrinterStatus& status) {
    if(status != PRINTER_STATUS_PRINTING && status != PRINTER_STATUS_PAUSED && status != PRINTER_STATUS_PAUSING) {
        it->second.printTask.taskId = "";
        it->second.printTask.fileName = "";
        it->second.printTask.totalTime = 0;
        // ...
    }
}
```

Elegoo explicitly zeroes task fields when not in a print state.

### Fix

In `merge_delta()`, after merging, call `clear_print_task_if_idle()`:

```rust
fn clear_print_task_if_idle(&mut self) {
    let norm = normalize_machine_status(
        self.full.machine_status.status,
        self.full.machine_status.sub_status,
    );
    match norm {
        NormalizedStatus::Printing | NormalizedStatus::Pausing | NormalizedStatus::Paused => {}
        _ => {
            self.full.print_status.filename = String::new();
            self.full.print_status.state = String::new();
            self.full.print_status.current_layer = None;
            self.full.print_status.remaining_time_sec = None;
            self.full.print_status.print_duration = None;
            self.full.print_status.uuid = String::new();
        }
    }
}
```

Also clear `PrintState::print_state()` to return `Idle` when filename is empty, regardless of `print_status.state` string.

---

## 4. Command Delivery  -  Silent Drops, No Acknowledgment

**Files:** `src/printer/client_ws.rs` (lines 161–175), `src/printer/manager.rs`, `src/api/printer.rs`

### 4a. Pre-registration drop

When a command arrives in `cmd_rx` before `registered = true`:
```rust
if !registered {
    debug!("[ws-cmd] dropping method {method} - not yet registered");
}
```
The command is silently discarded. `ws_cmd_tx.send()` succeeds (returns Ok) because the channel is alive. `manager.pause()` returns `Ok(())`. The API returns `{ "status": "queued" }`. The print never pauses.

**Fix:** Add a pre-registration command queue. On registration, drain it before entering the main loop. Max queue depth: 4 commands.

### 4b. id:0  -  no ack correlation

Pause/resume/stop/LED/fan/speed/home/jog all use `id: 0`. The printer's `api_response` for these commands replies with `id: 0` too, so we can't tell which command got acked or if it failed.

```rust
// all control commands today:
Command { id: 0, method: METHOD_PAUSE_PRINT, params: None }
```

The printer does return `api_response` with error_code for control commands. We process responses in `handle_publish` but only for ids > 0 (line 319: `if resp.id > 0`).

**Fix:** Use `rpc_call()` for pause/resume/stop instead of fire-and-forget. These are critical operations. The manager already has `rpc_call()` working correctly for history/thumbnail/canvas. Add a `rpc_call_no_result()` variant that awaits the ack without needing the payload. For LED/fan/speed, fire-and-forget is acceptable since they're non-critical.

### 4c. start_print goes via raw channel

`start_print` uses `raw_cmd_tx`. After fix 1 (raw client owns only registration), `start_print` must move to `ws_cmd_tx`. This matches the protocol  -  start_print is just an RPC request, not special-cased.

### 4d. get_file_list is a lie

```rust
pub async fn get_file_list(...) -> Result<serde_json::Value, PrinterError> {
    self.ws_cmd_tx.send(Command { id: 0, method: METHOD_GET_FILE_LIST, ... })?;
    Ok(serde_json::json!({ "ok": true }))  // ← lie
}
```

The actual list arrives later in `handle_publish` → `state.files`. The API endpoint returns fake `{ "ok": true }`. The frontend gets the list on the next WS push.

**Fix:** Use `rpc_call(METHOD_GET_FILE_LIST, ...)` to return the actual list synchronously. The `handle_publish` path for file list can stay as a cache-warmer but the API endpoint should return real data.

---

## 5. Event Feed is Dead

**Files:** `src/printer/manager.rs`, `src/api/ws.rs`, `frontend/src/ws.ts`

### What's wrong

`PrinterManager.event_tx` is a `broadcast::Sender<PrinterEvent>` that is **never sent to**. There are zero `self.event_tx.send()` calls in `manager.rs`. The WS server in `api/ws.rs` subscribes to `event_rx` and sends `type: "event"` messages  -  but since no events are ever sent, this path is dead.

Furthermore, `build_state_msg()` does not include an `"events"` key, despite the AGENT.md protocol spec saying it should. The frontend `ws.ts` has a handler for `msg.type === 'event'` that updates the `events` store  -  which never fires.

The events panel in the UI is probably empty on fresh page loads, or only populated if there's a REST endpoint called at startup.

### Fix

Two parts:

**Part A  -  populate event_tx:** When `add_event()` is called, also broadcast on `event_tx`:
```rust
pub fn add_event(&mut self, kind: EventKind, description: String) {
    // ... dedup, persist, push ...
    // notify WS clients of the new event
    let _ = self.event_tx_handle.send(e.clone());
}
```
`PrinterState` needs a clone of `event_tx` (or the event is forwarded by the manager after `merge_delta` returns).

**Part B  -  include recent events in state message:**
```rust
fn build_state_msg(s: &PrinterState) -> serde_json::Value {
    json!({
        // ...existing fields...
        "events": &s.events[s.events.len().saturating_sub(20)..],
    })
}
```
On initial connect, the FE gets the last 20 events. Subsequent `type: "event"` pushes append to the list.

---

## 6. Phase Logic Split Across Three Surfaces

**Files:** `frontend/src/ws.ts`, `frontend/src/lib/PrintHeader.svelte`, `frontend/src/lib/printerStatus.ts`

### What's wrong

Three different surfaces derive the current phase differently:

| Surface | Method | Problem |
|---------|--------|---------|
| `ws.ts` error toast | `getPhaseLabel(nowMachineStatus)` | No `printState` context; code 1 during idle gives "Printing" toast |
| `PrintHeader.svelte` badge | `resolvePhase({ status, printState, recentAction })` | Correct, but `recentAction` expires client-side only; lost on WS reconnect |
| `Controls.svelte` | `setRecentAction()` | Only set on user button click; auto-home/jog from backend has no context |
| `ws.ts` reset | `prevMachineStatus = null` on `onopen` | Loses transition baseline; first status after reconnect triggers no-transition toast |

Additionally, `sub_status` is never fed into `resolvePhase`. The `Pausing` sub_status (1) is invisible to the UI  -  badge shows "Printing" while the printer is already decelerating to pause.

### Fix

1. **`ws.ts` toast:** Replace `getPhaseLabel(nowMachineStatus)` with `resolvePhase({ status: nowMachineStatus, printState: msg.data?.print_status?.state ?? '', subStatus: msg.data?.machine_status?.sub_status ?? 0 })`. Only toast on `isErrorPhase`.

2. **`resolvePhase` signature:** Add `subStatus: number` param. Apply sub_status override before main status lookup:
   ```typescript
   if (subStatus === 1) return PHASES[3]; // Pausing
   if (subStatus === 2) return PHASES[2]; // Paused
   if (subStatus === 3) return PHASES[16]; // Print Completed
   ```

3. **`recentAction` reset:** Don't reset on `ws.onopen`. Only expire via `expiresAt`. The 8-second window is fine; a reconnect under 8s preserves context.

4. **Add Pausing badge variant:** Currently `phaseVariant = 'paused'` for both paused and pausing. Add `'pausing'` with a distinct animation or color to show the printer is mid-deceleration.

---

## 7. Notification Toggle Mapping Bug

**Files:** `src/notifications/manager.rs`, `src/config.rs`

### What's wrong

```rust
// event_kind_label():
EventKind::PhaseChanged(code, _) => match code {
    19 => "emergency_stop",
    999 => "machine_error",
    1000 | 1001 => "auth_error",   // ← bug: maps 1000 to auth_error
    _ => "other",
},

// event_matches_toggles():
EventKind::PhaseChanged(code, _) => match code {
    19 => t.emergency_stop,
    999 => t.machine_error,
    1000 | 1001 => t.auth_error,   // ← bug: ignores t.id_not_match
    _ => false,
},
```

`EventToggles` has both `id_not_match` and `auth_error` as separate fields (both configurable in the settings UI). But code 1000 ("ID Not Match") is mapped to `auth_error` instead of `id_not_match`. The `id_not_match` toggle never does anything.

### Fix

```rust
// event_kind_label():
1000 => "id_not_match",
1001 => "auth_error",

// event_matches_toggles():
1000 => t.id_not_match,
1001 => t.auth_error,
```

---

## 8. exception_status Is an Opaque Blob

**Files:** `src/printer/models.rs`, `src/printer/state.rs`, `src/notifications/payload.rs`

### What's wrong

```rust
pub struct MachineStatus {
    pub exception_status: Option<Vec<serde_json::Value>>,  // never read
    pub sub_status_reason_code: Option<i64>,                // never read
    ...
}
```

The printer sends structured error data in `exception_status` (array of objects with error codes, descriptions, recovery actions). We capture it as `Vec<serde_json::Value>` and never look at it. `sub_status_reason_code` is also captured but ignored.

Symptoms:
- When the printer enters error state 999, we know it's an error but not *which* error
- No notification body contains the actual error description
- Logs just say "Phase: Error (code 999)"
- Filament runout, bed sensor failure, nozzle clog  -  all look identical in the UI

### Fix

**Part A  -  Model:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExceptionEntry {
    pub code: i64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub recovery: Option<String>,
}
```
Replace `Vec<serde_json::Value>` with `Vec<ExceptionEntry>` (with fallback deserialization).

**Part B  -  Surface in events:**
When `merge_delta` detects a new exception entry, log it as `EventKind::ErrorOccurred(description)` with the error code and description.

**Part C  -  Surface in notifications:**
Include exception descriptions in the `PhaseChanged(999, _)` notification body:
```
title: "Printer Error"
body:  "Error 0x1234: Filament runout on extruder 1"
```

**Part D  -  Surface in UI:**
Add an error details row in the phase badge area when `exception_status` is non-empty. Small red icon with the error description on hover/tap.

---

## 9. State Broadcast Flooding

**Files:** `src/printer/state.rs`, `src/printer/client_ws.rs`, `src/api/ws.rs`

### What's wrong

Every `merge_delta()` call fires `state_changed_tx.send(())`. During active printing, the printer pushes status deltas roughly every 1–2 seconds (or faster during homing/moves). Each fires:
- `state_changed_tx.send(())`  -  N subscribers wake
- WS server reads full state, JSON-serializes it (potentially 5–10 KB), sends to all browser connections
- Notification manager `process_new_events()` runs  -  acquires two read locks

This is fine for 1 browser and 1 printer. It will degrade with slow connections or high-frequency deltas. More importantly, it's architecturally wrong: the browser doesn't need 60 state pushes per minute if only temperature changed.

### Fix (short-term)

Add a 200ms debounce on `state_changed_tx`. Instead of firing immediately, arm a `tokio::time::sleep(200ms)` and reset it on each merge. Fire at the end of the debounce window.

```rust
// in merge_delta():
self.pending_state_changed = true;
// A background task polls pending_state_changed and debounces
```

Or simpler: cap at 4 state broadcasts per second using a semaphore or rate-limiter token bucket.

### Fix (long-term)

Diff the serialized state before/after merge. Only broadcast when meaningful fields changed (status, temperatures, progress). Skip broadcast for pure PING/PONG.

---

## 10. WS Session Refresh Forces Unnecessary Reconnect

**File:** `src/printer/client_ws.rs` (line 126)

### What's wrong

```rust
let session_refresh = tokio::time::sleep(Duration::from_secs(8 * 60));
```

Every 8 minutes, the WS client returns `Ok(())`, manager sees no error, and reconnects. During reconnect:
- `ws_connected = false` for the duration of the handshake (typically <1s)
- `connected = false` → browser gets a disconnected state push
- WS client re-seeds full status → state_changed broadcast
- Any in-flight RPC calls are drained (pending_rpcs cleared)

This is unnecessary unless the printer firmware has a session limit. The existing MQTT keepalive (60s) and PING heartbeat (10s) are sufficient to detect a dead connection.

### Fix

Remove the 8-minute session_refresh timer. Rely on the heartbeat + tungstenite's built-in websocket ping/pong for connection health detection. If the printer firmware does have a session timeout, the heartbeat already resets it.

---

## 11. Reconnect Does Not Re-Request Files

**File:** `src/printer/client_ws.rs` (registration handler, ~lines 204–215)

### What's wrong

On registration, the WS client requests full status and AMS info:
```rust
// METHOD_GET_FULL_STATUS
// METHOD_GET_AMS_INFO
```
But it does **not** request the file list (`METHOD_GET_FILE_LIST`). After a reconnect, `state.files` retains whatever was cached before. If files changed on the printer while disconnected, the file list is stale.

### Fix

Add `METHOD_GET_FILE_LIST` request on registration, after the full status request:
```rust
let req_files = serde_json::json!({"id": id_seq, "method": METHOD_GET_FILE_LIST, 
    "params": {"storage_media": "local", "offset": 0, "limit": 50}});
```

---

## 12. Heartbeat on Both Clients  -  Redundant and Confusing

**Files:** `src/printer/client_raw.rs` (line 97), `src/printer/client_ws.rs` (line 125)

### What's wrong

Both clients send `{"type": "PING"}` to `elegoo/{pid}/{client_id}/api_request` every 10 seconds. After fix 1 (raw client no longer subscribes to `api_status`), the raw client heartbeat serves no purpose  -  it's just noise on the MQTT bus.

### Fix

After decoupling raw from telemetry (fix 1), remove the heartbeat from `client_raw`. The WS client heartbeat is sufficient and can stay.

---

## 13. PrinterManager.event_tx Is Public But Dead

**File:** `src/printer/manager.rs` (line 23)

### What's wrong

```rust
pub event_tx: broadcast::Sender<PrinterEvent>,
```

This field is `pub`, subscribed to by the WS handler, but never sent to. It was presumably planned for real-time event push but never wired up. All events go through `add_event()` → `state.events` → state_changed_tx. The event_tx pipeline is unused.

### Fix

Wire it up (see fix 5). Make `PrinterState.add_event()` also send on `event_tx`. To avoid giving `PrinterState` a sender, the manager can forward: after any operation that calls `add_event`, also send the resulting event on `event_tx`.

Simplest path: pass `event_tx.clone()` into `PrinterState::new()` and call it inside `add_event()`.

---

## 14. `sub_status_reason_code` Never Used

**File:** `src/printer/models.rs`

`MachineStatus.sub_status_reason_code` is captured but never read. This field contains the reason for the current sub-state (e.g., why the print paused). Should be:
- Logged in `PhaseChanged` event description
- Included in notification body when sub_status changes
- Surfaced in UI error details

---

## 15. PrintStatus.uuid Not Used for Task Correlation

**File:** `src/printer/models.rs`, `src/printer/state.rs`

`PrintStatus.uuid` is the firmware-assigned task UUID. It could be used to:
- Correlate print history entries with live status
- Deduplicate `PrintStarted` events across reconnects (if uuid matches, don't re-fire)
- Key the thumbnail cache instead of filename (safer  -  filenames can collide)

Currently ignored.

---

## 16. `PrintState::Idle` Doesn't Cover `PrintCompleted`

**File:** `src/printer/state.rs` (line 167)

```rust
pub fn print_state(&self) -> PrintState {
    match self.full.print_status.state.as_str() {
        "printing" => PrintState::Printing,
        "paused"   => PrintState::Paused,
        _          => PrintState::Idle,
    }
}
```

When a print completes, the firmware sets `machine_status.status = 16` (Print Completed) but `print_status.state` may briefly linger as "printing" before being cleared. During this window:
- `print_state()` returns `Printing`
- `record_state_transition` logs `PrintFinished` from `(Printing → Idle)`  -  fine
- But if a second delta arrives still with "printing", nothing logs

After fix 3 (clear print fields on idle), this will improve. But `PrintState` should also have a `Completed` variant to properly distinguish from `Idle`.

---

## 17. Frontend: actionCtxStore Reset on Reconnect

**File:** `frontend/src/ws.ts` (line 39)

```typescript
ws.onopen = () => {
    // ...
    prevMachineStatus = null;
};
```

`prevMachineStatus = null` on reconnect means the first status message after reconnect never triggers a phase transition toast (correct). But `actionCtxStore` is not reset  -  if the user had clicked Home 5 seconds before a WS reconnect and the action expires at T+8s, the context is still valid post-reconnect. This is fine.

However, there is a subtler issue: `prevMachineStatus` being reset to null means if the printer was in error state before disconnect, the first message after reconnect won't fire the error toast (since there's no prev to compare to). The user reconnects and sees an error badge but gets no toast.

### Fix

On `ws.onopen`, snapshot the current error state from the store. After the first message arrives, compare against snapshot rather than null. If the printer is still in error and we know it (because the badge was showing error before disconnect), suppress the toast. If it transitioned into error while we were disconnected, fire the toast.

---

## 18. Backend: `get_file_list` API Returns False Data

**File:** `src/printer/manager.rs` (lines 431–444), `src/api/printer.rs` (lines 186–197)

```rust
pub async fn get_file_list(...) -> Result<serde_json::Value, PrinterError> {
    self.ws_cmd_tx.send(Command { id: 0, method: METHOD_GET_FILE_LIST, ... })?;
    Ok(serde_json::json!({ "ok": true }))  // ← not the file list
}
```

The HTTP response claims success but returns no data. The caller (`api/printer.rs::get_files`) returns this to the frontend. The frontend presumably ignores it and waits for the WS state push.

This is a broken API contract. `/api/printer/files` should return the actual file list.

### Fix

Use `rpc_call(METHOD_GET_FILE_LIST, ...)` and parse the response. Cache in `state.files`. Return the actual array from the API endpoint.

---

## 19. Detection Score Leaks Into Idle State Messages

**File:** `src/api/ws.rs` (line 158), `src/printer/state.rs`

`build_state_msg` always includes `detection_score` in the WS payload, even when the printer is idle and detection is disabled/not running. A non-zero score from the last print session appears in the idle state message, which can confuse clients that gate detection UI visibility on the score.

### Fix

Zero out `detection_score` in `PrinterState` when transitioning to idle (alongside the print task clearing in fix 3). Or exclude it from the WS message when `detection_score == 0.0` and no print is active.

---

## 20. Disconnected Sentinel Code (-1) Is Fictional

**File:** `frontend/src/lib/PrintHeader.svelte` (line 22), `frontend/src/lib/printerStatus.ts`

```typescript
$: phaseCode = $printer.connected ? (machineStatus?.status ?? 0) : -1;
```
```typescript
[-1]: { label: 'Offline', variant: 'error' },
```

Code -1 is a frontend-only sentinel  -  the firmware never sends -1. This is reasonable, but it means `isErrorPhase(-1)` returns `true` (since variant is 'error' and status ≠ -1... wait, `isErrorPhase` checks `status !== -1`):

```typescript
export function isErrorPhase(status: number): boolean {
  return getPhaseInfo(status).variant === 'error' && status !== -1;
}
```

So -1 correctly doesn't trigger toasts. Fine. But the WS toast code in `ws.ts` reads `(msg.data as FullStatus)?.machine_status?.status`  -  if disconnected, this is 0 or whatever the last known value was, **not** -1. The disconnected state is only synthesized in `PrintHeader.svelte`. The WS toast can fire incorrectly on disconnect if the last status was an error code.

### Fix

Include a `connected` flag check in the WS toast logic:
```typescript
if (msg.connected && prevMachineStatus !== null && nowMachineStatus !== prevMachineStatus && isErrorPhase(nowMachineStatus)) {
    showToast(...);
}
```
Only toast when connected. This prevents disconnect-flicker toasts.

---

## 21. Logs API Doesn't Map EventKind Properly for UI

**File:** `src/api/logs.rs`, `src/printer/state.rs` (lines 260–270)

When loading events from `data/events.log`, all events are reconstructed with `EventKind::Loaded(string)`. The string is the debug-printed name (e.g., `"PrintStarted"`, `"PhaseChanged(1,Printing)"`). The frontend receives these as `kind: "Loaded(PrintStarted)"` which breaks any kind-based filtering or icons.

### Fix

In `persist_event`, serialize `kind` as a canonical JSON-friendly string (not Rust debug format):
```rust
let kind_str = match &e.kind {
    EventKind::PrintStarted => "print_started",
    EventKind::PrintFinished => "print_finished",
    EventKind::PhaseChanged(code, _) => ...,
    // ...
};
```

In `load_events_from_log`, reconstruct with a proper discriminant-matched `Loaded(kind_str)` that the FE can parse. Or better: send a typed JSON object per event so the FE can render icons/colors properly without reverse-engineering the string.

---

## 22. WS Handler Uses Mutex for Sender  -  Unnecessary Contention

**File:** `src/api/ws.rs` (lines 34, 51, 67)

```rust
let sender = Arc::new(Mutex<ws_sender>);
```

Both the send_task and the ping-reply path lock the same Mutex. The send_task holds it while serializing + sending each state message. This isn't wrong but adds latency to ping replies. Use `tokio::sync::Mutex` (already is) but consider moving to a single-writer architecture: have the receiver task just forward pings to a `mpsc::Sender` that the send_task processes, eliminating concurrent lock contention.

---

## Remediation Priority Order

### Critical (breaks correctness)

1. **Fix 1**  -  Decouple raw client from telemetry (eliminate dual-merge race)
2. **Fix 3**  -  Clear print fields on non-print status transitions
3. **Fix 4a**  -  Pre-registration command queue (no more silent drops)
4. **Fix 4b**  -  Use rpc_call for pause/resume/stop
5. **Fix 5**  -  Wire event_tx and include events in WS state message
6. **Fix 7**  -  Notification toggle 1000/1001 mapping

### High (visible bugs)

7. **Fix 2**  -  Normalization layer with sub_status (Pausing state, PrintCompleted)
8. **Fix 6**  -  Unify phase logic: ws.ts toast uses resolvePhase with sub_status
9. **Fix 8**  -  Parse exception_status into typed struct; surface in notifications/UI
10. **Fix 18**  -  get_file_list returns real data via rpc_call

### Medium (stability)

11. **Fix 9**  -  Debounce state broadcasts (200ms)
12. **Fix 10**  -  Remove 8-minute session refresh
13. **Fix 11**  -  Re-request file list on reconnect
14. **Fix 12**  -  Remove redundant raw client heartbeat
15. **Fix 20**  -  Gate WS error toasts on connected=true

### Low (polish)

16. **Fix 4c**  -  Move start_print to WS channel
17. **Fix 15**  -  Use print uuid for thumbnail cache key
18. **Fix 16**  -  Add PrintState::Completed variant
19. **Fix 19**  -  Clear detection_score on idle
20. **Fix 21**  -  Structured event kind serialization for logs

---

## File Change Map

| File | Changes Needed |
|------|---------------|
| `src/printer/client_raw.rs` | Remove api_status subscribe, merge_delta, seed, AMS request; keep only register + start_print TX |
| `src/printer/client_ws.rs` | Remove 8min session_refresh; add pre-reg command queue; request file list on registration |
| `src/printer/state.rs` | Add `NormalizedStatus` enum + `normalize_machine_status()`; add `clear_print_task_if_idle()`; call it in `merge_delta`; wire `event_tx` into `add_event` |
| `src/printer/models.rs` | Add `ExceptionEntry` struct; replace `Vec<serde_json::Value>` in `MachineStatus.exception_status` |
| `src/printer/manager.rs` | Use rpc_call for pause/resume/stop; move start_print to ws_cmd_tx; fix get_file_list to use rpc_call |
| `src/api/ws.rs` | Add `"events"` key to `build_state_msg`; add connected-check to toast logic |
| `src/notifications/manager.rs` | Fix 1000→id_not_match, 1001→auth_error mapping |
| `src/notifications/payload.rs` | Include exception_status description in PhaseChanged(999) body |
| `frontend/src/lib/printerStatus.ts` | Add `subStatus` param to `resolvePhase`; add sub_status override block; add Pausing variant |
| `frontend/src/ws.ts` | Use `resolvePhase` for error toast; gate toast on `msg.connected`; don't reset `prevMachineStatus = null` on reconnect if last state was error |
| `frontend/src/lib/PrintHeader.svelte` | Pass `subStatus` to `resolvePhase`; add Pausing badge style |
