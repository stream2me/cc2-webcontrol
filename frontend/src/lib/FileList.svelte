<script lang="ts">
  import { onMount } from 'svelte';
  import { showToast } from '../stores';
  import { getFiles, getHistory, startPrint, uploadGcode, type HistoryTask } from '../api';
  import type { PrinterFile } from '../stores';
  import { toErrorMessage } from './errors';
  import PrintModal from './PrintModal.svelte';
  import type { PrintOptions } from './PrintModal.svelte';

  let activeTab: 'local' | 'udisk' | 'history' = 'local';
  let loading = false;
  let error = '';
  let usbError = '';
  let historyFiles: PrinterFile[] = [];
  let localFiles: PrinterFile[] = [];
  let usbFiles: PrinterFile[] = [];

  let localPath = '/';
  let usbPath = '/';

  let localOffset = 0;
  let localTotal = 0;
  let localMaxSite = 1;
  let localPageNumber = 1;
  let localPageSize = 10;
  let localPageHistory: Record<string, number> = {};

  let usbOffset = 0;
  let usbTotal = 0;
  let usbMaxSite = 1;
  let usbPageNumber = 1;
  let usbPageSize = 10;
  let usbPageHistory: Record<string, number> = {}

  let historyPageNumber = 1;
  let historyPageSize = 10;
  let historyTotal = 0;
  let historyMaxSite = 1;

  let printModalOpen = false;
  let printModalFile: PrinterFile | null = null;

  async function loadFiles() {
    loading = true;
    error = '';

    try {
      const result = await getFiles(
        'local',
        localPath,
        localPageNumber,
        localPageSize
      );

      localFiles = result.file_list;
      localOffset = result.offset;
      localTotal = result.total;
      localMaxSite = Math.max(1, Math.ceil(localTotal / localPageSize));
    } catch (e) {
      error = toErrorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function loadHistory() {
    loading = true;
    error = '';

    try {
      const res = await getHistory(
        historyPageNumber,
        historyPageSize
      );
      const list: HistoryTask[] = res.history_task_list ?? [];

      historyFiles = list.map((t) => ({
        ...t,
        filename: t.filename ?? t.task_name ?? '',
        name: t.name ?? t.task_name ?? '',
        create_time: t.create_time ?? t.begin_time ?? 0,
      })) as PrinterFile[];

      historyTotal = res.total ?? 0;

      historyMaxSite = Math.max(
        1,
        Math.ceil(historyTotal / historyPageSize)
      );

    } catch (e) {
      error = toErrorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function loadUsbFiles() {
    loading = true;
    usbError = '';

    try {
      const result = await getFiles(
        'u-disk',
        usbPath,
        usbPageNumber,
        usbPageSize
      );

      usbFiles = result.file_list;
      usbOffset = result.offset;
      usbTotal = result.total;
      usbMaxSite = Math.max(1, Math.ceil(usbTotal / usbPageSize));
    } catch (e) {
      usbError = toErrorMessage(e);
    } finally {
      loading = false;
    }
  }

  function switchTab(tab: typeof activeTab) {
    activeTab = tab;
    if (tab === 'local') loadFiles();
    else if (tab === 'udisk') loadUsbFiles();
    else loadHistory();
  }

  function openPrintModal(file: PrinterFile) {
    printModalFile = file;
    printModalOpen = true;
  }

  function refreshCurrentTab() {
    if (activeTab === 'local') {
      localPageNumber = 1;
      localPageHistory[localPath] = 1;
      loadFiles();
    } else if (activeTab === 'udisk') {
      usbPageNumber = 1;
      usbPageHistory[usbPath] = 1;
      loadUsbFiles();
    } else {
      historyPageNumber = 1;
      loadHistory();
    }
  }

  async function handlePrint(opts: PrintOptions) {
    if (!opts.filename) return;
    const storage = activeTab === 'udisk' ? 'u-disk' : 'local';
    try {
      await startPrint(opts.filename, storage, {
        plate: opts.plate,
        tray_id: opts.selectedTrayId,
        tray_slot: opts.selectedSlotIndex,
        canvas_id: opts.selectedCanvasId,
        timelapse: opts.timelapse,
        bedlevel_force: opts.heatedBedLevel,
      });
      showToast('Print started', 'info', 3000);
    } catch (e) {
      showToast(toErrorMessage(e), 'error', 6000);
    }
  }

  function formatSize(bytes: number): string {
    if (!bytes) return '--';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatDate(value: number | string | null | undefined): string {
    if (value == null || value === '')
      {return '--';
    }

    let date: Date;

    if (typeof value === 'number') {
      date = new Date(value * 1000);
    } else {
      const numeric = Number(value);

      if (!Number.isNaN(numeric) && value.trim() !== '') {
        date = new Date(numeric * 1000);
      } else {
        date = new Date(value);
      }
    }

    if (Number.isNaN(date.getTime())) {
      return '--';
    }

    return date.toLocaleDateString(undefined, {
      day: 'numeric',
      month: 'short',
      year: 'numeric'
    });
  }

  function shortName(name: string): string {
    if (!name) return '--';
    const base = name.replace(/\.gcode$/i, '');
    return base.length > 40 ? '...' + base.slice(-39) : base;
  }

  let collapsed = true;
  let uploading = false;
  let uploadInput: HTMLInputElement;

  async function handleUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    uploading = true;
    error = '';
    try {
      await uploadGcode(file);
      showToast(`Uploaded ${file.name}`, 'info');
      await loadFiles();
    } catch (e) {
      showToast(toErrorMessage(e));
    } finally {
      uploading = false;
      input.value = '';
    }
  }

  function joinPath(base: string, name: string): string {
    if (base === '/') {
      return `/${name}/`;
    }

    return `${base.replace(/\/+$/, '')}/${name}/`;
  }

  function parentPath(path: string): string {
    if (path === '/') {
      return '/';
    }

    const parts = path.split('/').filter(Boolean);
    parts.pop();

    return parts.length === 0
      ? '/'
      : `/${parts.join('/')}/`;
  }

  async function openFolder(file: PrinterFile) {
    if (file.type !== 'folder') {
      return;
    }

    if (activeTab === 'local') {
      localPageHistory[localPath] = localPageNumber;
      localPath = joinPath(localPath, file.filename);
      localPageNumber = localPageHistory[localPath] || 1;
      await loadFiles();
    } else if (activeTab === 'udisk') {
      usbPageHistory[usbPath] = usbPageNumber;
      usbPath = joinPath(usbPath, file.filename);
      usbPageNumber = usbPageHistory[usbPath] || 1;
      await loadUsbFiles();
    }
  }

  onMount(() => {
    loadFiles();
  });
</script>

<PrintModal
  open={printModalOpen}
  onClose={() => { printModalOpen = false; printModalFile = null; }}
  file={printModalFile}
  onPrint={handlePrint}
/>

<div class="card">
  <div class="card-header" role="button" tabindex="0" on:click={() => collapsed = !collapsed} on:keydown={(e) => e.key === 'Enter' && (collapsed = !collapsed)}>
    <span class="card-title">Files</span>
    <div class="header-right">
      {#if activeTab === 'local'}
        <input
          bind:this={uploadInput}
          type="file"
          accept=".gcode,.gco"
          style="display:none"
          on:change={handleUpload}
        />
        <button
          class="import-btn"
          on:click|stopPropagation={() => uploadInput.click()}
          disabled={uploading}
          title="Upload .gcode file to printer"
        >
          {#if uploading}
            <svg width="13" height="13" viewBox="0 0 13 13" fill="none" class="spin">
              <path d="M11 6.5a4.5 4.5 0 11-1.3-3.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
              <path d="M11 2v3h-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Uploading…
          {:else}
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
              <path d="M8 2v9M4.5 5.5L8 2l3.5 3.5M3 13h10" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Upload
          {/if}
        </button>
      {/if}
      <button class="import-btn" on:click|stopPropagation={refreshCurrentTab} title="Refresh">
        <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
          <path d="M11 6.5a4.5 4.5 0 11-1.3-3.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
          <path d="M11 2v3h-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        Refresh
      </button>
      <svg class="chevron {collapsed ? 'collapsed' : ''}" width="14" height="14" viewBox="0 0 14 14" fill="none">
        <path d="M3 5l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </div>
  </div>

  {#if !collapsed}
    <div class="tabs">
      <button class="tab {activeTab === 'local' ? 'active' : ''}" on:click={() => switchTab('local')}>Printer</button>
      <button class="tab {activeTab === 'udisk' ? 'active' : ''}" on:click={() => switchTab('udisk')}>USB</button>
      <button class="tab {activeTab === 'history' ? 'active' : ''}" on:click={() => switchTab('history')}>History</button>
    </div>

    {#if error || usbError}
      <div class="err-row">
        <span>{error || usbError}</span>
        <button class="retry-btn" on:click={() => activeTab === 'local' ? loadFiles() : activeTab === 'udisk' ? loadUsbFiles() : loadHistory()}>Retry</button>
      </div>
    {/if}

    <div class="table-wrap">
      {#if (activeTab === 'local' && localPath !== '/') || (activeTab === 'udisk' && usbPath !== '/')}
        <div class="path-bar">
          <button
            class="back-btn"
            on:click={() => {
              if (activeTab === 'local') {
                localPath = parentPath(localPath);
                localPageNumber = localPageHistory[localPath] || 1;
                loadFiles();
              } else if (activeTab === 'udisk') {
                usbPath = parentPath(usbPath);
                usbPageNumber = usbPageHistory[usbPath] || 1;
                loadUsbFiles();
              }
            }}
          >
            ← Back
          </button>

          <span class="current-path">
            {activeTab === 'local' ? localPath : usbPath}
          </span>
        </div>
      {/if}
      {#if loading}
        <div class="empty">Loading...</div>
      {:else}
        {@const displayFiles = activeTab === 'history' ? historyFiles : activeTab === 'udisk' ? usbFiles : localFiles}
        {#if displayFiles == null ||displayFiles.length === 0}
          <div class="empty">
            {
              activeTab === 'history'
                ? 'No print history found.'
                : (
                    (activeTab === 'local' && localPath !== '/') ||
                    (activeTab === 'udisk' && usbPath !== '/')
                  )
                  ? 'Folder is empty.'
                  : (
                      activeTab === 'local'
                        ? 'No files found. Click Refresh to load.'
                        : 'No USB files found. Insert USB drive and click Refresh.'
                    )
            }
          </div>
        {:else}
          <table>
            <thead>
              <tr>
                <th class="col-name">File Name</th>
                <th class="col-size">Size</th>
                <th class="col-layer">Layers</th>
                <th class="col-date">Created</th>
                <th class="col-action"></th>
              </tr>
            </thead>
            <tbody>
              {#each displayFiles as file}
                <tr class="file-row" on:click={() => file.type === 'folder' ? openFolder(file) : openPrintModal(file)}>
                  <td class="col-name">
                    <div class="filename-cell">
                      {#if file.type === 'folder'}
                        <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                          <path
                            d="M1.5 3.5h3l1.2 1.2h5.8v5.8h-10V3.5z"
                            stroke="var(--accent)"
                            stroke-width="1"
                            stroke-linejoin="round"
                          />
                        </svg>
                      {:else}
                        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                          <rect
                            x="1" y="1"
                            width="10"
                            height="10"
                            rx="2"
                            stroke="var(--muted)"
                            stroke-width="1"
                            fill="none"
                          />
                          <path
                            d="M3 4h6M3 6h6M3 8h4"
                            stroke="var(--muted)"
                            stroke-width="0.9"
                            stroke-linecap="round"
                          />
                        </svg>
                      {/if}
                      <span title={file.filename ?? file.name ?? ''}>{shortName(file.filename ?? file.name ?? '')}</span>
                    </div>
                  </td>
                  <td class="col-size mono">{formatSize(+(file.size ?? file.file_size ?? 0))}</td>
                  <td class="col-layer mono">{file.total_layer ?? file.layer ?? file.layers ?? '--'}</td>
                  <td class="col-date">{formatDate(file.create_time ?? file.created)}</td>
                  <td class="col-action">
                    {#if file.type !== 'folder'}
                      <button
                        class="print-btn"
                        on:click|stopPropagation={() => openPrintModal(file)}
                        title="Print this file"
                      >
                        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                          <path
                            d="M2.5 1.5l8 4.5-8 4.5V1.5z"
                            fill="currentColor"
                          />
                        </svg>
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if (activeTab === 'local' && localMaxSite > 1) || (activeTab === 'udisk' && usbMaxSite > 1) || (activeTab === 'history' && historyMaxSite > 1)}
            {@const currentPage = activeTab === 'local' ? localPageNumber : activeTab === 'udisk' ? usbPageNumber : historyPageNumber}
            {@const maxPage = activeTab === 'local' ? localMaxSite : activeTab === 'udisk' ? usbMaxSite : historyMaxSite}

            <div class="pagination">
              <button
                class="page-btn"
                disabled={currentPage <= 1}
                on:click={() => {
                  if (currentPage <= 1) return;

                  if (activeTab === 'local') {
                    localPageNumber--;
                    loadFiles();
                  } else if (activeTab === 'udisk') {
                    usbPageNumber--;
                    loadUsbFiles();
                  } else {
                    historyPageNumber--;
                    loadHistory();
                  }
                }}
              >
                ◀ Previous
              </button>

              <span class="page-info">
                Page {currentPage} / {maxPage}
              </span>

              <button
                class="page-btn"
                disabled={currentPage >= maxPage}
                on:click={() => {
                  if (currentPage >= maxPage) return;

                  if (activeTab === 'local') {
                    localPageNumber++;
                    loadFiles();
                  } else if (activeTab === 'udisk') {
                    usbPageNumber++;
                    loadUsbFiles();
                  } else {
                    historyPageNumber++;
                    loadHistory();
                  }
                }}
              >
                Next ▶
              </button>
            </div>
          {/if}
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px 8px;
    cursor: pointer;
    user-select: none;
    border-bottom: 1px solid var(--border);
  }
  .card-header:hover { background: var(--surface2); }

  .card-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--muted);
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .import-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    font-size: 11px;
    font-weight: 500;
    background: var(--surface2);
    color: var(--muted);
    border: 1px solid var(--border2);
    border-radius: var(--radius-sm);
    transition: background 0.15s, color 0.15s;
  }

  .import-btn:hover:not(:disabled) { background: var(--border); color: var(--text); }
  .import-btn:disabled { opacity: 0.6; cursor: not-allowed; }
  .spin { animation: spin 0.9s linear infinite; transform-origin: center; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .chevron {
    color: var(--muted);
    transition: transform 0.2s;
    flex-shrink: 0;
  }
  .chevron.collapsed { transform: rotate(-90deg); }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding: 0 14px;
  }

  .tab {
    padding: 8px 14px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color 0.15s, border-color 0.15s;
    cursor: pointer;
  }

  .tab:hover { color: var(--text); }
  .tab.active { color: var(--text); border-bottom-color: var(--border2); }

  .err-row {
    margin: 8px 14px;
    font-size: 12px;
    color: var(--danger);
    padding: 6px 10px;
    background: var(--danger-dim);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .retry-btn {
    font-size: 11px;
    font-weight: 500;
    color: var(--danger);
    background: none;
    border: 1px solid rgba(192,57,74,0.4);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .retry-btn:hover { background: rgba(192,57,74,0.12); }

  .table-wrap { overflow-x: auto; min-height: 60px; }

  .empty {
    padding: 24px 14px;
    font-size: 13px;
    color: var(--muted);
    text-align: center;
  }

  table { width: 100%; border-collapse: collapse; font-size: 12px; }

  thead th {
    padding: 7px 12px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    text-align: left;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }

  tbody tr.file-row {
    border-bottom: 1px solid var(--border);
    transition: background 0.1s;
    cursor: pointer;
  }
  tbody tr.file-row:last-child { border-bottom: none; }
  tbody tr.file-row:hover { background: var(--surface2); }

  td { padding: 8px 12px; vertical-align: middle; }

  .col-name { min-width: 180px; }
  .col-size { min-width: 70px; }
  .col-layer { min-width: 60px; }
  .col-date { min-width: 100px; color: var(--muted); }
  .col-action { width: 40px; text-align: center; }

  .filename-cell {
    display: flex;
    align-items: center;
    gap: 7px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 280px;
  }
  .filename-cell span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

  .print-btn {
    width: 26px;
    height: 26px;
    border-radius: 6px;
    background: var(--accent-dim);
    border: 1px solid rgba(45,135,240,0.3);
    color: var(--accent);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: filter 0.15s;
  }
  .print-btn:hover { filter: brightness(1.2); }

  .path-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border);
  }

  .back-btn {
    padding: 3px 8px;
    font-size: 11px;
    color: var(--muted);
    background: var(--surface2);
    border: 1px solid var(--border2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .back-btn:hover {
    color: var(--text);
    background: var(--border);
  }

  .current-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pagination {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 1rem;
    margin-top: 1rem;
  }

  .page-btn {
    padding: 0.4rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    cursor: pointer;
  }

  .page-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .page-info {
    font-size: 0.9rem;
    font-weight: 500;
  }
</style>
