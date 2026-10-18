// Pregunta por la UI real (anteponer: const PARAMS = [pregunta, alias_modelo];)
const [pregunta, modelo] = PARAMS;
const sel = document.getElementById('select-modelo');
if (sel.value !== modelo) { sel.value = modelo; sel.dispatchEvent(new Event('change')); }
const antes = document.querySelectorAll('.mensaje-asistente').length;
const entrada = document.getElementById('chat-input');
entrada.value = pregunta;
document.getElementById('chat-form').requestSubmit();
const t0 = Date.now();
for (let i = 0; i < 400; i++) {
  await new Promise((r) => setTimeout(r, 1500));
  const msgs = document.querySelectorAll('.mensaje-asistente');
  const ultimo = msgs[msgs.length - 1];
  if (msgs.length > antes && !ultimo.querySelector('.cargando')) {
    await new Promise((r) => setTimeout(r, 400));
    return {
      segundos: Math.round((Date.now() - t0) / 1000),
      tools: [...ultimo.querySelectorAll('.tool-linea')].map((n) => n.textContent),
      fuentes: [...ultimo.querySelectorAll('.chip-fuente')].map((c) => c.textContent),
      notas: ultimo.querySelector('.notas')?.textContent,
      pestana: document.querySelector('.tab-btn.active')?.textContent,
      scrollY_en_mensajes: [...ultimo.querySelectorAll('*')].filter((n) => n.scrollHeight > n.clientHeight + 2 && getComputedStyle(n).overflowY !== 'visible' && !n.closest('.md-tabla-envoltorio')).length,
      error: ultimo.querySelector('.error-consulta')?.textContent,
    };
  }
}
return 'timeout';
