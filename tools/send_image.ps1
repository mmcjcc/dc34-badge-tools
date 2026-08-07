param(
  [string]$Port = "COM4",
  [int]$Baud = 1000000,
  [Parameter(Mandatory=$true)][string]$File,
  [int]$Max = 0,             # 0 = all
  [double]$Delay = 0.05,
  [int]$RespMs = 3000
)

$ErrorActionPreference = "Stop"
$lines = Get-Content $File
if ($Max -gt 0 -and $lines.Count -gt $Max) { $lines = $lines[0..($Max-1)] }

$sp = New-Object System.IO.Ports.SerialPort $Port, $Baud, "None", 8, "One"
$sp.ReadTimeout  = 300
$sp.WriteTimeout = 1000
$sp.DtrEnable = $true
$sp.RtsEnable = $true
$sp.Open()
Start-Sleep -Milliseconds 250
$sp.DiscardInBuffer()

function Read-Response($sp, $ms) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $acc = New-Object System.Text.StringBuilder
  while ($sw.ElapsedMilliseconds -lt $ms) {
    try { $c = $sp.ReadExisting() } catch { $c = "" }
    if ($c.Length -gt 0) { [void]$acc.Append($c) } else { Start-Sleep -Milliseconds 10; continue }
    $text = $acc.ToString()
    foreach ($ln in ($text -split "`r?`n")) {
      $t = $ln.Trim()
      if ($t -eq "OK" -or $t -eq "ERR" -or $t -eq "SUCCESS" -or $t -eq "CLEAR") {
        return @{ verdict = $t; raw = $text }
      }
    }
  }
  return @{ verdict = "TIMEOUT"; raw = $acc.ToString() }
}

$i = 0
$final = ""
try {
  foreach ($line in $lines) {
    if (-not $line.Trim()) { continue }
    $sp.Write($line + "`n")
    $r = Read-Response $sp $RespMs
    $v = $r.verdict
    if ($v -eq "OK") {
      Write-Output ("chunk {0,2}/{1} -> OK" -f ($i+1), $lines.Count)
    } elseif ($v -eq "SUCCESS") {
      Write-Output ("chunk {0,2}/{1} -> SUCCESS (transfer complete)" -f ($i+1), $lines.Count)
      $final = "SUCCESS"
      break
    } else {
      Write-Output ("chunk {0,2}/{1} -> {2}" -f ($i+1), $lines.Count, $v)
      Write-Output "RAW: $($r.raw)"
      $final = $v
      break
    }
    $i++
    if ($Delay -gt 0) { Start-Sleep -Milliseconds ([int]($Delay*1000)) }
  }
  Write-Output "=== sent $i chunk(s); final=$final ==="
}
finally {
  if ($sp.IsOpen) { $sp.Close() }
  $sp.Dispose()
}
