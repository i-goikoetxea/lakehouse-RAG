# Script de ayuda para configurar e instalar el MCP de Lakehouse RAG en Claude Desktop / Claude Code
# Uso: powershell -ExecutionPolicy Bypass -File .\install_mcp.ps1

Write-Host "===========================================================" -ForegroundColor Cyan
Write-Host "  Instalador / Asistente MCP — Lakehouse RAG Industrial    " -ForegroundColor Cyan
Write-Host "===========================================================" -ForegroundColor Cyan

$projectDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$cargoToml = Join-Path $projectDir "Cargo.toml"

Write-Host "`n[1] Directorio del proyecto:" -ForegroundColor Yellow
Write-Host "    $projectDir"

Write-Host "`n[2] Comando para Claude Code CLI:" -ForegroundColor Yellow
$claudeCmd = "claude mcp add lakehouse-rag -- cargo run --manifest-path `"$cargoToml`" -- --mcp-server"
Write-Host "    $claudeCmd" -ForegroundColor Green

Write-Host "`n[3] Configuración para Claude Desktop (claude_desktop_config.json):" -ForegroundColor Yellow
$jsonSnippet = @"
{
  "mcpServers": {
    "lakehouse-rag": {
      "command": "cargo",
      "args": [
        "run",
        "--manifest-path",
        "$($cargoToml.Replace('\', '\\'))",
        "--",
        "--mcp-server"
      ]
    }
  }
}
"@
Write-Host $jsonSnippet -ForegroundColor Gray

Write-Host "`n[4] Comprobando disponibilidad de claude CLI..." -ForegroundColor Yellow
if (Get-Command claude -ErrorAction SilentlyContinue) {
    Write-Host "    Claude CLI detectado. ¿Deseas registrar automáticamente el MCP? (S/N): " -NoNewline -ForegroundColor White
    $resp = Read-Host
    if ($resp -match "^[sSyY]") {
        Invoke-Expression $claudeCmd
        Write-Host "    ¡Servidor MCP registrado en Claude Code con éxito!" -ForegroundColor Green
    }
} else {
    Write-Host "    Claude CLI no se encuentra en el PATH actual. Copia la configuración [3] en tu cliente." -ForegroundColor DarkGray
}

Write-Host "`nPara más detalles técnicos, consulta: MCP_GUIDE.md" -ForegroundColor Cyan
