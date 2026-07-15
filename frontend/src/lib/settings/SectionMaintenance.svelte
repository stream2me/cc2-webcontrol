<script>
  import { restartServer, rebootPrinter } from '../../api';

  let restarting = false;
  let restartError = '';
  let rebooting = false;
  let rebootError = '';

  async function waitForServer() {
    while (true) {
      try {
        const res = await fetch('/api/printer/status');
        if (res.ok) {
          const status = await res.json();
          if (status.printer_ws_status === 'connected') {
            await new Promise((r) => setTimeout(r, 3000));
            window.location.href = '/';
            return;
          }
        }
      } catch {}
      await new Promise(r => setTimeout(r, 2000));
    }
  }

  // Restart webcontrol
  async function doRestart() {
    restarting = true;
    restartError = '';
    try {
      await restartServer();
      await waitForServer();
    } catch (e) {
      restartError = 'Restart failed';
      restarting = false;
    }
  }

  // reboot Printer
  async function doReboot() {
    rebooting = true;
    rebootError = '';
    try {
      await rebootPrinter(); 
      await waitForServer();
    } catch (e) {
      rebootError = 'Reboot failed';
      rebooting = false;
    }
  }
</script>

<div class="group">
  <div class="row">
    <div class="row-label">
      <div class="row-title">webcontrol</div>
      <div class="row-sub">Restart the webcontrol binary.</div>
    </div>
    <div class="system-area">
      <button
        class="btn sm"
        on:click={doRestart}
        disabled={restarting || rebooting}>
        {#if restarting}
          Restarting...
        {:else}
          Restart webcontrol
        {/if}
      </button>
      {#if restartError}
        <div class="error">{restartError}</div>
      {/if}
    </div>
  </div>
  <div class="row">
    <div class="row-label">
      <div class="row-title">Printer</div>
      <div class="row-sub">Reboot the printer.</div>
    </div>
    <div class="system-area">
      <button
        class="btn sm" 
        on:click={doReboot} 
        disabled={restarting || rebooting}>
        {#if rebooting}
          Rebooting...
        {:else}
          Reboot printer
        {/if}
      </button>
      {#if rebootError}
        <div class="error">{rebootError}</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .group {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 220px;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  .row:first-child { border-top: none; }
  .row-label { min-width: 0; }
  .row-title { font-size: 12.5px; font-weight: 500; color: var(--text); }
  .row-sub { font-size: 11.5px; color: var(--muted); margin-top: 2px; line-height: 1.45; }

  .system-area { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .error { color: var(--error, #ff4444); font-size: 11.5px; }
</style>
