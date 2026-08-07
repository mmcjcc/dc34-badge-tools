param(
  [string]$Port = "auto",
  [int]$Baud = 1000000,
  [Parameter(Mandatory=$true)][string]$BrightFile,
  [Parameter(Mandatory=$true)][string]$DarkFile,
  [string]$Message = "SOS",
  [double]$Unit = 0.45,      # seconds per morse unit (dot = 1, dash = 3)
  [int]$Loops = 1,
  [double]$Delay = 0.015,
  [int]$RespMs = 3000
)

# The OLED is the only light source we can drive from the console: the LED
# verbs (hue/autogamy/transmute/mate/rate) are NOT compiled into shipped
# firmware -- they fall through to the generic `test` help line, whereas a real
# but under-specified command (`test bootwait`) returns its own usage string.
#
# WEAR: every frame is a genuine write to the badge's PDDB, not a framebuffer
# blit. One SOS cycle is 18 writes. Keep -Loops small; this is a party trick,
# not something to leave running for hours.

$ErrorActionPreference = "Stop"

$MORSE = @{
  'A'='.-';    'B'='-...';  'C'='-.-.';  'D'='-..';   'E'='.';     'F'='..-.'
  'G'='--.';   'H'='....';  'I'='..';    'J'='.---';  'K'='-.-';   'L'='.-..'
  'M'='--';    'N'='-.';    'O'='---';   'P'='.--.';  'Q'='--.-';  'R'='.-.'
  'S'='...';   'T'='-';     'U'='..-';   'V'='...-';  'W'='.--';   'X'='-..-'
  'Y'='-.--';  'Z'='--..';  '0'='-----'; '1'='.----'; '2'='..---'; '3'='...--'
  '4'='....-'; '5'='.....'; '6'='-....'; '7'='--...'; '8'='---..'; '9'='----.'
}

if ($Port -eq "auto") {
  $dev = Get-PnpDevice -Class Ports -PresentOnly -ErrorAction SilentlyContinue |
         Where-Object { $_.InstanceId -like '*VID_1D50&PID_6198*' } | Select-Object -First 1
  if (-not $dev) { throw "Badge not found - is it plugged in?" }
  $Port = [regex]::Match($dev.FriendlyName, '\((COM\d+)\)').Groups[1].Value
  Write-Output "auto-detected badge on $Port"
}

$bright = Get-Content $BrightFile
$dark   = Get-Content $DarkFile

$sp = New-Object System.IO.Ports.SerialPort $Port, $Baud, "None", 8, "One"
$sp.ReadTimeout = 300; $sp.WriteTimeout = 1000
$sp.DtrEnable = $true; $sp.RtsEnable = $true
$sp.Open(); Start-Sleep -Milliseconds 250; $sp.DiscardInBuffer()

function Read-Verdict($sp, $ms) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $acc = New-Object System.Text.StringBuilder
  while ($sw.ElapsedMilliseconds -lt $ms) {
    try { $c = $sp.ReadExisting() } catch { $c = "" }
    if ($c.Length -gt 0) { [void]$acc.Append($c) } else { Start-Sleep -Milliseconds 8; continue }
    foreach ($ln in ($acc.ToString() -split "`r?`n")) {
      $t = $ln.Trim()
      if ($t -eq "OK" -or $t -eq "ERR" -or $t -eq "SUCCESS") { return $t }
    }
  }
  return "TIMEOUT"
}

function Show-Frame($sp, $lines) {
  foreach ($line in $lines) {
    if (-not $line.Trim()) { continue }
    $sp.Write($line + "`n")
    $v = Read-Verdict $sp $RespMs
    if ($v -ne "OK" -and $v -ne "SUCCESS") { return $false }
    if ($Delay -gt 0) { Start-Sleep -Milliseconds ([int]($Delay * 1000)) }
  }
  return $true
}

try {
  $code = ($Message.ToUpper().ToCharArray() | ForEach-Object {
    if ($MORSE.ContainsKey([string]$_)) { $MORSE[[string]$_] } else { $null }
  }) -join ' '
  Write-Output "message '$Message' -> $code"

  for ($loop = 1; $loop -le $Loops; $loop++) {
    foreach ($sym in $code.ToCharArray()) {
      if ($sym -eq ' ') { Start-Sleep -Milliseconds ([int]($Unit * 3000)); continue }
      $hold = if ($sym -eq '-') { $Unit * 3 } else { $Unit }
      [void](Show-Frame $sp $bright)
      Start-Sleep -Milliseconds ([int]($hold * 1000))
      [void](Show-Frame $sp $dark)
      Start-Sleep -Milliseconds ([int]($Unit * 1000))
    }
    Write-Output "  loop $loop/$Loops done"
  }
  Write-Output "=== morse complete ==="
}
finally {
  if ($sp.IsOpen) { $sp.Close() }
  $sp.Dispose()
}
