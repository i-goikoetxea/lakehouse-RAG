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

El servidor expone las **6 herramientas v2** (las mismas que usa el agente de la GUI y de la CLI). Todas filtran por el centro activo y aceptan `fecha_referencia` opcional (ISO `AAAA-MM-DD`; sin ella → lo vigente). Los esquemas son estrictos (`additionalProperties: false`).

| Herramienta | Entrada (`inputSchema`) | Propósito |
| :--- | :--- | :--- |
| **`inspect_lakehouse_catalog`** | `centro_id`, `fecha_referencia?` | Documentos (tipo, prelación, revisión y vigencia) e inventario de tablas de doble entrada (ejes, unidades, instrucciones). |
| **`query_operational_matrix`** | `centro_id`, `fase?`, `rol?`, `rol_en?` (`ejecutor`/`responsable`/`cualquiera`), `registro?`, `doc_id?`, `objeto?`, `fecha_referencia?` | Filas atómicas de la matriz con filtros exactos normalizados y cita (doc_id, revisión, breadcrumb). |
| **`lookup_double_entry_table`** | `centro_id`, `tabla_id?` o `busqueda_tabla?`, `valor_x`, `valor_y` (número o categoría), `eje_z?`, `fecha_referencia?` | Cruce exacto con la inclusividad de cada extremo; devuelve la nota al pie obligatoria o informa de ambigüedad / sin coincidencia (no interpola). |
| **`execute_athena_sql`** | `centro_id`, `sql_query`, `fecha_referencia?` | Una sola sentencia `SELECT`/`WITH` validada con el parser de DataFusion, solo sobre las vistas del centro, con límite de filas (1000) y timeout. |
| **`traverse_normative_graph`** | `centro_id`, `nodo_origen`, `tipo_relacion?`, `max_saltos?` (≤ 2), `max_vecinos?` (15), `fecha_referencia?` | Recorrido acotado (≤ 50 nodos) con `hub_resumido` para supernodos y orden por prelación. |
| **`search_normative_context`** | `centro_id`, `consulta`, `top_k?`, `nivel_prelacion_max?`, `doc_id?`, `fecha_referencia?` | Búsqueda híbrida con breadcrumb, condición heredada y agrupación por tema con `prevalece`/`supletorio`. |

---

## 6. Verificación Manual por Stdio

Puedes enviar un mensaje JSON-RPC 2.0 por consola para comprobar la respuesta:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

El servidor responderá en la siguiente línea con la lista de herramientas y sus esquemas JSON correspondientes.
