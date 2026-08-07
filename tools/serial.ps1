param(
  [string]$Port = "COM4",
  [int]$Baud = 115200,
  [string]$Send = "",
  [int]$ReadMs = 1500,
  [switch]$NoNewline
)

$ErrorActionPreference = "Stop"
try {
  $sp = New-Object System.IO.Ports.SerialPort $Port, $Baud, "None", 8, "One"
  $sp.ReadTimeout = 500
  $sp.WriteTimeout = 500
  $sp.DtrEnable = $true
  $sp.RtsEnable = $true
  $sp.NewLine = "`r`n"
  $sp.Open()
  Start-Sleep -Milliseconds 200

  # Drain any banner already waiting
  Start-Sleep -Milliseconds 100

  if ($Send.Length -gt 0 -or -not $NoNewline) {
    if ($NoNewline) {
      $sp.Write($Send)
    } else {
      $sp.Write($Send + "`r")
    }
  }

  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $buf = New-Object System.Text.StringBuilder
  while ($sw.ElapsedMilliseconds -lt $ReadMs) {
    try {
      $chunk = $sp.ReadExisting()
      if ($chunk.Length -gt 0) {
        [void]$buf.Append($chunk)
        $sw.Restart()  # keep reading while data flows, up to quiet gap
      } else {
        Start-Sleep -Milliseconds 40
      }
    } catch [TimeoutException] {
      Start-Sleep -Milliseconds 40
    }
  }
  Write-Output $buf.ToString()
}
finally {
  if ($sp -and $sp.IsOpen) { $sp.Close() }
  if ($sp) { $sp.Dispose() }
}
