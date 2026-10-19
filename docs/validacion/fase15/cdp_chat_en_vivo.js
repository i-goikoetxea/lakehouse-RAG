// Prueba real del chat en vivo (Fase 15) en la ventana Tauri: se evalúa en la página con
// `node docs/validacion/fase14/cdp_ventana.mjs @docs/validacion/fase15/cdp_chat_en_vivo.js`.
// Envía una pregunta y muestrea cada 250 ms el texto visible, el estado «trabajando», las tools
// y la distancia al final del scroll. Devuelve la línea de tiempo.
const espera = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (s) => document.querySelector(s);
for (let i = 0; i < 40 && !$('#select-centro').value; i++) await espera(250);
const modelo = $('#select-modelo');
for (let i = 0; i < 80 && modelo.disabled; i++) await espera(250);
const haiku = Array.from(modelo.options).find((o) => o.value === 'haiku');
if (haiku) { modelo.value = 'haiku'; modelo.dispatchEvent(new Event('change')); }
const pregunta = window.__preguntaPrueba || 'Dame el listado completo de los registros que exige el PO-HOR-07 vigente, con la fase y quién firma cada uno.';
$('#chat-input').value = pregunta;
$('#chat-form').requestSubmit();
const inicio = Date.now();
const linea = [];
let ultimo = '';
while (Date.now() - inicio < 240000) {
  await espera(250);
  const vivo = $('.respuesta.en-vivo');
  const trabajando = $('.trabajando');
  const caracteres = vivo ? vivo.innerText.length : -1;
  const estado = [
    `${((Date.now() - inicio) / 1000).toFixed(1)}s`,
    `texto=${caracteres}`,
    `trab=${trabajando && !trabajando.hidden ? trabajando.textContent : '-'}`,
    `tools=${Array.from(document.querySelectorAll('.mensaje-asistente.ultimo .tool-linea')).map((t) => t.textContent.slice(0, 40)).join('|')}`,
    `dist=${Math.round($('#chat-messages').scrollHeight - $('#chat-messages').scrollTop - $('#chat-messages').clientHeight)}`,
    `boton=${$('#btn-send').classList.contains('detener') ? 'detener' : 'enviar'}`,
  ].join(' ');
  const clave = estado.replace(/^\S+ /, '');
  if (clave !== ultimo) { linea.push(estado); ultimo = clave; }
  if (!$('#btn-send').classList.contains('detener') && Date.now() - inicio > 2000) break;
}
const final = $('.mensaje-asistente.ultimo');
return {
  pasos: linea.length,
  linea: linea.slice(0, 60),
  final: final ? final.innerText.slice(0, 600) : null,
  pregunta_top: Math.round(Array.from(document.querySelectorAll('.mensaje-usuario')).pop().getBoundingClientRect().top),
};
