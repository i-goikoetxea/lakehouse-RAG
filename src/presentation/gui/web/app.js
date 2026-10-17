/**
 * Lakehouse RAG Industrial — Frontend Client (Linear + NotebookLM)
 * Arquitectura SOTA 2026: Soporte dual Tauri IPC y Web Fallback
 */

(function () {
  'use strict';

  // 1. Detección de entorno Tauri IPC
  const isTauri = typeof window.__TAURI__ !== 'undefined';
  const tauriInvoke = isTauri && window.__TAURI__.core ? window.__TAURI__.core.invoke : (isTauri && window.__TAURI__.invoke ? window.__TAURI__.invoke : null);

  // 2. Elementos del DOM
  const htmlEl = document.documentElement;
  const btnThemeToggle = document.getElementById('btn-theme-toggle');
  const btnMinimize = document.getElementById('btn-minimize');
  const btnMaximize = document.getElementById('btn-maximize');
  const btnClose = document.getElementById('btn-close');

  const selectCentro = document.getElementById('select-centro');
  const selectModelo = document.getElementById('select-modelo');
  const dropZone = document.getElementById('drop-zone');
  const fileInput = document.getElementById('file-input');
  const dropProgress = document.getElementById('dropzone-progress');
  const sourcesList = document.getElementById('sources-list');
  const sourcesCount = document.getElementById('sources-count');
  const btnRefreshSources = document.getElementById('btn-refresh-sources');

  const chatForm = document.getElementById('chat-form');
  const chatInput = document.getElementById('chat-input');
  const chatMessages = document.getElementById('chat-messages');

  const tabButtons = document.querySelectorAll('.studio-tabs .tab-btn');
  const tabContents = document.querySelectorAll('.panel-studio .tab-content');
  const matrixInspector = document.getElementById('matrix-inspector-result');
  const graphInspector = document.getElementById('graph-inspector-result');

  // 3. Catálogo en memoria de documentos activos
  let documentsCatalog = [
    {
      id: 'DOC-NORM-2026',
      name: 'Norma Técnica de Soldadura y Recipientes 2026',
      type: 'PDF',
      size: '2.4 MB',
      status: 'vigente',
      hash: 'sha256:7f83b165...',
      date: '15/01/2026'
    },
    {
      id: 'DOC-ESPEC-042',
      name: 'Matriz Operativa de Presión y Temperatura Alfa',
      type: 'Markdown',
      size: '480 KB',
      status: 'vigente',
      hash: 'sha256:c912e58a...',
      date: '10/02/2026'
    },
    {
      id: 'DOC-NORM-2020',
      name: 'Reglamento Histórico de Soldadura 2020',
      type: 'PDF',
      size: '1.8 MB',
      status: 'derogada',
      hash: 'sha256:3a15f029...',
      date: '04/05/2020'
    }
  ];

  // 4. Gestión del Tema Oscuro / Claro (Persistencia en LocalStorage)
  function initTheme() {
    const savedTheme = localStorage.getItem('lakehouse_theme') || 'dark';
    htmlEl.setAttribute('data-theme', savedTheme);
  }

  function toggleTheme() {
    const currentTheme = htmlEl.getAttribute('data-theme') || 'dark';
    const newTheme = currentTheme === 'dark' ? 'light' : 'dark';
    htmlEl.setAttribute('data-theme', newTheme);
    localStorage.setItem('lakehouse_theme', newTheme);
  }

  btnThemeToggle.addEventListener('click', toggleTheme);
  initTheme();

  // 3b. Catálogo dinámico de modelos (Claude Code headless vía IPC)
  const MODELOS_RESPALDO = [
    { alias: 'sonnet', displayName: 'Sonnet', description: 'Modo demostración sin Tauri' },
    { alias: 'haiku', displayName: 'Haiku', description: 'Modo demostración sin Tauri' },
    { alias: 'opus', displayName: 'Opus', description: 'Modo demostración sin Tauri' }
  ];

  function renderModelOptions(modelos) {
    selectModelo.innerHTML = '';
    modelos.forEach((modelo) => {
      const option = document.createElement('option');
      option.value = modelo.alias;
      option.textContent = modelo.description
        ? `${modelo.displayName} — ${modelo.description}`
        : modelo.displayName;
      option.title = modelo.resolvedModel || modelo.alias;
      option.dataset.label = modelo.displayName;
      selectModelo.appendChild(option);
    });
    selectModelo.disabled = modelos.length === 0;
  }

  async function loadModelCatalog() {
    if (!tauriInvoke) {
      renderModelOptions(MODELOS_RESPALDO);
      return;
    }
    try {
      const modelos = await tauriInvoke('listar_modelos_claude');
      renderModelOptions(modelos);
    } catch (err) {
      console.error('Error al listar modelos de Claude Code:', err);
      selectModelo.innerHTML = '<option value="">Claude Code no disponible</option>';
      selectModelo.disabled = true;
      appendSystemNotice(`No se pudo obtener el catálogo de modelos de Claude Code: ${err.message || err}`, true);
    }
  }

  function selectedModelLabel() {
    const option = selectModelo.options[selectModelo.selectedIndex];
    return option ? (option.dataset.label || option.textContent) : '';
  }

  // 5. Controles de Ventana Frameless (Tauri IPC con Fallback)
  function getTauriWindow() {
    if (!window.__TAURI__) return null;
    if (window.__TAURI__.webviewWindow && window.__TAURI__.webviewWindow.getCurrentWebviewWindow) {
      return window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
    }
    if (window.__TAURI__.window && window.__TAURI__.window.getCurrentWindow) {
      return window.__TAURI__.window.getCurrentWindow();
    }
    return null;
  }

  if (btnMinimize) {
    btnMinimize.addEventListener('click', async () => {
      const appWin = getTauriWindow();
      if (appWin) {
        try {
          await appWin.minimize();
        } catch (e) {
          console.warn('Tauri window minimize:', e);
        }
      } else {
        console.log('[Web Fallback] Minimizar ventana');
      }
    });
  }

  if (btnMaximize) {
    btnMaximize.addEventListener('click', async () => {
      const appWin = getTauriWindow();
      if (appWin) {
        try {
          await appWin.toggleMaximize();
        } catch (e) {
          console.warn('Tauri window maximize:', e);
        }
      } else {
        console.log('[Web Fallback] Maximizar/Restaurar ventana');
      }
    });
  }

  if (btnClose) {
    btnClose.addEventListener('click', async () => {
      const appWin = getTauriWindow();
      if (appWin) {
        try {
          await appWin.close();
        } catch (e) {
          console.warn('Tauri window close:', e);
        }
      } else {
        console.log('[Web Fallback] Cerrar aplicación');
      }
    });
  }

  // 6. Navegación de Tabs del Lakehouse Studio

  tabButtons.forEach(btn => {
    btn.addEventListener('click', () => {
      const targetId = btn.getAttribute('data-tab');
      tabButtons.forEach(b => b.classList.remove('active'));
      tabContents.forEach(c => c.classList.remove('active'));

      btn.classList.add('active');
      const targetContent = document.getElementById(targetId);
      if (targetContent) {
        targetContent.classList.add('active');
      }
    });
  });

  // 7. Renderizado del Catálogo de Fuentes (Arrow 01)
  function renderSources() {
    sourcesList.innerHTML = '';
    sourcesCount.textContent = `${documentsCatalog.length} docs`;

    documentsCatalog.forEach(doc => {
      const li = document.createElement('li');
      li.className = 'source-item';
      const isDerogada = doc.status === 'derogada';

      li.innerHTML = `
        <div class="source-item-info">
          <div class="source-item-title-row">
            <span class="source-name">${escapeHtml(doc.name)}</span>
            <span class="status-badge ${isDerogada ? 'badge-derogada' : 'badge-vigente'}">
              ${isDerogada ? 'Derogada' : 'Vigente'}
            </span>
          </div>
          <div class="source-item-meta">
            <span>ID: ${escapeHtml(doc.id)}</span> • <span>${escapeHtml(doc.size)}</span> • <span>${escapeHtml(doc.date)}</span>
          </div>
        </div>
      `;
      sourcesList.appendChild(li);
    });
  }

  renderSources();

  btnRefreshSources.addEventListener('click', () => {
    renderSources();
  });

  // 8. Zona Drag & Drop e Ingesta Atómica
  dropZone.addEventListener('click', () => fileInput.click());

  dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropZone.classList.add('drag-active');
  });

  dropZone.addEventListener('dragleave', (e) => {
    e.preventDefault();
    dropZone.classList.remove('drag-active');
  });

  dropZone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropZone.classList.remove('drag-active');
    if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      handleFiles(e.dataTransfer.files);
    }
  });

  fileInput.addEventListener('change', (e) => {
    if (e.target.files && e.target.files.length > 0) {
      handleFiles(e.target.files);
    }
  });

  async function handleFiles(files) {
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      await ingestFile(file);
    }
  }

  async function ingestFile(file) {
    dropProgress.style.width = '20%';
    appendSystemNotice(`Iniciando extracción y hash Arrow de ${file.name}...`);

    try {
      if (tauriInvoke) {
        // En Tauri se envía el path o el contenido al backend
        await tauriInvoke('ingestar_documento', { 
          nombre: file.name, 
          centroId: selectCentro.value 
        });
      } else {
        // Fallback simulado
        await simulateProgress();
      }

      dropProgress.style.width = '100%';
      setTimeout(() => { dropProgress.style.width = '0%'; }, 500);

      const newDoc = {
        id: `DOC-INGEST-${Math.floor(1000 + Math.random() * 9000)}`,
        name: file.name,
        type: file.name.endsWith('.pdf') ? 'PDF' : 'Markdown',
        size: `${(file.size / 1024).toFixed(1)} KB`,
        status: 'vigente',
        hash: `sha256:${Math.random().toString(16).substring(2, 10)}...`,
        date: new Date().toLocaleDateString('es-ES')
      };

      documentsCatalog.unshift(newDoc);
      renderSources();
      appendSystemNotice(`Documento ${file.name} ingestado con éxito en los 6 esquemas Arrow.`);
    } catch (err) {
      console.error('Error al ingestar:', err);
      dropProgress.style.width = '0%';
      appendSystemNotice(`Error al procesar ${file.name}: ${err.message || err}`, true);
    }
  }

  function simulateProgress() {
    return new Promise((resolve) => {
      let p = 20;
      const interval = setInterval(() => {
        p += 25;
        dropProgress.style.width = `${p}%`;
        if (p >= 100) {
          clearInterval(interval);
          resolve();
        }
      }, 150);
    });
  }

  // 9. Lógica de Chat, Observabilidad de Tools y Grounding
  chatForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const query = chatInput.value.trim();
    if (!query) return;

    chatInput.value = '';
    appendUserMessage(query);

    // Scroll inmediato hacia abajo
    chatMessages.scrollTop = chatMessages.scrollHeight;

    // Procesar respuesta
    await processAgentQuery(query);
  });

  function appendUserMessage(text) {
    const msgDiv = document.createElement('div');
    msgDiv.className = 'message message-user';
    msgDiv.innerHTML = `
      <div class="message-bubble">
        <div class="message-meta">
          <span class="sender-name">Operador</span>
          <span class="timestamp">${formatTime()}</span>
        </div>
        <div class="message-text">${escapeHtml(text)}</div>
      </div>
    `;
    chatMessages.appendChild(msgDiv);
    chatMessages.scrollTop = chatMessages.scrollHeight;
  }

  function appendSystemNotice(text, isError = false) {
    const notice = document.createElement('div');
    notice.className = `system-notice ${isError ? 'notice-error' : 'notice-info'}`;
    notice.textContent = text;
    chatMessages.appendChild(notice);
    chatMessages.scrollTop = chatMessages.scrollHeight;
  }

  async function processAgentQuery(query) {
    const centroId = selectCentro.value;
    const modelo = selectModelo.value;
    if (!modelo) {
      appendSystemNotice('Selecciona un modelo disponible antes de enviar la consulta.', true);
      return;
    }

    // Mostrar loader del asistente
    const assistantMsg = document.createElement('div');
    assistantMsg.className = 'message message-assistant';
    const bubble = document.createElement('div');
    bubble.className = 'message-bubble';
    bubble.innerHTML = `
      <div class="message-meta">
        <span class="sender-name">Asistente Lakehouse</span>
        <span class="timestamp">${formatTime()}</span>
      </div>
      <div class="message-text message-loading">
        <span class="pulse-dot"></span> Evaluando consultas deterministas y semánticas...
      </div>
    `;
    assistantMsg.appendChild(bubble);
    chatMessages.appendChild(assistantMsg);
    chatMessages.scrollTop = chatMessages.scrollHeight;

    try {
      let responseData;
      if (tauriInvoke) {
        responseData = await tauriInvoke('enviar_mensaje_chat', {
          mensaje: query,
          centroId: centroId,
          modelo: modelo
        });
      } else {
        // Fallback realista que emula el servicio de tools
        responseData = await simulateToolUsePipeline(query, centroId);
      }

      // Reemplazar loader con contenido enriquecido
      renderAssistantResponse(bubble, responseData);
    } catch (err) {
      console.error('Error en consulta de chat:', err);
      bubble.querySelector('.message-text').innerHTML = `
        <span style="color: var(--color-error);">Error al procesar la consulta: ${escapeHtml(err.message || String(err))}</span>
      `;
    } finally {
      chatMessages.scrollTop = chatMessages.scrollHeight;
    }
  }

  function renderAssistantResponse(bubble, data) {
    let html = `
      <div class="message-meta">
        <span class="sender-name">Asistente Lakehouse</span>
        <span class="model-tag">${escapeHtml(selectedModelLabel())}</span>
        <span class="timestamp">${formatTime()}</span>
      </div>
    `;

    // 1. Tarjetas colapsables de Tool Use (Observabilidad en tiempo real)
    if (data.toolsUsed && data.toolsUsed.length > 0) {
      data.toolsUsed.forEach((tool, idx) => {
        html += `
          <div class="tool-card ${tool.status === 'ok' ? 'success' : 'failed'}">
            <div class="tool-card-header" onclick="this.parentElement.classList.toggle('collapsed')">
              <div class="tool-badge">
                <span class="tool-icon">⚡</span>
                <span class="tool-name">${escapeHtml(tool.name)}</span>
              </div>
              <span class="tool-execution-time">${escapeHtml(tool.duration || '12ms')}</span>
            </div>
            <div class="tool-card-body">
              <div class="tool-args-label">Parámetros:</div>
              <pre class="tool-args-json">${escapeHtml(JSON.stringify(tool.args, null, 2))}</pre>
              <div class="tool-args-label">Resultado LanceDB/Arrow:</div>
              <pre class="tool-args-json">${escapeHtml(JSON.stringify(tool.result, null, 2))}</pre>
            </div>
          </div>
        `;
      });
    }

    // 2. Texto con citas interactivas
    html += `<div class="message-text">${data.replyHtml}</div>`;

    // 3. Badges de fuentes citadas
    if (data.citations && data.citations.length > 0) {
      html += `<div class="citations-container"><span class="citations-label">Fuentes verificadas:</span>`;
      data.citations.forEach(c => {
        html += `
          <span class="citation-badge" title="${escapeHtml(c.title)} (Hash: ${escapeHtml(c.hash)})" onclick="window.highlightCitation('${escapeHtml(c.docId)}')">
            <span class="citation-badge-id">${escapeHtml(c.docId)}</span>
            <span class="citation-badge-page">pág. ${escapeHtml(c.page)}</span>
          </span>
        `;
      });
      html += `</div>`;
    }

    bubble.innerHTML = html;

    // Actualizar inspectores en Lakehouse Studio si corresponde
    if (data.matrixResult) {
      updateMatrixInspector(data.matrixResult);
    }
    if (data.graphResult) {
      updateGraphInspector(data.graphResult);
    }
  }

  function updateMatrixInspector(matrix) {
    matrixInspector.innerHTML = `
      <div class="inspector-badge-row">
        <span class="chip-badge purple">${escapeHtml(matrix.tableId)}</span>
        <span class="chip-badge cyan">Fila: ${escapeHtml(matrix.rowKey)} | Col: ${escapeHtml(matrix.colKey)}</span>
      </div>
      <table class="inspector-table">
        <thead>
          <tr>
            <th>Espesor (mm)</th>
            <th>Acero Carbono</th>
            <th>Acero Inox</th>
            <th>Aleación Ni</th>
          </tr>
        </thead>
        <tbody>
          <tr class="${matrix.colKey === '15' ? 'highlight-row' : ''}">
            <td>10.0 - 20.0</td>
            <td>E-7018</td>
            <td class="resolved-cell">E-316L-16 ★</td>
            <td>Inconel 625</td>
          </tr>
        </tbody>
      </table>
      <div class="inspector-note">
        <strong>Nota al pie resuelta:</strong> ${escapeHtml(matrix.footnote || 'Precalentamiento obligatorio a 150°C según código ASME Sección IX.')}
      </div>
    `;

    // Auto-activar tab de matrices
    activateTab('tab-matrices');
  }

  function updateGraphInspector(graph) {
    graphInspector.innerHTML = `
      <div class="graph-node-chain">
        <div class="graph-node deprecated">${escapeHtml(graph.deprecatedId)} <span class="node-tag">Derogada</span></div>
        <div class="graph-arrow">↓ ${escapeHtml(graph.relation)}</div>
        <div class="graph-node active">${escapeHtml(graph.activeId)} <span class="node-tag">PREVALENTE</span></div>
      </div>
      <p class="inspector-note" style="margin-top: 12px;">
        <strong>Dictamen de Precedencia:</strong> La norma <code>${escapeHtml(graph.activeId)}</code> deroga expresamente las tolerancias de la norma <code>${escapeHtml(graph.deprecatedId)}</code>.
      </p>
    `;
    activateTab('tab-graph');
  }

  function activateTab(tabId) {
    tabButtons.forEach(b => {
      if (b.getAttribute('data-tab') === tabId) {
        b.classList.add('active');
      } else {
        b.classList.remove('active');
      }
    });
    tabContents.forEach(c => {
      if (c.id === tabId) {
        c.classList.add('active');
      } else {
        c.classList.remove('active');
      }
    });
  }

  window.highlightCitation = function (docId) {
    console.log('Inspeccionando cita:', docId);
    activateTab('tab-schemas');
  };

  // Simulación determinista de pipeline de herramientas para el entorno web / preview
  async function simulateToolUsePipeline(query, centroId) {
    await new Promise(r => setTimeout(r, 600));

    const qLower = query.toLowerCase();

    if (qLower.includes('temperatura') || qLower.includes('recepción') || qLower.includes('presión')) {
      return {
        replyHtml: `Según la <strong>Matriz Operativa de Planta Alfa</strong> (Fase de Recepción), la temperatura máxima permitida es de <strong>85.0 °C</strong> con una tolerancia estricta de ±2.5 °C. Cualquier valor por encima de 87.5 °C activa automáticamente el protocolo de alivio térmico.`,
        toolsUsed: [
          {
            name: 'consultar_matriz_operativa',
            args: { centro_id: centroId, fase_id: 'RECEPCION' },
            result: { registros: [{ parametro: 'TEMPERATURA', min: 20.0, max: 85.0, unidad: 'C', tolerancia: 2.5 }] },
            status: 'ok',
            duration: '9ms'
          }
        ],
        citations: [
          { docId: 'DOC-ESPEC-042', title: 'Matriz Operativa de Presión y Temperatura Alfa', page: 4, hash: 'sha256:c912e58a' }
        ]
      };
    } else if (qLower.includes('electrodo') || qLower.includes('soldar') || qLower.includes('espesor')) {
      return {
        replyHtml: `Para una pieza de <strong>15 mm de espesor</strong> en <strong>acero inoxidable</strong>, la resolución determinista de la tabla de doble entrada especifica el electrodo <strong>E-316L-16</strong>.<br><br>Adicionalmente, se aplica la nota técnica que exige precalentamiento a 150 °C previo al cordón de raíz.`,
        toolsUsed: [
          {
            name: 'evaluar_rango_tabla_doble_entrada',
            args: { tabla_id: 'TABLA-SOLDADURA-01', valor_x: 'acero_inoxidable', valor_y: 15.0 },
            result: { celda_resuelta: 'E-316L-16', rango_y: [10.0, 20.0], nota_pie: 'Precalentamiento obligatorio a 150°C según ASME IX.' },
            status: 'ok',
            duration: '14ms'
          }
        ],
        citations: [
          { docId: 'DOC-NORM-2026', title: 'Norma Técnica de Soldadura 2026', page: 12, hash: 'sha256:7f83b165' }
        ],
        matrixResult: {
          tableId: 'TABLA-SOLDADURA-01',
          rowKey: 'acero_inoxidable',
          colKey: '15',
          footnote: 'Precalentamiento obligatorio a 150°C según código ASME Sección IX.'
        }
      };
    } else if (qLower.includes('vigente') || qLower.includes('derogada') || qLower.includes('norm-001') || qLower.includes('grafo') || qLower.includes('norma')) {
      return {
        replyHtml: `La norma <strong>DOC-NORM-2020</strong> fue expresamente <strong>derogada</strong> por la norma prevalente <strong>DOC-NORM-2026</strong>. Toda la operación en Planta Alfa debe regirse exclusivamente por los criterios de la revisión 2026.`,
        toolsUsed: [
          {
            name: 'consultar_grafo_referencias',
            args: { doc_id: 'DOC-NORM-2020' },
            result: { prevalente: 'DOC-NORM-2026', estado: 'DEROGADA', ciclo_detectado: false },
            status: 'ok',
            duration: '11ms'
          }
        ],
        citations: [
          { docId: 'DOC-NORM-2026', title: 'Norma Técnica de Soldadura 2026', page: 1, hash: 'sha256:7f83b165' }
        ],
        graphResult: {
          deprecatedId: 'DOC-NORM-2020',
          activeId: 'DOC-NORM-2026',
          relation: 'deroga'
        }
      };
    } else {
      return {
        replyHtml: `He consultado el índice vectorial y los esquemas Arrow de <strong>${escapeHtml(centroId)}</strong>. Para tu consulta: <em>"${escapeHtml(query)}"</em>, no se registran infracciones normativas ni desviaciones en las matrices operativas activas.`,
        toolsUsed: [
          {
            name: 'buscar_chunks_similares',
            args: { centro_id: centroId, query: query, limit: 3 },
            result: { chunks: 3, top_score: 0.89 },
            status: 'ok',
            duration: '18ms'
          }
        ],
        citations: [
          { docId: 'DOC-NORM-2026', title: 'Norma Técnica de Soldadura 2026', page: 3, hash: 'sha256:7f83b165' }
        ]
      };
    }
  }

  function escapeHtml(text) {
    if (!text) return '';
    return String(text)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }

  loadModelCatalog();

  function formatTime() {
    const d = new Date();
    return d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' });
  }

})();
