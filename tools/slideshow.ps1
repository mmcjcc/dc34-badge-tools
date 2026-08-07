param(
  [string]$Port = "auto",
  [int]$Baud = 1000000,
  [Parameter(Mandatory=$true)][string[]]$Files,  # chunk files, in order
  [double]$Dwell = 4.0,      # seconds to hold each frame
  [int]$Loops = 1,           # how many times through the list
  [double]$Delay = 0.015,    # inter-chunk delay
  [int]$RespMs = 3000,
  [int]$Retries = 3          # per-frame retries if the badge drops off USB
)

# WEAR: every frame is a real write to the badge's PDDB (persistent store), not
# a framebuffer blit. Fine for a slideshow of a few frames; don't leave it
# running as a high-frame-rate animation for hours.
#
# The badge can drop off the USB bus between frames (suspend, or a marginal
# cable). Rather than hold one handle open for the whole run, each frame opens
# the port fresh and re-resolves the COM number, so a transient drop costs one
# frame instead of the entire slideshow.

$ErrorActionPreference = "Stop"
foreach ($f in $Files) { if (-not (Test-Path $f)) { throw "missing frame file: $f" } }

function Resolve-BadgePort([string]$requested) {
  if ($requested -ne "auto") { return $requested }
  $dev = Get-PnpDevice -Class Ports -PresentOnly -ErrorAction SilentlyContinue |
         Where-Object { $_.InstanceId -like '*VID_1D50&PID_6198*' } | Select-Object -First 1
  if (-not $dev) { return $null }
  return [regex]::Match($dev.FriendlyName, '\((COM\d+)\)').Groups[1].Value
}

function Read-Verdict($sp, $ms) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $acc = New-Object System.Text.StringBuilder
  while ($sw.ElapsedMilliseconds -lt $ms) {
    try { $c = $sp.ReadExisting() } catch { return "PORTLOST" }
    if ($c.Length -gt 0) { [void]$acc.Append($c) } else { Start-Sleep -Milliseconds 8; continue }
    foreach ($ln in ($acc.ToString() -split "`r?`n")) {
      $t = $ln.Trim()
      if ($t -eq "OK" -or $t -eq "ERR" -or $t -eq "SUCCESS") { return $t }
    }
  }
  return "TIMEOUT"
}

function Send-Frame([string]$file, [string]$requestedPort) {
  $p = Resolve-BadgePort $requestedPort
  if (-not $p) { return "NOPORT" }
  $sp = New-Object System.IO.Ports.SerialPort $p, $Baud, "None", 8, "One"
  $sp.ReadTimeout = 300; $sp.WriteTimeout = 1000
  $sp.DtrEnable = $true; $sp.RtsEnable = $true
  try {
    $sp.Open(); Start-Sleep -Milliseconds 200; $sp.DiscardInBuffer()
    foreach ($line in (Get-Content $file)) {
      if (-not $line.Trim()) { continue }
      $sp.Write($line + "`n")
      $v = Read-Verdict $sp $RespMs
      if ($v -ne "OK" -and $v -ne "SUCCESS") { return $v }
      if ($Delay -gt 0) { Start-Sleep -Milliseconds ([int]($Delay * 1000)) }
    }
    return "SUCCESS"
  }
  catch { return "PORTLOST" }
  finally { if ($sp.IsOpen) { $sp.Close() }; $sp.Dispose() }
}

Write-Output ("slideshow: {0} frame(s) x {1} loop(s), {2}s dwell" -f $Files.Count, $Loops, $Dwell)

for ($loop = 1; $loop -le $Loops; $loop++) {
  foreach ($file in $Files) {
    $name = [System.IO.Path]::GetFileNameWithoutExtension($file)
    $ok = $false
    for ($try = 1; $try -le $Retries; $try++) {
      $r = Send-Frame $file $Port
      if ($r -eq "SUCCESS") { $ok = $true; break }
      Write-Output ("  [{0}] '{1}' attempt {2}/{3} -> {4}" -f $loop, $name, $try, $Retries, $r)
      Start-Sleep -Seconds 2
    }
    if ($ok) { Write-Output ("  [{0}] showing '{1}'" -f $loop, $name) }
    else     { Write-Output ("  [{0}] SKIPPED '{1}'" -f $loop, $name) }
    Start-Sleep -Milliseconds ([int]($Dwell * 1000))
  }
}
Write-Output "=== slideshow complete ==="
