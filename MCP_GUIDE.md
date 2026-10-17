# Guía de Instalación e Integración MCP — Lakehouse RAG Industrial

> **Propósito:** Este documento proporciona las instrucciones exactas para que **cualquier agente de IA** (Claude Desktop, Claude Code, Cursor, Windsurf, Gemini) o desarrollador pueda integrar y consumir las herramientas deterministas de este Lakehouse RAG mediante el protocolo estándar **MCP (Model Context Protocol)**.

---

## 1. Comando Rápido de Autoinstalación

El propio binario incluye un comando que genera la configuración lista para copiar y pegar:

```powershell
cargo run -- --mcp-info
```
O con el binario compilado:
```powershell
.\target\release\lakehouse_rag.exe --mcp-info
```

---

## 2. Instalación en Claude Desktop

Abre o edita el archivo de configuración de Claude Desktop:
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`
- **macOS / Linux:** `~/.config/Claude/claude_desktop_config.json`

Añade la sección `lakehouse-rag` dentro de `mcpServers`:

```json
{
  "mcpServers": {
    "lakehouse-rag": {
      "command": "cargo",
      "args": [
        "run",
        "--manifest-path",
        "D:\\Proyectos\\lakehouse RAG\\Cargo.toml",
        "--",
        "--mcp-server"
      ]
    }
  }
}
```

*(Si utilizas el binario compilado en modo `--release`, sustituye el `command` por la ruta directa a `lakehouse_rag.exe` y `args` por `["--mcp-server"]`)*.

---

## 3. Instalación en Claude Code (CLI)

Ejecuta directamente en tu terminal:

```powershell
claude mcp add lakehouse-rag -- cargo run --manifest-path "D:\Proyectos\lakehouse RAG\Cargo.toml" -- --mcp-server
```

Para verificar que Claude Code lo reconoce y lista las herramientas:

```powershell
claude mcp list
```

---

## 4. Instalación en Cursor / Windsurf

En la configuración del editor (`Settings` $\rightarrow$ `Features` $\rightarrow$ `MCP Servers` $\rightarrow$ `Add New MCP Server`):
- **Name:** `lakehouse-rag`
- **Type:** `command` (stdio)
- **Command:** `D:\Proyectos\lakehouse RAG\target\debug\lakehouse_rag.exe --mcp-server`

---

## 5. Herramientas Expuestas por el Servidor MCP

El servidor expone automáticamente **5 herramientas analíticas deterministas**:

| Nombre de la Herramienta | Entrada (`inputSchema`) | Propósito |
| :--- | :--- | :--- |
| **`inspect_lakehouse_catalog`** | `{ "centro_id": string, "solo_vigentes": boolean }` | Lista documentos, normas y fichas técnicas registradas en el catálogo Arrow 01. |
| **`query_operational_matrix`** | `{ "centro_id": string, "fase_id": string?, "parametro": string? }` | Consulta tolerancias numéricas y rangos de operación en la matriz operacional Arrow 03. |
| **`evaluate_double_entry_table`** | `{ "tabla_id": string, "valor_x": any, "valor_y": any }` | Resuelve celdas en tablas bidireccionales con evaluación continua de rangos y notas técnicas. |
| **`traverse_normative_graph`** | `{ "doc_id": string, "centro_id": string? }` | Consulta el grafo Petgraph para determinar si una norma está vigente, derogada o su precedencia jurídica. |
| **`execute_deterministic_sql`** | `{ "centro_id": string, "sql": string }` | Ejecuta consultas analíticas `SELECT` DataFusion con aislamiento estricto entre centros (prohíbe mutaciones). |

---

## 6. Verificación Manual por Stdio

Puedes enviar un mensaje JSON-RPC 2.0 por consola para comprobar la respuesta:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

El servidor responderá en la siguiente línea con la lista de herramientas y sus esquemas JSON correspondientes.
