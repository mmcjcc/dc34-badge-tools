param(
  [string]$Port = "auto",
  [int]$Baud = 1000000,
  [int]$Seconds = 12,
  [string]$Out = "capture.log"
)
# Passive listener: sends NOTHING, just records what the badge volunteers.
# dc34-vault logs its LED "light gene" structs unprompted, so this is a
# read-only way to read the genome off the wire.
$ErrorActionPreference = "Stop"
if ($Port -eq "auto") {
  $dev = Get-PnpDevice -Class Ports -PresentOnly -ErrorAction SilentlyContinue |
         Where-Object { $_.InstanceId -like '*VID_1D50&PID_6198*' } | Select-Object -First 1
  if (-not $dev) { throw "badge not found" }
  $Port = [regex]::Match($dev.FriendlyName,'\((COM\d+)\)').Groups[1].Value
}
$sp = New-Object System.IO.Ports.SerialPort $Port, $Baud, "None", 8, "One"
$sp.ReadTimeout = 200; $sp.DtrEnable = $true; $sp.RtsEnable = $true
$sp.Open()
$acc = New-Object System.Text.StringBuilder
$deadline = (Get-Date).AddSeconds($Seconds)   # hard stop; the log never idles
try {
  while ((Get-Date) -lt $deadline) {
    try { $c = $sp.ReadExisting() } catch { break }
    if ($c.Length -gt 0) { [void]$acc.Append($c) } else { Start-Sleep -Milliseconds 20 }
  }
}
finally { if ($sp.IsOpen) { $sp.Close() }; $sp.Dispose() }
$text = $acc.ToString()
$text | Out-File -FilePath $Out -Encoding utf8
Write-Output "captured $($text.Length) chars from $Port -> $Out"
