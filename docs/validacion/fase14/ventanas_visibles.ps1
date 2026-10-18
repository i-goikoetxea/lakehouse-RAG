# Lista los procesos con ventana visible relacionados con la app (consolas incluidas)
Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and $_.ProcessName -match 'lakehouse|claude|conhost|cmd|OpenConsole|WindowsTerminal|node' } |
  Select-Object ProcessName, Id, MainWindowTitle | Format-Table -AutoSize | Out-String -Width 200
Write-Output ("Procesos claude vivos: " + @(Get-Process -Name claude -ErrorAction SilentlyContinue).Count)
