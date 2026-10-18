# Verificación de consola del binario release (subsistema Windows + AttachConsole).
# Se ejecuta en una consola interactiva SIN redirección: la salida debe verse aquí.
$env:LAKEHOUSE_RAG_CONFIG_DIR = 'D:\lh_fase14\config'
$env:FASTEMBED_CACHE_DIR = 'D:\Proyectos\lakehouse RAG\target\fastembed_cache'
$exe = 'D:\Proyectos\lakehouse RAG\target\release\lakehouse_rag.exe'
Write-Host '=== --centros ==='
& $exe --centros
Write-Host "codigo=$LASTEXITCODE"
Write-Host '=== --ask ==='
& $exe --ask 'En la Tabla 4-A, que temperatura corresponde a e = 30 mm y CE = 0,42?' --centro obra-planta-solar-01 --modelo haiku
Write-Host "codigo=$LASTEXITCODE"
Write-Host '=== FIN ==='
