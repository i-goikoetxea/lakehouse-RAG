// Conduce la ventana REAL de Tauri (WebView2) por el protocolo de depuración de Chromium.
// Requiere lanzar la app con WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222.
// Uso: node cdp_ventana.mjs <script-js | @archivo.js> [captura.png] [timeout_s]
//   Evalúa el JS en la página (await permitido) y, si se indica, guarda una captura PNG.
import { readFileSync, writeFileSync } from 'node:fs';

const [, , fuente, captura, timeoutTexto] = process.argv;
const codigo = fuente && fuente.startsWith('@') ? readFileSync(fuente.slice(1), 'utf8') : (fuente || 'document.title');
const timeoutMs = Number(timeoutTexto || 600) * 1000;

const objetivos = await (await fetch('http://127.0.0.1:9222/json')).json();
const pagina = objetivos.find((o) => o.type === 'page');
if (!pagina) throw new Error('No hay página WebView2 depurable');

const ws = new WebSocket(pagina.webSocketDebuggerUrl);
let siguiente = 0;
const pendientes = new Map();
ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pendientes.has(msg.id)) {
    pendientes.get(msg.id)(msg);
    pendientes.delete(msg.id);
  }
};
await new Promise((ok) => { ws.onopen = ok; });
const enviar = (method, params = {}) => new Promise((ok) => {
  const id = ++siguiente;
  pendientes.set(id, ok);
  ws.send(JSON.stringify({ id, method, params }));
});

const temporizador = setTimeout(() => { console.error('TIMEOUT'); process.exit(2); }, timeoutMs);
const r = await enviar('Runtime.evaluate', {
  expression: `(async () => { ${codigo} })()`,
  awaitPromise: true,
  returnByValue: true,
});
if (r.result.exceptionDetails) {
  console.error('EXCEPCION', JSON.stringify(r.result.exceptionDetails));
} else {
  console.log(JSON.stringify(r.result.result.value, null, 2));
}
if (captura) {
  const img = await enviar('Page.captureScreenshot', { format: 'png' });
  writeFileSync(captura, Buffer.from(img.result.data, 'base64'));
  console.log('captura:', captura);
}
clearTimeout(temporizador);
ws.close();
