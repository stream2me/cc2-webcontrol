<script context="module" lang="ts">
  export interface PrintOptions {
    filename: string;
    plate: 'textured' | 'smooth';
    selectedTrayId: number | null;
    selectedSlotIndex: number | null;
    selectedCanvasId: number;
    timelapse: boolean;
    heatedBedLevel: boolean;
  }
</script>

<script lang="ts">
  import Modal from './Modal.svelte';
  import { printer } from '../stores';
  import type { PrinterFile } from '../stores';
  import { getThumbnail, getFileDetail, type FileDetail } from '../api';
  import plateTextured from '../assets/cc2_build_plate_A_textured_ui_bt_pte.png';
  import plateSmooth from '../assets/cc2_build_plate_B_smooth_ui_bt_pc.png';

  export let open: boolean;
  export let onClose: () => void;
  export let file: PrinterFile | null = null;
  export let onPrint: (opts: PrintOptions) => void;

  let plate: 'textured' | 'smooth' = 'textured';
  let timelapse = false;
  let heatedBedLevel = true;
  let selectedTrayId: number | null = null;
  let selectedSlotIndex: number | null = null;

  let thumbUri = '';
  let detail: FileDetail | null = null;
  let detailLoading = false;
  let thumbFailed = false;
  let thumbRetrying = false;
  let thumbRetryOk: boolean | null = null;

  $: if (open && file) {
    fetchDetail(file.filename ?? file.name ?? '');
  } else if (!open) {
    thumbUri = '';
    detail = null;
    thumbFailed = false;
    thumbRetryOk = null;
  }

  async function fetchDetail(filename: string) {
    if (!filename) return;
    detailLoading = true;
    thumbUri = '';
    thumbFailed = false;
    thumbRetryOk = null;
    detail = null;
    try {
      const [thumbRes, detailRes] = await Promise.allSettled([
        getThumbnail(filename),
        getFileDetail(filename),
      ]);
      if (thumbRes.status === 'fulfilled' && thumbRes.value.thumbnail) {
        thumbUri = `data:image/png;base64,${thumbRes.value.thumbnail}`;
      }
      if (detailRes.status === 'fulfilled') {
        detail = detailRes.value;
        // thumbnail can also come from file detail (method 1046)
        if (!thumbUri && detail.thumbnail) {
          thumbUri = `data:image/png;base64,${detail.thumbnail}`;
        }
      }
      if (!thumbUri) thumbFailed = true;
    } finally {
      detailLoading = false;
    }
  }

  async function retryThumb() {
    if (thumbRetrying || !file) return;
    thumbRetrying = true;
    thumbRetryOk = null;
    try {
      const filename = file.filename ?? file.name ?? '';
      const res = await getThumbnail(filename);
      if (res.thumbnail) {
        thumbUri = `data:image/png;base64,${res.thumbnail}`;
        thumbFailed = false;
        thumbRetryOk = true;
      } else {
        thumbRetryOk = false;
        setTimeout(() => { thumbRetryOk = null; }, 3000);
      }
    } catch {
      thumbRetryOk = false;
      setTimeout(() => { thumbRetryOk = null; }, 3000);
    } finally {
      thumbRetrying = false;
    }
  }

  // prefer file-detail metadata; list data is fallback only
  $: printTime = detail?.print_time ?? file?.print_time;
  $: totalLayer = detail?.total_layer ?? file?.total_layer ?? file?.layers ?? file?.layer;
  $: filamentUsed = detail?.total_filament_used ?? file?.total_filament_used;

  $: s = $printer.state;
  $: canvasInfo = (s as any)?.canvas_info;
  $: activeCanvasId = canvasInfo?.active_canvas_id ?? 0;
  $: activeTrayId = canvasInfo?.active_tray_id ?? -1;
  $: canvas = canvasInfo?.canvas_list?.find((c: any) => c.canvas_id === activeCanvasId)
    ?? canvasInfo?.canvas_list?.[0];
  $: trayList = canvas?.tray_list ?? [];

  type Spool = { trayId: number; slotIndex: number; slot: number; color: string | null; material: string; empty: boolean };

  $: spools = [0, 1, 2, 3].map<Spool>((i) => {
    const tray = trayList[i];
    if (!tray) return { trayId: -1, slotIndex: i, slot: i + 1, color: null, material: '-', empty: true };
    const rawColor = (tray.filament_color as string | undefined)?.trim();
    const hexOnly = rawColor?.startsWith('#') ? rawColor.slice(1) : rawColor;
    const color = hexOnly && hexOnly.length >= 6 ? `#${hexOnly.slice(0, 6)}` : null;
    // prefer filament_name over tray_type
    const material = (tray.filament_name || tray.tray_type || tray.filament_type || '-').toString() || '-';
    return { trayId: tray.tray_id ?? -1, slotIndex: i, slot: i + 1, color, material, empty: !color };
  });

  $: if (open && selectedTrayId === null && activeTrayId >= 0) {
    const match = spools.find(sp => sp.trayId === activeTrayId);
    if (match) {
      selectedTrayId = match.trayId;
      selectedSlotIndex = match.slotIndex;
    }
  }

  function reset() {
    plate = 'textured';
    timelapse = false;
    heatedBedLevel = true;
    selectedTrayId = null;
    selectedSlotIndex = null;
  }

  function handleClose() {
    reset();
    onClose();
  }

  function handlePrint() {
    if (!file) return;
    onPrint({
      filename: file.filename ?? file.name ?? '',
      plate,
      selectedTrayId,
      selectedSlotIndex,
      selectedCanvasId: activeCanvasId,
      timelapse,
      heatedBedLevel,
    });
    handleClose();
  }

  function formatTime(sec: number | undefined): string {
    if (!sec) return '--';
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function shortName(name: string | undefined): string {
    if (!name) return '--';
    return name.replace(/\.gcode$/i, '');
  }

  function labelColor(bg: string | null): string {
    if (!bg) return 'var(--muted)';
    const hex = bg.replace('#', '');
    if (hex.length < 6) return '#fff';
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6 ? '#111' : '#fff';
  }
</script>

<Modal {open} onClose={handleClose}>
  <div class="print-modal" role="dialog" aria-modal="true" aria-label="Start print">
    <div class="pm-header">
      <div class="pm-title">{shortName(file?.filename ?? file?.name)}</div>
      <button class="pm-close" on:click={handleClose} aria-label="Close">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M2 2l10 10M12 2L2 12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
        </svg>
      </button>
    </div>

    <div class="pm-body">
      {#if thumbUri || detailLoading || thumbFailed}
        <div class="pm-thumb-wrap">
          {#if thumbUri}
            <img src={thumbUri} alt="Print preview" class="pm-thumb" />
          {:else if detailLoading}
            <div class="pm-thumb-placeholder">
              <svg width="28" height="28" viewBox="0 0 28 28" fill="none" aria-hidden="true">
                <circle cx="14" cy="14" r="12" stroke="var(--border2)" stroke-width="1.5" fill="none"/>
                <path d="M9 14h10M14 9v10" stroke="var(--border2)" stroke-width="1.5" stroke-linecap="round" class="spin-fade"/>
              </svg>
            </div>
          {:else if thumbFailed}
            <div class="pm-thumb-placeholder pm-thumb-failed">
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true">
                <path d="M3 17l7-7 7 7M3 3l7 7 7-7" stroke="var(--border2)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
              <span>No preview</span>
              <button
                class="thumb-retry-btn"
                on:click={retryThumb}
                disabled={thumbRetrying}
                title="Retry thumbnail fetch"
              >
                {#if thumbRetrying}
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" class="spin">
                    <path d="M13.5 4.5A6 6 0 1 0 14 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" fill="none"/>
                    <path d="M10 4.5h3.5V1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                {:else if thumbRetryOk === false}
                  <svg width="12" height="12" viewBox="0 0 14 14" fill="none">
                    <path d="M2 2l10 10M12 2L2 12" stroke="var(--danger)" stroke-width="1.6" stroke-linecap="round"/>
                  </svg>
                {:else}
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                    <path d="M13.5 4.5A6 6 0 1 0 14 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" fill="none"/>
                    <path d="M10 4.5h3.5V1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                {/if}
              </button>
            </div>
          {/if}
        </div>
      {/if}

      <div class="pm-info">
        <div class="info-row">
          <span class="info-label">Time</span>
          <span class="info-val mono">{formatTime(printTime)}</span>
        </div>
        <div class="info-row">
          <span class="info-label">Filament</span>
          <span class="info-val mono">{filamentUsed != null ? `${(+filamentUsed).toFixed(1)} g` : '--'}</span>
        </div>
        <div class="info-row">
          <span class="info-label">Layers</span>
          <span class="info-val mono">{totalLayer ?? '--'}</span>
        </div>
      </div>

      <div class="pm-section">
        <div class="section-title">Build Plate</div>
        <div class="plate-cards">
          <button
            class="plate-card {plate === 'textured' ? 'active' : ''}"
            on:click={() => plate = 'textured'}
            title="Textured (Side A)"
          >
            <img src={plateTextured} alt="Textured plate (Side A)" class="plate-img" />
          </button>
          <button
            class="plate-card {plate === 'smooth' ? 'active' : ''}"
            on:click={() => plate = 'smooth'}
            title="Smooth (Side B)"
          >
            <img src={plateSmooth} alt="Smooth plate (Side B)" class="plate-img" />
          </button>
        </div>
      </div>

      <div class="pm-section">
        <div class="section-title">Filament <span class="section-sub">(Canvas slots)</span></div>
        {#if spools.every(s => s.empty)}
          <div class="no-spools">No Canvas data from printer</div>
        {:else}
          <div class="spools-row">
            {#each spools as sp}
              <button
                class="spool-btn {selectedTrayId === sp.trayId && !sp.empty ? 'selected' : ''} {sp.empty ? 'empty' : ''}"
                disabled={sp.empty}
                on:click={() => { if (!sp.empty) { selectedTrayId = sp.trayId; selectedSlotIndex = sp.slotIndex; } }}
                title={sp.empty ? `Slot ${sp.slot} - empty` : `${sp.material}`}
              >
                <div class="spool-dot" style="background:{sp.color ?? 'var(--border2)'}; color:{labelColor(sp.color)}">
                  {sp.slot}
                </div>
                <div class="spool-meta">
                  <span class="spool-mat">{sp.material}</span>
                </div>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="pm-section pm-options">
        <div class="option-row">
          <div class="option-label">
            <span>Timelapse</span>
          </div>
          <label class="toggle">
            <input type="checkbox" bind:checked={timelapse} />
            <span class="knob"></span>
          </label>
        </div>
        <div class="option-row">
          <div class="option-label">
            <span>Heated Bed Leveling</span>
          </div>
          <label class="toggle">
            <input type="checkbox" bind:checked={heatedBedLevel} />
            <span class="knob"></span>
          </label>
        </div>
      </div>
    </div>

    <div class="pm-actions">
      <button class="pm-btn cancel" on:click={handleClose}>Cancel</button>
      <button class="pm-btn print" on:click={handlePrint}>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <path d="M3 2l9 5-9 5V2z" fill="currentColor"/>
        </svg>
        Print
      </button>
    </div>
  </div>
</Modal>

<style>
  .print-modal {
    background: var(--surface);
    border: 1px solid var(--border2);
    border-radius: 14px;
    width: 380px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    box-shadow: 0 24px 64px rgba(0,0,0,0.55);
  }

  .pm-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 20px 14px;
    border-bottom: 1px solid var(--border);
    gap: 12px;
  }

  .pm-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .pm-close {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--surface2);
    border: 1px solid var(--border);
    color: var(--muted);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    cursor: pointer;
  }
  .pm-close:hover { background: var(--border); color: var(--text); }

  .pm-body {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  /* info */
  .pm-thumb-wrap {
    width: 100%;
    border-radius: var(--radius);
    overflow: hidden;
    border: 1px solid var(--border);
    background: var(--surface2);
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 120px;
    max-height: 200px;
  }
  .pm-thumb {
    width: 100%;
    max-height: 200px;
    object-fit: contain;
    display: block;
  }
  .pm-thumb-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-height: 120px;
    opacity: 0.4;
  }
  .pm-thumb-failed {
    flex-direction: column;
    gap: 6px;
    opacity: 1;
    color: var(--muted2);
    font-size: 11px;
  }
  .thumb-retry-btn {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--surface);
    border: 1px solid var(--border2);
    color: var(--muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s, background 0.15s;
  }
  .thumb-retry-btn:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--border2);
    background: var(--surface3);
  }
  .thumb-retry-btn:disabled { opacity: 0.6; cursor: not-allowed; }
  .spin { animation: spin 0.9s linear infinite; transform-origin: center; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .spin-fade {
    animation: pulse-fade 1.2s ease-in-out infinite;
  }
  @keyframes pulse-fade {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  /* compact stats row keeps primary choices visible */
  .pm-info {
    display: flex;
    gap: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .info-row {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px 10px;
    gap: 3px;
  }
  .info-row + .info-row { border-left: 1px solid var(--border); }
  .info-label { font-size: 10px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .info-val { font-size: 14px; font-weight: 600; color: var(--text); }

  /* section */
  .pm-section { display: flex; flex-direction: column; gap: 8px; }
  .section-title { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .section-sub { font-size: 10px; text-transform: none; letter-spacing: 0; font-weight: 400; }

  /* plate pills */
  .plate-cards { display: flex; gap: 8px; }
  .plate-card {
    flex: 1;
    padding: 4px;
    border: 2px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface2);
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    overflow: hidden;
  }
  .plate-card:hover { border-color: var(--border2); background: var(--surface3, var(--surface2)); }
  .plate-card.active {
    border-color: var(--accent);
    background: var(--accent-dim);
  }
  .plate-img {
    width: 100%;
    height: auto;
    display: block;
    border-radius: 4px;
    object-fit: contain;
  }

  /* spools */
  .spools-row { display: flex; gap: 8px; }
  .spool-btn {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface2);
    cursor: pointer;
    transition: border-color 0.15s;
  }
  .spool-btn:hover:not(:disabled):not(.selected) { border-color: var(--border2); }
  .spool-btn.selected { border-color: var(--accent); background: var(--accent-dim); }
  .spool-btn.empty { opacity: 0.35; cursor: not-allowed; }

  .spool-dot {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    border: 2px solid rgba(255,255,255,0.15);
  }
  .spool-meta { display: flex; flex-direction: column; align-items: center; gap: 1px; }
  .spool-mat { font-size: 10px; font-weight: 600; color: var(--text); text-align: center; }

  .no-spools { font-size: 12px; color: var(--muted); font-style: italic; }

  /* options */
  .pm-options { gap: 10px; }
  .option-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .option-label { font-size: 13px; color: var(--text); }

  /* toggle */
  .toggle { position: relative; display: block; width: 36px; height: 20px; cursor: pointer; flex-shrink: 0; }
  .toggle input { opacity: 0; width: 0; height: 0; position: absolute; }
  .knob {
    position: absolute;
    inset: 0;
    background: var(--surface2);
    border: 1px solid var(--border2);
    border-radius: 20px;
    transition: background 0.2s;
  }
  .knob::before {
    content: '';
    position: absolute;
    width: 14px; height: 14px;
    left: 2px; top: 2px;
    background: var(--muted);
    border-radius: 50%;
    transition: transform 0.2s, background 0.2s;
  }
  input:checked + .knob { background: var(--surface2); border-color: var(--border2); }
  input:checked + .knob::before { transform: translateX(16px); background: var(--accent); }

  /* actions */
  .pm-actions {
    display: flex;
    gap: 10px;
    padding: 14px 20px 18px;
    border-top: 1px solid var(--border);
  }
  .pm-btn {
    flex: 1;
    padding: 10px 16px;
    border-radius: 8px;
    border: 1px solid;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    transition: filter 0.15s;
  }
  .pm-btn:hover { filter: brightness(1.15); }
  .pm-btn.cancel {
    background: var(--surface2);
    border-color: var(--border2);
    color: var(--text);
  }
  .pm-btn.print {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    flex: 2;
  }

  .mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }
</style>
