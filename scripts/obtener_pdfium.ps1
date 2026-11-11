# Descarga la versión fijada de pdfium (proyecto bblanchon/pdfium-binaries) y comprueba su sha256.
# La DLL queda en vendor/pdfium/pdfium.dll, ignorada por git; build.rs la copia junto a los
# ejecutables al compilar. Contrato: docs/arquitectura/ssd_generalidad_4_piezas.md §1.2.
#
# Para actualizar pdfium (parches de seguridad de Chromium): cambia la versión y los dos sha256
# aquí y en src/infrastructure/parsers/pdfium_extractor.rs (VERSION_PDFIUM y SHA256_PDFIUM), y
# comprueba que la feature pdfium_XXXX de pdfium-render en Cargo.toml es compatible.

$ErrorActionPreference = 'Stop'

$Version = 'chromium/8125'
$Url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$Version/pdfium-win-x64.tgz"
$Sha256Paquete = '9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b'
$Sha256Dll = '4b0dec92555d69437bef0df37be630181ab3e83a6823ca264ede355a4f2ef994'

$Raiz = Split-Path -Parent $PSScriptRoot
$Destino = Join-Path $Raiz 'vendor\pdfium'
$Dll = Join-Path $Destino 'pdfium.dll'

if ((Test-Path $Dll) -and ((Get-FileHash $Dll -Algorithm SHA256).Hash.ToLower() -eq $Sha256Dll)) {
    Write-Host "pdfium $Version ya está en $Dll"
    exit 0
}

$Temporal = Join-Path ([System.IO.Path]::GetTempPath()) ("pdfium-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $Temporal | Out-Null
try {
    $Paquete = Join-Path $Temporal 'pdfium-win-x64.tgz'
    Write-Host "Descargando $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Paquete -UseBasicParsing
    $Huella = (Get-FileHash $Paquete -Algorithm SHA256).Hash.ToLower()
    if ($Huella -ne $Sha256Paquete) {
        throw "El paquete descargado no es el fijado (sha256 $Huella). No se instala."
    }
    $Extraido = Join-Path $Temporal 'x'
    New-Item -ItemType Directory -Force $Extraido | Out-Null
    tar -xzf $Paquete -C $Extraido
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo descomprimir el paquete.' }
    $DllExtraida = Join-Path $Extraido 'bin\pdfium.dll'
    $HuellaDll = (Get-FileHash $DllExtraida -Algorithm SHA256).Hash.ToLower()
    if ($HuellaDll -ne $Sha256Dll) {
        throw "La DLL del paquete no es la fijada (sha256 $HuellaDll). No se instala."
    }
    New-Item -ItemType Directory -Force $Destino | Out-Null
    Copy-Item $DllExtraida $Dll -Force
    Copy-Item (Join-Path $Extraido 'LICENSE') (Join-Path $Destino 'LICENSE') -Force
    Write-Host "pdfium $Version instalado en $Dll. Vuelve a compilar para copiarlo junto a los ejecutables."
}
finally {
    Remove-Item -Recurse -Force $Temporal -ErrorAction SilentlyContinue
}
