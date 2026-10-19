/**
 * Lakehouse RAG Industrial — Cliente de la ventana Tauri 2 (Linear + NotebookLM)
 *
 * Todo dato viene de los comandos IPC sobre LakehouseApp: no hay datos simulados.
 * Seguridad: el texto de documentos, tools y modelo se pinta SIEMPRE con textContent.
 * La única excepción es `replyHtml`, que el backend entrega saneado
 * (markdown_seguro.rs: Markdown real con todo el HTML crudo escapado y enlaces inertes).
 */

(function () {
  'use strict';

  // 1. Puente con Tauri
  const tauri = window.__TAURI__;
  const invoke = tauri && tauri.core ? tauri.core.invoke : null;
  const listen = tauri && tauri.event ? tauri.event.listen : null;

  const AVISO_MODELO_EMBEDDINGS =
    'La primera vez se carga el modelo de embeddings (~2,2 GB): puede tardar unos 100 s. La ventana sigue operativa.';
  const PREF = { centro: 'lakehouse_centro', modelo: 'lakehouse_modelo', tema: 'lakehouse_theme', paneles: 'lakehouse_paneles' };
  const LIMITES = { fuentes: [220, 520, 300], studio: [280, 760, 380] };
  const ANCHO_PLEGADO = 40;

  // 2. DOM
  const $ = (id) => document.getElementById(id);
  const htmlEl = document.documentElement;
  const workspace = $('workspace');
  const selectCentro = $('select-centro');
  const centroEstado = $('centro-estado');
  const btnToggleCentro = $('btn-toggle-centro');
  const btnEliminarCentro = $('btn-eliminar-centro');
  const selectModelo = $('select-modelo');
  const dropZone = $('drop-zone');
  const ingestaProgreso = $('ingesta-progreso');
  const dropProgress = $('dropzone-progress');
  const ingestaEstado = $('ingesta-estado');
  const sourcesList = $('sources-list');
  const sourcesCount = $('sources-count');
  const chatMessages = $('chat-messages');
  const chatColumna = $('chat-columna');
  const chatVacio = $('chat-vacio');
  const chatForm = $('chat-form');
  const chatInput = $('chat-input');
  const btnSend = $('btn-send');
  const btnIrFinal = $('btn-ir-final');
  const tabButtons = document.querySelectorAll('.studio-tabs .tab-btn');
  const tabContents = document.querySelectorAll('.studio-cuerpo .tab-content');

  // 3. Estado
  let centros = [];
  let ingestasEnCurso = 0;
  let colaIngesta = Promise.resolve();
  let avisoEmbeddingsMostrado = false;
  let consultaEnCurso = false;
  let pestanaElegidaPorUsuario = false;
  let fuentesUltimaRespuesta = [];

  // 4. Utilidades seguras
  function el(tag, clase, texto) {
    const nodo = document.createElement(tag);
    if (clase) nodo.className = clase;
    if (texto !== undefined && texto !== null) nodo.textContent = String(texto);
    return nodo;
  }
  function vaciar(nodo) { while (nodo.firstChild) nodo.removeChild(nodo.firstChild); }
  function mensajeError(err) { return err && err.message ? err.message : String(err); }
  function leer(clave) { try { return localStorage.getItem(clave); } catch (_) { return null; } }
  function guardar(clave, valor) { try { localStorage.setItem(clave, valor); } catch (_) { /* opcional */ } }
  function hora() { return new Date().toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' }); }
  function plural(n, singular, pluralTexto) { return `${n} ${n === 1 ? singular : pluralTexto}`; }
  function centroSeleccionado() { return centros.find((c) => c.id === selectCentro.value) || null; }

  function ficha(pares) {
    const dl = el('dl', 'ficha');
    pares.filter(([, v]) => v !== null && v !== undefined && v !== '').forEach(([k, v]) => {
      dl.appendChild(el('dt', null, k));
      dl.appendChild(el('dd', null, v));
    });
    return dl;
  }

  /** Ruta de apartados; sin el primer segmento si solo repite el documento («PO-ALM-03 Título…») */
  function miga(breadcrumb, docId) {
    const span = el('span', 'ruta-miga');
    let partes = String(breadcrumb || '').split(' > ');
    if (docId && partes.length > 1 && partes[0].startsWith(docId)) partes = partes.slice(1);
    partes.forEach((parte, i) => {
      if (i > 0) span.appendChild(el('span', 'sep', '›'));
      span.appendChild(document.createTextNode(parte));
    });
    return span;
  }

  // 5. Avisos: toast efímero + registro de actividad (fuera de la conversación)
  function registrar(texto, tipo = 'info', conToast = true) {
    const lista = $('actividad-lista');
    const li = el('li', tipo === 'error' ? 'error' : null);
    li.appendChild(el('time', null, hora()));
    li.appendChild(document.createTextNode(texto));
    lista.insertBefore(li, lista.firstChild);
    while (lista.children.length > 50) lista.removeChild(lista.lastChild);
    $('actividad-contador').textContent = String(lista.children.length);
    if (conToast) toast(texto, tipo);
  }

  function toast(texto, tipo) {
    const nodo = el('div', `toast ${tipo}`, texto);
    $('toasts').appendChild(nodo);
    setTimeout(() => nodo.remove(), tipo === 'error' ? 9000 : 4500);
  }

  // 6. Tema y ventana
  htmlEl.setAttribute('data-theme', leer(PREF.tema) || 'dark');
  $('btn-theme-toggle').addEventListener('click', () => {
    const nuevo = htmlEl.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
    htmlEl.setAttribute('data-theme', nuevo);
    guardar(PREF.tema, nuevo);
  });

  function ventanaActual() {
    return tauri && tauri.window && tauri.window.getCurrentWindow ? tauri.window.getCurrentWindow() : null;
  }
  [['btn-minimize', 'minimize'], ['btn-maximize', 'toggleMaximize'], ['btn-close', 'close']].forEach(([id, accion]) => {
    $(id).addEventListener('click', async () => {
      const ventana = ventanaActual();
      if (!ventana) return;
      try { await ventana[accion](); } catch (e) { registrar(`No se pudo ${accion}: ${mensajeError(e)}`, 'error'); }
    });
  });

  // 7. Paneles redimensionables y plegables (anchos recordados)
  const paneles = (() => {
    try { return JSON.parse(leer(PREF.paneles)) || {}; } catch (_) { return {}; }
  })();

  function aplicarPaneles() {
    ['fuentes', 'studio'].forEach((panel) => {
      const [min, max, defecto] = LIMITES[panel];
      const ancho = Math.min(max, Math.max(min, Number(paneles[`ancho_${panel}`]) || defecto));
      const plegado = Boolean(paneles[`plegado_${panel}`]);
      // El estilo en línea manda sobre la clase: el ancho plegado se fija aquí
      workspace.style.setProperty(`--ancho-${panel}`, plegado ? `${ANCHO_PLEGADO}px` : `${ancho}px`);
      workspace.classList.toggle(`${panel}-plegado`, plegado);
    });
  }

  function fijarAncho(panel, ancho) {
    const [min, max] = LIMITES[panel];
    paneles[`ancho_${panel}`] = Math.round(Math.min(max, Math.max(min, ancho)));
    paneles[`plegado_${panel}`] = false;
    aplicarPaneles();
  }

  function guardarPaneles() { guardar(PREF.paneles, JSON.stringify(paneles)); }

  document.querySelectorAll('.separador').forEach((separador) => {
    const panel = separador.dataset.separador;
    separador.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      try { separador.setPointerCapture(e.pointerId); } catch (_) { /* puntero sintético */ }
      separador.classList.add('activo');
      workspace.classList.add('redimensionando');
      const caja = workspace.getBoundingClientRect();
      const mover = (ev) => fijarAncho(panel, panel === 'fuentes' ? ev.clientX - caja.left : caja.right - ev.clientX);
      const soltar = () => {
        separador.removeEventListener('pointermove', mover);
        separador.classList.remove('activo');
        workspace.classList.remove('redimensionando');
        guardarPaneles();
      };
      separador.addEventListener('pointermove', mover);
      separador.addEventListener('pointerup', soltar, { once: true });
      separador.addEventListener('pointercancel', soltar, { once: true });
    });
    separador.addEventListener('dblclick', () => { fijarAncho(panel, LIMITES[panel][2]); guardarPaneles(); });
    separador.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      const actual = Number(paneles[`ancho_${panel}`]) || LIMITES[panel][2];
      const crece = (e.key === 'ArrowRight') === (panel === 'fuentes');
      fijarAncho(panel, actual + (crece ? 24 : -24));
      guardarPaneles();
    });
  });

  document.querySelectorAll('[data-plegar]').forEach((b) => b.addEventListener('click', () => {
    paneles[`plegado_${b.dataset.plegar}`] = true; aplicarPaneles(); guardarPaneles();
  }));
  document.querySelectorAll('[data-desplegar]').forEach((b) => b.addEventListener('click', () => {
    paneles[`plegado_${b.dataset.desplegar}`] = false; aplicarPaneles(); guardarPaneles();
  }));
  aplicarPaneles();

  // 8. Pestañas del Studio (no roban el foco si el usuario eligió otra)
  function activarPestana(tabId) {
    tabButtons.forEach((b) => {
      const activa = b.dataset.tab === tabId;
      b.classList.toggle('active', activa);
      if (activa) b.classList.remove('novedad');
    });
    tabContents.forEach((c) => c.classList.toggle('active', c.id === tabId));
    if (paneles.plegado_studio) { paneles.plegado_studio = false; aplicarPaneles(); guardarPaneles(); }
  }

  function marcarNovedad(tabId) {
    tabButtons.forEach((b) => { if (b.dataset.tab === tabId && !b.classList.contains('active')) b.classList.add('novedad'); });
  }

  tabButtons.forEach((b) => b.addEventListener('click', () => { pestanaElegidaPorUsuario = true; activarPestana(b.dataset.tab); }));

  // 9. Modelos (cambio en caliente: cada turno usa el modelo elegido)
  async function cargarModelos() {
    try {
      const modelos = await invoke('listar_modelos_claude');
      vaciar(selectModelo);
      modelos.forEach((m) => {
        const opcion = el('option', null, m.displayName);
        opcion.value = m.alias;
        opcion.title = [m.description, m.resolvedModel || m.alias].filter(Boolean).join(' · ');
        selectModelo.appendChild(opcion);
      });
      const preferido = leer(PREF.modelo);
      if (preferido && modelos.some((m) => m.alias === preferido)) selectModelo.value = preferido;
      selectModelo.disabled = modelos.length === 0;
      selectModelo.title = selectModelo.selectedOptions[0] ? selectModelo.selectedOptions[0].title : '';
    } catch (err) {
      vaciar(selectModelo);
      selectModelo.appendChild(el('option', null, 'Claude Code no disponible'));
      selectModelo.disabled = true;
      registrar(`No se pudo obtener el catálogo de modelos de Claude Code: ${mensajeError(err)}`, 'error');
    }
  }

  function etiquetaModelo() {
    const opcion = selectModelo.selectedOptions[0];
    return opcion ? opcion.textContent : '';
  }

  selectModelo.addEventListener('change', () => {
    guardar(PREF.modelo, selectModelo.value);
    selectModelo.title = selectModelo.selectedOptions[0] ? selectModelo.selectedOptions[0].title : '';
    registrar(`Modelo: ${etiquetaModelo()} (desde la próxima pregunta).`, 'info');
  });

  // 10. Centros
  async function cargarCentros(preferido) {
    try {
      centros = await invoke('listar_centros');
    } catch (err) {
      centros = [];
      registrar(`No se pudo leer el registro de centros: ${mensajeError(err)}`, 'error');
    }
    vaciar(selectCentro);
    if (centros.length === 0) selectCentro.appendChild(el('option', null, 'Sin centros: crea uno'));
    centros.forEach((c) => {
      const opcion = el('option', null, `${c.nombre} (${c.id})`);
      opcion.value = c.id;
      opcion.title = c.rutaDisco;
      selectCentro.appendChild(opcion);
    });
    const objetivo = preferido || leer(PREF.centro);
    if (objetivo && centros.some((c) => c.id === objetivo)) selectCentro.value = objetivo;
    selectCentro.disabled = centros.length === 0;
    await alCambiarCentro();
  }

  async function alCambiarCentro() {
    const centro = centroSeleccionado();
    btnToggleCentro.disabled = !centro;
    btnEliminarCentro.disabled = !centro;
    if (centro) guardar(PREF.centro, centro.id);
    centroEstado.textContent = centro ? (centro.activo ? 'Activo' : 'Inactivo') : '—';
    centroEstado.className = `chip ${centro && centro.activo ? 'chip-ok' : 'chip-aviso'}`;
    btnToggleCentro.textContent = centro && centro.activo ? 'Desactivar' : 'Activar';
    $('estado-dot').classList.toggle('online', Boolean(centro && centro.activo));
    $('estado-texto').textContent = centro
      ? (centro.activo ? `${centro.id} · vistas cargadas` : `${centro.id} · inactivo (sin acceso)`)
      : 'Sin centro seleccionado';
    pintarCentro(centro);
    await refrescarCatalogo();
  }

  selectCentro.addEventListener('change', alCambiarCentro);

  btnToggleCentro.addEventListener('click', async () => {
    const centro = centroSeleccionado();
    if (!centro) return;
    btnToggleCentro.disabled = true;
    try {
      if (centro.activo) {
        await invoke('desactivar_centro', { centroId: centro.id });
        registrar(`Centro ${centro.id} desactivado: sus vistas se han retirado.`, 'info');
      } else {
        const a = await invoke('activar_centro', { centroId: centro.id });
        registrar(`Centro ${centro.id} activo: ${a.vistas} vistas, ${plural(a.filas, 'fila', 'filas')}.`, 'ok');
      }
    } catch (err) {
      registrar(`No se pudo cambiar el estado del centro: ${mensajeError(err)}`, 'error');
    }
    await cargarCentros(centro.id);
  });

  function confirmar(titulo, texto) {
    const dialogo = $('dialogo-confirmar');
    $('confirmar-titulo').textContent = titulo;
    $('confirmar-texto').textContent = texto;
    dialogo.returnValue = 'no';
    dialogo.showModal();
    return new Promise((ok) => dialogo.addEventListener('close', () => ok(dialogo.returnValue === 'si'), { once: true }));
  }

  btnEliminarCentro.addEventListener('click', async () => {
    const centro = centroSeleccionado();
    if (!centro) return;
    const si = await confirmar(`Quitar «${centro.nombre}» del registro`,
      `El centro deja de aparecer en la app. Su carpeta (${centro.rutaDisco}) NO se borra y se puede volver a dar de alta con la misma ruta.`);
    if (!si) return;
    try {
      if (centro.activo) await invoke('desactivar_centro', { centroId: centro.id });
      await invoke('eliminar_centro', { centroId: centro.id });
      registrar(`Centro ${centro.id} quitado del registro (el disco no se ha tocado).`, 'ok');
    } catch (err) {
      registrar(`No se pudo quitar el centro: ${mensajeError(err)}`, 'error');
    }
    await cargarCentros();
  });

  $('btn-nuevo-centro').addEventListener('click', () => {
    $('centro-error').textContent = '';
    $('form-centro').reset();
    $('dialogo-centro').showModal();
  });
  $('btn-cancelar-centro').addEventListener('click', () => $('dialogo-centro').close());
  $('btn-elegir-carpeta').addEventListener('click', async () => {
    try {
      const carpeta = await invoke('seleccionar_directorio');
      if (carpeta) $('centro-ruta').value = carpeta;
    } catch (err) { $('centro-error').textContent = mensajeError(err); }
  });
  $('form-centro').addEventListener('submit', async (e) => {
    e.preventDefault();
    const centroId = $('centro-id').value.trim();
    const nombre = $('centro-nombre').value.trim();
    const ruta = $('centro-ruta').value.trim();
    try {
      await invoke('crear_centro', { centroId, ruta, nombre: nombre || null });
      await invoke('activar_centro', { centroId });
      $('dialogo-centro').close();
      registrar(`Centro ${centroId} creado en ${ruta} y activado.`, 'ok');
      await cargarCentros(centroId);
    } catch (err) { $('centro-error').textContent = mensajeError(err); }
  });

  function pintarCentro(centro) {
    const destino = $('tab-centro');
    vaciar(destino);
    if (!centro) { destino.appendChild(el('p', 'vacio', 'Selecciona o crea un centro.')); return; }
    const tarjeta = el('div', 'tarjeta');
    const titulo = el('h4', null, centro.nombre);
    titulo.appendChild(el('span', 'chip chip-info chip-mono', centro.id));
    titulo.appendChild(el('span', `chip ${centro.activo ? 'chip-ok' : 'chip-aviso'}`, centro.activo ? 'Activo' : 'Inactivo'));
    tarjeta.appendChild(titulo);
    tarjeta.appendChild(el('p', 'ruta-centro', centro.rutaDisco));
    destino.appendChild(tarjeta);

    const integridad = el('div', 'tarjeta');
    integridad.appendChild(el('h4', null, 'Integridad de las 6 tablas'));
    const resultado = el('div', 'texto-sutil', 'Comprueba filas huérfanas, ingestas STAGED y versiones vigentes duplicadas.');
    const boton = el('button', 'btn', 'Verificar integridad');
    boton.addEventListener('click', async () => {
      boton.disabled = true;
      resultado.textContent = 'Verificando…';
      try {
        const i = await invoke('integridad_centro', { centroId: centro.id });
        vaciar(resultado);
        resultado.appendChild(el('span', `chip ${i.integro ? 'chip-ok' : 'chip-aviso'}`, i.integro ? 'Íntegro' : 'Con incidencias'));
        resultado.appendChild(ficha([
          ['Tablas ausentes', String(i.tablasAusentes.length)],
          ['Ingestas STAGED', String(i.ingestasStaged.length)],
          ['Filas huérfanas', String(i.huerfanos)],
          ['Vigentes múltiples', String(i.versionesVigentesMultiples)],
        ]));
      } catch (err) {
        resultado.textContent = `No se pudo verificar: ${mensajeError(err)}`;
      } finally { boton.disabled = false; }
    });
    integridad.appendChild(resultado);
    integridad.appendChild(boton);
    destino.appendChild(integridad);
  }

  // 11. Catálogo real del centro (todas las revisiones)
  async function refrescarCatalogo() {
    vaciar(sourcesList);
    const centro = centroSeleccionado();
    if (!centro || !centro.activo) {
      sourcesCount.textContent = '0 documentos';
      sourcesList.appendChild(el('li', 'vacio', centro ? 'Activa el centro para ver su catálogo.' : 'Sin centro.'));
      return;
    }
    try {
      const documentos = await invoke('catalogo_centro', { centroId: centro.id });
      const vigentes = documentos.filter((d) => d.vigente).length;
      sourcesCount.textContent = plural(vigentes, 'vigente', 'vigentes');
      sourcesCount.title = `${plural(documentos.length, 'revisión', 'revisiones')} en total`;
      if (documentos.length === 0) sourcesList.appendChild(el('li', 'vacio', 'Centro vacío: ingiere un documento.'));
      documentos.forEach((doc) => sourcesList.appendChild(itemCatalogo(doc)));
    } catch (err) {
      sourcesList.appendChild(el('li', 'vacio', `No se pudo leer el catálogo: ${mensajeError(err)}`));
    }
  }

  function itemCatalogo(doc) {
    const li = el('li', `source-item${doc.vigente ? '' : ' derogado'}`);
    const cabecera = el('div', 'source-item-cabecera');
    cabecera.appendChild(el('span', 'source-name', `${doc.docId} · ${doc.revision}`));
    cabecera.appendChild(el('span', `chip ${doc.vigente ? 'chip-ok' : 'chip-error'}`, doc.vigente ? 'Vigente' : 'Derogada'));
    li.appendChild(cabecera);
    li.appendChild(el('div', 'source-title', doc.titulo));
    const vigencia = doc.vigente ? `en vigor desde ${doc.fechaVigor || '—'}` : `${doc.fechaVigor || '—'} → ${doc.fechaDerogacion || '—'}`;
    const prelacion = doc.nivelPrelacion === null || doc.nivelPrelacion === undefined ? '' : ` · prelación ${doc.nivelPrelacion}`;
    li.appendChild(el('div', 'source-item-meta', `${doc.tipoDocumento.replace(/_/g, ' ').toLowerCase()}${prelacion} · ${vigencia}`));
    return li;
  }

  $('btn-refresh-sources').addEventListener('click', refrescarCatalogo);

  // 12. Ingesta por ruta (diálogo nativo o drop de la ventana)
  async function elegirDocumentos() {
    if (!centroSeleccionado()) { registrar('Crea o selecciona un centro antes de ingerir.', 'error'); return; }
    try { encolarIngestas(await invoke('seleccionar_documentos')); } catch (err) {
      registrar(`No se pudo abrir el selector de archivos: ${mensajeError(err)}`, 'error');
    }
  }
  dropZone.addEventListener('click', elegirDocumentos);
  dropZone.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); elegirDocumentos(); } });
  ['dragover', 'drop'].forEach((tipo) => document.addEventListener(tipo, (e) => e.preventDefault()));

  function encolarIngestas(rutas) {
    const centro = centroSeleccionado();
    if (!centro) { registrar('Crea o selecciona un centro antes de ingerir.', 'error'); return; }
    (rutas || []).forEach((ruta) => { colaIngesta = colaIngesta.then(() => ingerir(ruta, centro.id)); });
  }

  async function ingerir(ruta, centroId) {
    ingestasEnCurso += 1;
    dropZone.classList.add('ingestando');
    const nombre = ruta.split(/[\\/]/).pop();
    mostrarProgreso(3, `${nombre}: en cola…`);
    try {
      const r = await invoke('ingestar_documento', { ruta, centroId });
      mostrarProgreso(100, `${r.archivo}: ${r.yaIngerido ? 'ya estaba ingerido' : 'ingerido'}`);
      registrar(r.yaIngerido
        ? `${r.archivo}: ya estaba ingerido (${r.docId} v${r.version}); no se ha vuelto a destilar.`
        : `${r.archivo} → ${r.docId} v${r.version}: ${plural(r.totalChunks, 'chunk', 'chunks')}, ${plural(r.totalHitos, 'hito', 'hitos')}, ${plural(r.totalTablas, 'tabla', 'tablas')}, ${plural(r.totalCeldas, 'celda', 'celdas')}, ${plural(r.totalAristas, 'arista', 'aristas')}${r.versionesDerogadas.length ? `; deroga v${r.versionesDerogadas.join(', v')}` : ''}.`, 'ok');
      if (r.aviso) registrar(r.aviso, 'error');
      if (centroId === selectCentro.value) await refrescarCatalogo();
    } catch (err) {
      mostrarProgreso(0, `${nombre}: error`);
      registrar(`No se pudo ingerir ${nombre}: ${mensajeError(err)}`, 'error');
    } finally {
      ingestasEnCurso -= 1;
      if (ingestasEnCurso === 0) {
        dropZone.classList.remove('ingestando');
        setTimeout(() => { if (ingestasEnCurso === 0) ingestaProgreso.hidden = true; }, 4000);
      }
    }
  }

  function mostrarProgreso(porcentaje, texto) {
    ingestaProgreso.hidden = false;
    dropProgress.style.width = `${Math.max(0, Math.min(100, porcentaje))}%`;
    ingestaEstado.textContent = texto;
  }

  function alProgresoIngesta(evento) {
    const p = evento.payload || {};
    mostrarProgreso(p.porcentaje, `${p.archivo}: ${p.descripcion}…${p.etapa === 'destilacion' ? ' (30–60 s)' : ''}`);
    if (p.etapa === 'embeddings' && !avisoEmbeddingsMostrado) {
      avisoEmbeddingsMostrado = true;
      registrar(AVISO_MODELO_EMBEDDINGS, 'info');
    }
  }

  // 13. Chat: streaming por evento `chat-stream`, Markdown renderizado en Rust a intervalos,
  // pregunta anclada arriba y scroll pegado al final solo si el usuario no se ha alejado.
  const UMBRAL_PEGADO_PX = 24;
  const INTERVALO_RENDER_MS = 120;
  let pegadoAlFinal = true;
  let turnoActivo = null;
  let contadorTurnos = 0;
  let espaciador = null;

  function ajustarAlturaEntrada() {
    chatInput.style.height = 'auto';
    chatInput.style.height = `${chatInput.scrollHeight}px`; // el tope (25vh) y el scroll interno van en CSS
  }
  chatInput.addEventListener('input', ajustarAlturaEntrada);
  chatInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); if (!turnoActivo) chatForm.requestSubmit(); }
  });
  document.querySelectorAll('.ejemplo').forEach((b) => b.addEventListener('click', () => {
    chatInput.value = b.textContent; ajustarAlturaEntrada(); chatInput.focus();
  }));

  function distanciaAlFinal() { return chatMessages.scrollHeight - chatMessages.scrollTop - chatMessages.clientHeight; }
  function irAlFinal() {
    chatMessages.scrollTop = chatMessages.scrollHeight;
    pegadoAlFinal = true;
    btnIrFinal.hidden = true;
  }
  chatMessages.addEventListener('scroll', () => {
    pegadoAlFinal = distanciaAlFinal() <= UMBRAL_PEGADO_PX;
    btnIrFinal.hidden = pegadoAlFinal;
  });
  btnIrFinal.addEventListener('click', irAlFinal);

  // El espaciador final deja la última pregunta arriba mientras la respuesta crece debajo
  function anclarPregunta(pregunta) {
    if (espaciador) espaciador.remove();
    espaciador = el('div', 'espaciador-ancla');
    chatColumna.appendChild(espaciador);
    ajustarEspaciador(pregunta);
    irAlFinal();
  }
  function ajustarEspaciador(pregunta) {
    if (!espaciador || !pregunta) return;
    // La parte superior del espaciador no depende de su altura: se mide sin colapsarlo
    const disponible = chatMessages.clientHeight - 40;
    const ocupado = espaciador.getBoundingClientRect().top - pregunta.getBoundingClientRect().top;
    espaciador.style.height = `${Math.max(0, disponible - ocupado)}px`;
  }
  function seguirAlFinal(turno) {
    ajustarEspaciador(turno.pregunta);
    if (pegadoAlFinal) irAlFinal();
  }

  function modoDetener(activo) {
    btnSend.classList.toggle('detener', activo);
    btnSend.title = activo ? 'Detener la respuesta' : 'Enviar (Intro)';
    btnSend.setAttribute('aria-label', activo ? 'Detener' : 'Enviar');
  }

  async function detenerTurno() {
    if (!turnoActivo) return;
    turnoActivo.detenido = true;
    try { await invoke('detener_respuesta', { turnoId: turnoActivo.id }); } catch (err) {
      registrar(`No se pudo detener la respuesta: ${mensajeError(err)}`, 'error');
    }
  }

  chatForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (turnoActivo) { await detenerTurno(); return; }
    const consulta = chatInput.value.trim();
    if (!consulta || consultaEnCurso) return;
    const centro = centroSeleccionado();
    if (!centro || !centro.activo) { registrar('Activa un centro antes de preguntar.', 'error'); return; }
    if (!selectModelo.value) { registrar('Selecciona un modelo disponible antes de preguntar.', 'error'); return; }
    chatInput.value = '';
    ajustarAlturaEntrada();
    chatVacio.hidden = true;
    const pregunta = el('div', 'mensaje-usuario', consulta);
    chatColumna.appendChild(pregunta);
    await consultarAgente(consulta, centro.id, selectModelo.value, etiquetaModelo(), pregunta);
  });

  function segundosDesde(inicio) { return Math.round((Date.now() - inicio) / 1000); }

  function actualizarTrabajando(turno) {
    const pendiente = turno.lineas.find((l) => l.pendiente);
    if (pendiente) {
      turno.trabajando.hidden = false;
      turno.trabajando.textContent = `Consultando ${pendiente.nombre}… ${segundosDesde(turno.inicio)} s`;
    } else if (!turno.texto) {
      turno.trabajando.hidden = false;
      turno.trabajando.textContent = `Pensando con ${turno.etiqueta}… ${segundosDesde(turno.inicio)} s`;
    } else {
      turno.trabajando.hidden = true;
    }
  }

  function pintarHtmlSeguro(nodo, data) {
    nodo.innerHTML = data.replyHtml; // único punto de HTML: saneado en Rust (markdown_seguro.rs)
  }

  function programarRender(turno) {
    if (turno.temporizador) return;
    const espera = Math.max(0, INTERVALO_RENDER_MS - (Date.now() - turno.ultimoRender));
    turno.temporizador = setTimeout(async () => {
      turno.temporizador = null;
      turno.ultimoRender = Date.now();
      try {
        const html = await invoke('renderizar_markdown', { texto: turno.texto });
        if (turnoActivo !== turno) return;
        pintarHtmlSeguro(turno.respuesta, { replyHtml: html });
        seguirAlFinal(turno);
      } catch (_) { /* el render final llega con la respuesta completa */ }
    }, espera);
  }

  function alEventoChat(evento) {
    const datos = evento.payload || {};
    const turno = turnoActivo;
    if (!turno || datos.turnoId !== turno.id) return;
    if (datos.tipo === 'texto') {
      turno.texto += datos.texto;
      programarRender(turno);
    } else if (datos.tipo === 'herramientaInicio') {
      const boton = el('div', 'tool-linea en-curso');
      boton.appendChild(el('span', 'estado', '●'));
      boton.appendChild(el('span', 'nombre', datos.nombre));
      turno.tools.appendChild(boton);
      turno.lineas.push({ nombre: datos.nombre, nodo: boton, pendiente: true });
    } else if (datos.tipo === 'herramientaFin') {
      const linea = turno.lineas.find((l) => l.pendiente && l.nombre === datos.nombre);
      if (linea) {
        linea.pendiente = false;
        linea.nodo.classList.remove('en-curso');
        if (datos.esError) linea.nodo.classList.add('fallo');
        linea.nodo.querySelector('.estado').textContent = datos.esError ? '⚠' : '✓';
      }
    }
    actualizarTrabajando(turno);
    seguirAlFinal(turno);
  }

  async function consultarAgente(consulta, centroId, modelo, etiqueta, pregunta) {
    consultaEnCurso = true;
    pestanaElegidaPorUsuario = false;
    chatColumna.querySelectorAll('.mensaje-asistente.ultimo').forEach((m) => m.classList.remove('ultimo'));
    const mensaje = el('div', 'mensaje-asistente ultimo');
    const turno = {
      id: `turno-${Date.now()}-${++contadorTurnos}`,
      mensaje, pregunta, etiqueta,
      tools: el('div', 'tools-usadas'),
      trabajando: el('div', 'trabajando'),
      respuesta: el('div', 'respuesta en-vivo'),
      texto: '', lineas: [], inicio: Date.now(), ultimoRender: 0, temporizador: null, detenido: false,
    };
    mensaje.append(turno.tools, turno.trabajando, turno.respuesta);
    chatColumna.appendChild(mensaje); // tras su pregunta; anclarPregunta recoloca el espaciador al final
    anclarPregunta(pregunta);
    turnoActivo = turno;
    modoDetener(true);
    actualizarTrabajando(turno);
    const cronometro = setInterval(() => actualizarTrabajando(turno), 1000);
    try {
      const data = await invoke('enviar_mensaje_chat', { mensaje: consulta, centroId, modelo, turnoId: turno.id });
      turnoActivo = null;
      mensaje.replaceChildren();
      pintarRespuesta(mensaje, data, segundosDesde(turno.inicio), etiqueta);
    } catch (err) {
      turnoActivo = null;
      turno.trabajando.remove();
      turno.respuesta.classList.remove('en-vivo');
      if (!turno.texto) turno.respuesta.remove();
      const texto = turno.detenido ? 'Respuesta detenida.' : `No se pudo responder: ${mensajeError(err)}`;
      mensaje.appendChild(el('div', turno.detenido ? 'texto-sutil' : 'error-consulta', texto));
    } finally {
      clearInterval(cronometro);
      clearTimeout(turno.temporizador);
      consultaEnCurso = false;
      modoDetener(false);
      seguirAlFinal(turno);
    }
  }

  function botonCopiar(obtenerTexto) {
    const boton = el('button', 'btn-accion');
    boton.type = 'button';
    boton.title = 'Copiar';
    boton.appendChild(el('span', 'copiar', '⧉ Copiar'));
    boton.appendChild(el('span', 'ok', '✓ Copiado'));
    boton.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(obtenerTexto());
        boton.classList.add('copiado');
        setTimeout(() => boton.classList.remove('copiado'), 1500);
      } catch (_) { registrar('No se pudo copiar al portapapeles.', 'error'); }
    });
    return boton;
  }

  function pintarRespuesta(mensaje, data, segundos, etiqueta) {
    const tools = data.toolsUsed || [];
    if (tools.length) {
      const bloque = el('div', 'tools-usadas');
      tools.forEach((tool) => bloque.appendChild(lineaTool(tool)));
      mensaje.appendChild(bloque);
    }

    const respuesta = el('div', 'respuesta');
    pintarHtmlSeguro(respuesta, data);
    mensaje.appendChild(respuesta);

    const notas = notasYCondiciones(data.citations || []);
    if (notas.length) {
      const bloque = el('div', 'notas');
      bloque.appendChild(el('h4', null, 'Notas al pie y condiciones'));
      const lista = el('ul');
      notas.forEach(([fuente, textoNota]) => {
        const li = el('li');
        li.appendChild(el('span', 'chip chip-mono', fuente));
        li.appendChild(document.createTextNode(textoNota));
        lista.appendChild(li);
      });
      bloque.appendChild(lista);
      mensaje.appendChild(bloque);
    }

    const fuentes = data.fuentes || [];
    if (fuentes.length) {
      const fila = el('div', 'fuentes-respuesta');
      fila.appendChild(el('span', 'etiqueta', 'Fuentes'));
      fuentes.forEach((fuente, indice) => fila.appendChild(chipFuente(fuente, indice, fuentes)));
      mensaje.appendChild(fila);
    }

    const acciones = el('div', 'acciones-respuesta');
    acciones.appendChild(botonCopiar(() => respuesta.innerText));
    acciones.appendChild(el('span', 'chip', etiqueta));
    acciones.appendChild(el('span', 'texto-sutil', `${segundos} s · ${hora()}`));
    mensaje.appendChild(acciones);

    actualizarStudio(data);
  }

  function lineaTool(tool) {
    const ok = tool.status === 'ok';
    const boton = el('button', `tool-linea${ok ? '' : ' fallo'}`);
    boton.type = 'button';
    boton.title = 'Ver parámetros y resultado en el Studio';
    boton.appendChild(el('span', 'estado', ok ? '✓' : '⚠'));
    boton.appendChild(el('span', 'nombre', tool.name));
    boton.appendChild(el('span', 'resumen', tool.resumen || ''));
    boton.appendChild(el('span', 'duracion', tool.duration));
    boton.addEventListener('click', () => { pintarDetalleTool(tool); pestanaElegidaPorUsuario = true; activarPestana('tab-tools'); });
    return boton;
  }

  function notasYCondiciones(citas) {
    const vistas = new Map();
    citas.forEach((c) => {
      const fuente = `${c.docId}${c.revision ? ` ${c.revision}` : ''}`;
      if (c.notaPie) vistas.set(`n|${fuente}|${c.notaPie}`, [fuente, `Nota: ${c.notaPie}`]);
      if (c.condicionAplicacion) vistas.set(`c|${fuente}|${c.condicionAplicacion}`, [fuente, `Condición: ${c.condicionAplicacion}`]);
    });
    return Array.from(vistas.values());
  }

  function chipFuente(fuente, indice, todas) {
    const chip = el('button', 'chip-fuente');
    chip.type = 'button';
    chip.appendChild(el('span', 'doc', fuente.docId));
    if (fuente.revision) chip.appendChild(el('span', null, fuente.revision));
    const n = Math.max(fuente.apartados.length, 1);
    chip.appendChild(el('span', 'contador', String(n)));
    chip.title = fuente.apartados.length ? fuente.apartados.join('\n') : `${plural(fuente.citas.length, 'cita', 'citas')}`;
    chip.addEventListener('click', () => {
      pintarFuentes(todas, indice);
      pestanaElegidaPorUsuario = true;
      activarPestana('tab-fuentes');
    });
    return chip;
  }

  // 14. Studio
  function actualizarStudio(data) {
    fuentesUltimaRespuesta = data.fuentes || [];
    pintarFuentes(fuentesUltimaRespuesta, -1);
    const novedades = [];
    if (fuentesUltimaRespuesta.length) novedades.push('tab-fuentes');
    if (data.graphResult) { pintarGrafo(data.graphResult); novedades.unshift('tab-grafo'); }
    if (data.matrixResult) { pintarTabla(data.matrixResult); novedades.unshift('tab-tabla'); }
    const tools = data.toolsUsed || [];
    if (tools.length) pintarDetalleTool(tools[tools.length - 1]);
    if (!novedades.length) return;
    if (!pestanaElegidaPorUsuario) activarPestana(novedades[0]);
    novedades.forEach(marcarNovedad);
  }

  function pintarFuentes(fuentes, seleccionada) {
    const destino = $('tab-fuentes');
    vaciar(destino);
    if (!fuentes.length) { destino.appendChild(el('p', 'vacio', 'La última respuesta no trae fuentes citadas.')); return; }
    fuentes.forEach((fuente, indice) => {
      const tarjeta = el('div', `tarjeta${indice === seleccionada ? ' destacada' : ''}`);
      const titulo = el('h4');
      titulo.appendChild(el('span', 'chip chip-info chip-mono', fuente.docId));
      if (fuente.revision) titulo.appendChild(el('span', 'chip', fuente.revision));
      titulo.appendChild(el('span', 'texto-sutil', plural(fuente.citas.length, 'cita', 'citas')));
      tarjeta.appendChild(titulo);
      const lista = el('ul', 'lista-apartados');
      agruparPorApartado(fuente.citas).forEach((a) => {
        const li = el('li');
        li.appendChild(a.breadcrumb ? miga(a.breadcrumb, fuente.docId) : el('span', 'texto-sutil', 'Sin apartado (tabla o arista)'));
        li.appendChild(ficha([
          ['Tools', a.tools.join(', ')],
          ['Condición', a.condiciones.join(' · ')],
          ['Nota al pie', a.notas.join(' · ')],
          ['Fragmentos', a.fragmentos.length > 3 ? `${a.fragmentos.slice(0, 3).join(', ')} y ${a.fragmentos.length - 3} más` : a.fragmentos.join(', ')],
        ]));
        lista.appendChild(li);
      });
      tarjeta.appendChild(lista);
      destino.appendChild(tarjeta);
      if (indice === seleccionada) setTimeout(() => tarjeta.scrollIntoView({ block: 'nearest' }), 0);
    });
  }

  /** Un apartado por breadcrumb con sus tools, condiciones, notas y fragmentos sin repetir */
  function agruparPorApartado(citas) {
    const apartados = new Map();
    const anadir = (lista, valor) => { if (valor && !lista.includes(valor)) lista.push(valor); };
    citas.forEach((c) => {
      const clave = c.breadcrumb || '';
      if (!apartados.has(clave)) apartados.set(clave, { breadcrumb: c.breadcrumb, tools: [], condiciones: [], notas: [], fragmentos: [] });
      const a = apartados.get(clave);
      anadir(a.tools, c.herramienta);
      anadir(a.condiciones, c.condicionAplicacion);
      anadir(a.notas, c.notaPie);
      anadir(a.fragmentos, c.fragmentoId);
    });
    return Array.from(apartados.values());
  }

  function textoCita(cita) { return cita ? `${cita.doc_id || ''} ${cita.revision || ''}`.trim() : ''; }

  function pintarTabla(r) {
    const destino = $('tab-tabla');
    vaciar(destino);
    if (r.tipo === 'matriz') {
      const cabeceraMatriz = el('div', 'fila-chips');
      cabeceraMatriz.appendChild(el('span', 'chip chip-info chip-mono', '03_matriz_operativa'));
      cabeceraMatriz.appendChild(el('span', 'chip chip-lila', plural(r.total ?? (r.filas || []).length, 'fila', 'filas')));
      if (r.fechaReferencia) cabeceraMatriz.appendChild(el('span', 'chip chip-aviso', `a ${r.fechaReferencia}`));
      destino.appendChild(cabeceraMatriz);
      const rejilla = el('div', 'rejilla-fichas');
      (r.filas || []).forEach((f) => {
        const tarjeta = el('div', 'tarjeta');
        const titulo = el('h4', null, f.fase_operativa);
        if (f.registro_codigo) titulo.appendChild(el('span', 'chip chip-mono', f.registro_codigo));
        tarjeta.appendChild(titulo);
        tarjeta.appendChild(ficha([
          ['Responsable', f.rol_responsable],
          ['Ejecutor', f.rol_ejecutor],
          ['Registro', [f.registro_codigo, f.registro_nombre].filter(Boolean).join(' · ')],
          ['Objeto', f.objeto_alcance],
          ['Criterio', f.criterio_aceptacion],
          ['Condición', f.condicion_aplicacion],
          ['Cita', textoCita(f.cita)],
        ]));
        if (f.cita && f.cita.breadcrumb) tarjeta.appendChild(miga(f.cita.breadcrumb, f.cita.doc_id));
        rejilla.appendChild(tarjeta);
      });
      destino.appendChild(rejilla);
      return;
    }
    const cabeceraLookup = el('div', 'fila-chips');
    cabeceraLookup.appendChild(el('span', `chip ${r.estado === 'resuelto' ? 'chip-ok' : 'chip-aviso'}`, String(r.estado || '').replace(/_/g, ' ')));
    if (r.fecha_referencia) cabeceraLookup.appendChild(el('span', 'chip chip-aviso', `a ${r.fecha_referencia}`));
    destino.appendChild(cabeceraLookup);
    const tarjeta = el('div', 'tarjeta');
    if (r.tabla) tarjeta.appendChild(el('h4', null, r.tabla.titulo_tabla || r.tabla.tabla_id));
    const ejes = r.tabla ? [r.tabla.eje_x && r.tabla.eje_x.nombre, r.tabla.eje_y && r.tabla.eje_y.nombre] : [];
    if (r.entrada) {
      tarjeta.appendChild(ficha([
        [ejes[0] || 'Eje x', String(r.entrada.valor_x)],
        [ejes[1] || 'Eje y', String(r.entrada.valor_y)],
        ['Eje z', r.entrada.eje_z],
      ]));
    }
    if (r.estado === 'resuelto') {
      tarjeta.appendChild(el('div', 'valor-resuelto', `${r.valor_resultado_texto}${r.unidad_medida ? ` ${r.unidad_medida}` : ''}`));
      tarjeta.appendChild(ficha([['Rango x', r.rango_x], ['Rango y', r.rango_y], ['Cita', textoCita(r.cita)], ['Celda', r.cita && r.cita.celda_id]]));
      if (r.nota_pie_condicion) {
        const nota = el('div', 'nota-obligatoria');
        nota.appendChild(el('strong', null, 'Nota al pie obligatoria: '));
        nota.appendChild(document.createTextNode(r.nota_pie_condicion));
        tarjeta.appendChild(nota);
      }
    } else if (Array.isArray(r.candidatas)) {
      tarjeta.appendChild(el('p', 'texto-sutil', r.mensaje));
      r.candidatas.forEach((c) => {
        const sub = el('div', `tarjeta${c.prevalece ? ' destacada' : ''}`);
        sub.appendChild(ficha([['Valor', c.valor_resultado_texto], ['Nota', c.nota_pie_condicion], ['Eje z', c.eje_z], ['Prevalece', c.prevalece ? 'sí' : ''], ['Cita', textoCita(c.cita)]]));
        tarjeta.appendChild(sub);
      });
    } else {
      tarjeta.appendChild(el('p', 'texto-sutil', r.mensaje || 'Sin resultado.'));
      (r.tablas || []).forEach((t) => tarjeta.appendChild(ficha([['Tabla', t.titulo_tabla || t.tabla_id], ['Id', t.tabla_id], ['Documento', t.doc_id]])));
    }
    destino.appendChild(tarjeta);
  }

  function pintarGrafo(g) {
    const destino = $('tab-grafo');
    vaciar(destino);
    const cabeceraGrafo = el('div', 'fila-chips');
    cabeceraGrafo.appendChild(el('span', 'chip chip-info chip-mono', g.nodoOrigen));
    cabeceraGrafo.appendChild(el('span', 'chip chip-lila', `${plural((g.nodos || []).length, 'nodo', 'nodos')} · ${plural((g.aristas || []).length, 'arista', 'aristas')}`));
    if (g.truncado) cabeceraGrafo.appendChild(el('span', 'chip chip-aviso', 'truncado'));
    destino.appendChild(cabeceraGrafo);

    const porRelacion = new Map();
    (g.aristas || []).forEach((a) => {
      if (!porRelacion.has(a.tipo_relacion)) porRelacion.set(a.tipo_relacion, []);
      porRelacion.get(a.tipo_relacion).push(a);
    });
    porRelacion.forEach((aristas, relacion) => {
      const grupo = el('div', 'tarjeta grupo-relacion');
      grupo.appendChild(el('h5', null, `${relacion.replace(/_/g, ' ').toLowerCase()} (${aristas.length})`));
      const lista = el('ul', 'lista-apartados');
      aristas.forEach((a) => {
        const li = el('li');
        const linea = el('div', 'fila-chips');
        linea.appendChild(el('span', 'chip chip-mono', a.origen));
        linea.appendChild(el('span', 'texto-sutil', '→'));
        linea.appendChild(el('span', 'chip chip-info chip-mono', a.destino));
        li.appendChild(linea);
        if (a.contexto) li.appendChild(el('p', 'texto-sutil', a.contexto));
        lista.appendChild(li);
      });
      grupo.appendChild(lista);
      destino.appendChild(grupo);
    });

    const hubs = (g.nodos || []).filter((n) => n.hub_resumido);
    hubs.forEach((n) => {
      const tarjeta = el('div', 'tarjeta nodo-hub');
      tarjeta.appendChild(el('h4', null, `${n.id} · hub resumido`));
      const top = n.resumen_hub && n.resumen_hub.top_relaciones ? n.resumen_hub.top_relaciones.map((t) => `${t.tipo_relacion} ×${t.aristas}`).join(', ') : '';
      tarjeta.appendChild(ficha([['Grado', String(n.grado)], ['Relaciones', top]]));
      destino.appendChild(tarjeta);
    });
  }

  function pintarDetalleTool(tool) {
    const destino = $('tab-tools');
    vaciar(destino);
    const tarjeta = el('div', 'tarjeta');
    const titulo = el('h4');
    titulo.appendChild(el('span', 'chip chip-info chip-mono', tool.name));
    titulo.appendChild(el('span', `chip ${tool.status === 'ok' ? 'chip-ok' : 'chip-error'}`, tool.status));
    titulo.appendChild(el('span', 'texto-sutil', tool.duration));
    tarjeta.appendChild(titulo);
    if (tool.resumen) tarjeta.appendChild(el('p', 'texto-sutil', tool.resumen));
    tarjeta.appendChild(el('h5', 'texto-sutil', 'Parámetros'));
    tarjeta.appendChild(el('pre', 'json', JSON.stringify(tool.args, null, 2)));
    tarjeta.appendChild(el('h5', 'texto-sutil', 'Resultado'));
    tarjeta.appendChild(el('pre', 'json', JSON.stringify(tool.result, null, 2)));
    destino.appendChild(tarjeta);
  }

  // 15. Arranque
  async function iniciar() {
    if (!invoke || !listen) {
      registrar('Esta interfaz necesita la aplicación de escritorio (Tauri): no hay datos simulados.', 'error');
      return;
    }
    await listen('ingesta-progreso', alProgresoIngesta);
    await listen('chat-stream', alEventoChat);
    await listen('documentos-soltados', (evento) => { dropZone.classList.remove('drag-active'); encolarIngestas(evento.payload); });
    await listen('documentos-arrastre', (evento) => dropZone.classList.toggle('drag-active', evento.payload === 'entrar'));
    await Promise.all([cargarCentros(), cargarModelos()]);
  }

  iniciar();
})();
