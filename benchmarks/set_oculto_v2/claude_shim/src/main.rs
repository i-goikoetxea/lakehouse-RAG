//! Envoltorio de medición: reenvía todo al claude.exe real (CLAUDE_REAL) y deja en
//! LAKEHOUSE_SHIM_LOG un registro por invocación: argumentos (sin prompts largos),
//! inicio, duración, código de salida y la salida estándar completa (stream-json).
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() {
    let real = std::env::var("CLAUDE_REAL").expect("CLAUDE_REAL");
    let dir = std::env::var("LAKEHOUSE_SHIM_LOG").unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let t0 = Instant::now();
    let inicio_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut hijo = Command::new(&real)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("lanzar claude real");
    let mut entrada = hijo.stdin.take().unwrap();
    let hilo_in = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut buf);
        let _ = entrada.write_all(&buf);
        buf.len()
    });
    let mut salida = hijo.stdout.take().unwrap();
    let mut capturado = Vec::new();
    let mut buf = [0u8; 65536];
    let mut out = std::io::stdout();
    loop {
        let n = salida.read(&mut buf).unwrap_or(0);
        if n == 0 { break; }
        let _ = out.write_all(&buf[..n]);
        let _ = out.flush();
        capturado.extend_from_slice(&buf[..n]);
    }
    let bytes_in = hilo_in.join().unwrap_or(0);
    let estado = hijo.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
    let dur = t0.elapsed().as_millis();
    if !dir.is_empty() {
        let args_cortos: Vec<String> = args.iter().map(|a| if a.len() > 200 { format!("<{} caracteres>", a.len()) } else { a.clone() }).collect();
        let nombre = format!("{}/{}_{}.json", dir, inicio_ms, std::process::id());
        let cabecera = format!(
            "{{\"inicio_ms\":{},\"duracion_ms\":{},\"codigo\":{},\"bytes_stdin\":{},\"args\":{:?}}}\n",
            inicio_ms, dur, estado, bytes_in, args_cortos
        );
        if let Ok(mut f) = std::fs::File::create(nombre) {
            let _ = f.write_all(cabecera.as_bytes());
            let _ = f.write_all(&capturado);
        }
    }
    std::process::exit(estado);
}
