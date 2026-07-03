<script lang="ts">
  import { tick } from 'svelte';
  import { RefreshCw, FolderOpen, Circle } from 'lucide-svelte';
  import { getAutosaveCfg } from '../api';

  const profiles = {
    'side_a': ['bed_mesh default', 'Side A'],
    'side_b': ['bed_mesh default1', 'Side B'],
    'adaptive': ['bed_mesh ADAPTIVE', 'Adaptive']
  };

  let collapsed = true;
  let parsedProfiles: Record<string, any> = {};
  let currentProfile: string | null = null;
  let fileName = '';
  let errorMessage = '';
  let stats: { minZ: string; maxZ: string; flatnessDev: string; gridSize: string } | null = null;
  let activeProfileButtons: string[] = [];
  let isUploadHovered = false;
  let fileInput: HTMLInputElement;

  // Fetches configuration via API endpoint
  async function loadMeshFromAPI() {
    try {
      fileName = 'Loading configuration from Printer...';
      errorMessage = '';

      const data = await getAutosaveCfg();

      if (!data || data.trim() === '') {
        throw new Error('The received configuration file is empty.');
      }

      fileName = 'autosave.cfg (Printer)';
      processFile(data);

    } catch (error: any) {
      console.error("API Fetch Error:", error);
      errorMessage = `Error loading configuration. <br>Details: ${error.message}`;
      fileName = 'Loading from Printer failed.';
    }
  }

  // Parses Klipper config section using secure string methods
  function parseProfile(configContent: string, sectionName: string) {
    const lines = configContent.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');

    let inSection = false;
    let sectionLines: string[] = [];
    const targetSection = sectionName.trim().toLowerCase();

    // 1. Extract lines belonging to target section
    for (let line of lines) {
      let cleanLine = line.replace(/^#\*\#\s*/, '').trim();
      if (cleanLine === '') continue;

      if (cleanLine.startsWith('[') && cleanLine.endsWith(']')) {
        let currentSectionName = cleanLine.slice(1, -1).trim().toLowerCase();

        if (currentSectionName === targetSection) {
          inSection = true;
          continue;
        } else if (inSection) {
          break; // Exit if a new section starts
        }
      }

      if (inSection) {
        sectionLines.push(cleanLine);
      }
    }

    if (sectionLines.length === 0) return null;

    let pointsStr = '';
    let configMap: Record<string, number> = {
      min_x: 20, max_x: 246, min_y: 20, max_y: 246, x_count: 11, y_count: 11
    };

    // 2. Extract key-value configurations and points
    for (let line of sectionLines) {
      if (!line || !line.includes('=')) continue;

      let eqIndex = line.indexOf('=');
      let key = line.substring(0, eqIndex).trim().toLowerCase();
      let valueStr = line.substring(eqIndex + 1).trim();

      if (key === 'points') {
        pointsStr = valueStr;
      } else {
        let value = parseFloat(valueStr);
        if (!isNaN(value)) {
          configMap[key] = value;
        }
      }
    }

    // 3. Sanitize and split data matrix array
    let cleanPointsStr = pointsStr.replace(/->/g, '').replace(/>/g, '').replace(/\s+/g, '');
    if (!cleanPointsStr) return null;

    const meshPoints = cleanPointsStr.split(',')
      .filter(p => p.length > 0 && !isNaN(parseFloat(p)))
      .map(p => parseFloat(p));

    const expectedPoints = Math.floor(configMap.x_count) * Math.floor(configMap.y_count);

    if (meshPoints.length === 0) return null;

    // Array size protection and padding
    if (meshPoints.length !== expectedPoints) {
      console.warn(`Expected ${expectedPoints} points for ${sectionName}, found ${meshPoints.length}. Adjusting.`);
      while (meshPoints.length < expectedPoints) {
        meshPoints.push(meshPoints[meshPoints.length - 1] || 0);
      }
      if (meshPoints.length > expectedPoints) {
        meshPoints.splice(expectedPoints);
      }
    }

    // 4. Generate 2D array structure for Plotly surface mesh
    const points = [];
    const yCount = Math.floor(configMap.y_count);
    const xCount = Math.floor(configMap.x_count);

    for (let i = 0; i < yCount; i++) {
      points.push(meshPoints.slice(i * xCount, (i + 1) * xCount));
    }

    return {
      points,
      configs: {
        min_x: configMap.min_x,
        max_x: configMap.max_x,
        min_y: configMap.min_y,
        max_y: configMap.max_y,
        x_count: xCount,
        y_count: yCount
      }
    };
  }

  // Renders 3D Plotly Surface inside element container
  async function createPlot(profileKey: string, mesh: any) {
    const Plotly = (await import('plotly.js-gl3d-dist-min')).default;
    const points = mesh.points;
    const configs = mesh.configs;

    const xCoords = [];
    const yCoords = [];
    for (let i = 0; i < configs.x_count; i++) {
      xCoords.push(configs.min_x + (configs.max_x - configs.min_x) * i / (configs.x_count - 1));
    }
    for (let i = 0; i < configs.y_count; i++) {
      yCoords.push(configs.min_y + (configs.max_y - configs.min_y) * i / (configs.y_count - 1));
    }

    const flatPoints = points.flat();
    const minZ = Math.min(...flatPoints);
    const maxZ = Math.max(...flatPoints);
    const flatnessDev = maxZ - minZ;

    stats = {
      minZ: `${minZ.toFixed(5)} mm`,
      maxZ: `${maxZ.toFixed(5)} mm`,
      flatnessDev: `${flatnessDev.toFixed(5)} mm`,
      gridSize: `${configs.x_count} x ${configs.y_count}`
    };

    const zeroPlane = Array(2).fill(null).map(() => Array(2).fill(0));
    const planeX = [configs.min_x, configs.max_x];
    const planeY = [configs.min_y, configs.max_y];

    const data = [
      {
        type: 'surface',
        x: xCoords,
        y: yCoords,
        z: points,
        colorscale: 'RdBu',
        reversescale: false,
        colorbar: { title: 'Height (mm)', titleside: 'right' },
        name: 'Bed Mesh'
      },
      {
        type: 'surface',
        x: planeX,
        y: planeY,
        z: zeroPlane,
        colorscale: [[0, 'rgba(100, 255, 100, 0.3)'], [1, 'rgba(100, 255, 100, 0.3)']],
        showscale: false,
        name: 'Z=0 Reference',
        hovertemplate: 'X: %{x:.1f} mm<br>Y: %{y:.1f} mm<br>Z: 0.000 mm<extra></extra>'
      }
    ];

    const layout = {
      title: {
        text: `Bed Mesh: ${getProfileLabel(profileKey)}<br><sub>Flatness Deviation: ${flatnessDev.toFixed(3)} mm</sub>`,
        font: { color: '#e0e0e0' }
      },
      scene: {
        xaxis: { title: 'X (mm)', color: '#e0e0e0', gridcolor: '#4a4a4a' },
        yaxis: { title: 'Y (mm)', color: '#e0e0e0', gridcolor: '#4a4a4a' },
        zaxis: { title: 'Height (mm)', color: '#e0e0e0', gridcolor: '#4a4a4a' },
        bgcolor: '#2a2a2a',
        camera: { eye: { x: 1.5, y: 1.5, z: 1.3 } }
      },
      paper_bgcolor: '#2a2a2a',
      plot_bgcolor: '#2a2a2a',
      font: { color: '#e0e0e0' },
      margin: { l: 0, r: 0, t: 80, b: 0 }
    };

    Plotly.react('sveltePlotContainer', data, layout, {
      responsive: true,
      displayModeBar: true,
      displaylogo: false
    });
  }

  async function displayProfile(profileKey: string) {
    currentProfile = profileKey;
    const mesh = parsedProfiles[profileKey];
    await tick();
    createPlot(profileKey, mesh);
  }

  // Iterates profiles mapped configuration definition
  function processFile(content: string) {
    errorMessage = '';
    parsedProfiles = {};

    for (const [key, [sectionName]] of Object.entries(profiles)) {
      const mesh = parseProfile(content, sectionName);
      if (mesh) {
        parsedProfiles[key] = mesh;
      }
    }

    activeProfileButtons = Object.keys(parsedProfiles);

    if (activeProfileButtons.length === 0) {
      errorMessage = 'No valid Bed Mesh profiles found in file. Searched for: ' +
        Object.values(profiles).map(p => p[0]).join(', ');
      stats = null;
      return;
    }

    displayProfile(activeProfileButtons[0]);
  }

  function handleFileChange(e: Event) {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (!file) return;
    readFile(file);
  }

  function readFile(file: File) {
    fileName = `Local file: ${file.name}`;
    const reader = new FileReader();
    reader.onload = (event) => {
      if (event.target?.result) {
        processFile(event.target.result as string);
      }
    };
    reader.onerror = () => {
      errorMessage = 'Error reading file data.';
    };
    reader.readAsText(file);
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    isUploadHovered = false;
    const file = e.dataTransfer?.files[0];
    if (file) {
      readFile(file);
    }
  }

  function getProfileLabel(key: string): string {
    const item = profiles[key as keyof typeof profiles];
    return item ? item[1] : key;
  }

  function toggleCollapse() {
    collapsed = !collapsed;
    
    if (!collapsed) {
      setTimeout(() => {
        const container = document.getElementById('sveltePlotContainer');
        
        if (container && container.data) {
          Plotly.Plots.resize(container);
        } else if (currentProfile) {
          displayProfile(currentProfile); 
        }
      }, 50);
    }
  }
</script>

<div class="card">
  <div class="card-header" role="button" tabindex="0"
    on:click={toggleCollapse} 
    on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && toggleCollapse()}
  >
    <span class="card-title">Bed Mesh Visualizer</span>
    <div class="header-right">
      <svg class="chevron" class:up={collapsed} width="14" height="14" viewBox="0 0 14 14" fill="none">
        <path d="M3 5l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </div>
  </div>

  <div class="body" style="display: {collapsed ? 'none' : 'block'};">
        
    <div class="actions-row">
      <button type="button" class="load-btn" on:click={loadMeshFromAPI} title="Load config from printer" >
        <RefreshCw size={13} /> Load
      </button>

      <div class="upload-section"
           title="Upload local file (.cfg) or drag file here"
           role="region"
           aria-label="Configuration File Drag & Drop Zone"
           class:hovered={isUploadHovered}
           on:dragover|preventDefault={() => isUploadHovered = true}
           on:dragleave|preventDefault={() => isUploadHovered = false}
           on:drop|preventDefault={handleDrop}>
        <input type="file" accept=".cfg,.txt" bind:this={fileInput} on:change={handleFileChange} style="display: none;" />
        <button type="button" class="upload-trigger-btn" on:click={() => fileInput.click()}>
          <FolderOpen size={14} /> Upload
        </button>
      </div>
    </div>

    {#if fileName}
      <div class="source-status">
        <span class="status-indicator"><Circle size={8} fill="currentColor" /></span> 
        Current source: <strong>{fileName}</strong>
      </div>
    {/if}

    {#if errorMessage}
      <div class="error-msg">{@html errorMessage}</div>
    {/if}

    {#if activeProfileButtons.length > 0}
      <div class="selection-row">
        {#each activeProfileButtons as key}
          <button
            type="button"
            class="profile-btn"
            class:active={currentProfile === key}
            on:click={() => displayProfile(key)}>
            {getProfileLabel(key)}
          </button>
        {/each}
      </div>
    {/if}
      
    {#if stats}
      <div class="stats-grid">
        <div class="stat-card">
          <span class="stat-label">Min Height</span>
          <span class="stat-value">{stats.minZ}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Max Height</span>
          <span class="stat-value">{stats.maxZ}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Flatness Dev</span>
          <span class="stat-value">{stats.flatnessDev}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Grid Size</span>
          <span class="stat-value">{stats.gridSize}</span>
        </div>
      </div>
    {/if}

    <div id="sveltePlotContainer" class:hidden={!currentProfile}></div>
  </div>
</div>

<style>
  .card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }

  .card-header { display: flex; align-items: center; justify-content: space-between; padding: 10px 14px 9px; cursor: pointer; user-select: none; border-bottom: 1px solid var(--border); }
  .card-header:hover { background: var(--surface2); }

  .card-title { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.07em; color: var(--muted); }

  .body { padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; } 
  
  .chevron { color: var(--muted); transition: transform 0.2s; flex-shrink: 0; }
  .chevron.up { transform: rotate(-180deg); }

  .actions-row { display: flex; gap: 10px; margin-bottom: 5px; align-items: stretch;  }

  .load-btn { flex: 1;  background: #007acc; color: white; border: none; padding: 8px 10px; border-radius: 4px; cursor: pointer; font-size: 11px; display: flex; align-items: center; justify-content: center; gap: 6px; }

  .upload-section { flex: 1; border: 1px dashed #4a4a4a; padding: 0 10px; text-align: center; border-radius: 4px; background: #222; display: flex; align-items: center; justify-content: center; }

  .upload-section.hovered { border-color: #007acc; }

  .upload-trigger-btn { background: none; border: none; color: #ccc; cursor: pointer; font-size: 11px; display: flex; align-items: center; gap: 6px; }

  .source-status { font-size: 11px; color: #aaa; margin-bottom: 5px; display: flex; align-items: center; gap: 5px; }
  .status-indicator { color: #4cd964; }

  .error-msg { background: #5a2222; color: #ffbaba; padding: 8px; border-radius: 4px; margin-bottom: 10px; font-size: 11px; }

  .selection-row { display: flex; gap: 8px; margin-bottom: 5px; }

  .profile-btn { background: var(--surface2, #222); color: #aaa; border: 1px solid var(--border, #4a4a4a); padding: 6px 12px; border-radius: 4px; cursor: pointer; font-size: 11px; transition: all 0.2s; }

  .profile-btn.active { background: #007acc; color: white; border-color: #007acc; }

  .stats-grid { display: grid; grid-template-columns: repeat(4, 1fr);  gap: 10px; margin-top: 5px; margin-bottom: 5px; }

  .stat-card { background: var(--surface2, #222); border: 1px solid var(--border, #4a4a4a); border-radius: 4px; padding: 10px; display: flex; flex-direction: column; align-items: center;  justify-content: center; }

  .stat-label { font-size: 9px; text-transform: uppercase; color: var(--muted, #888); margin-bottom: 4px; letter-spacing: 0.05em; }

  .stat-value { font-size: 12px; font-weight: 600; color: #fff; }

  #sveltePlotContainer { width: 100%; height: 750px; background: #2a2a2a; }
  .hidden { display: none !important; }

  @media (max-width: 600px) { .stats-grid { grid-template-columns: repeat(2, 1fr); }}

</style>
