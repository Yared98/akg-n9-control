param([string]$Name = "capture", [string]$Req = "06 07 80 05 5A 03 00 D6 0C 00", [string]$Sel = 'HID\VID_0ECB&PID_20D9&MI_03&COL02')
# Runs probe.ps1 (one read-only battery query + 120 GetInputReport reads) and summarizes the RACE frames.
$dir = $PSScriptRoot
pwsh -NoProfile -File "$dir\probe.ps1" -Log "$dir\$Name.log" -Req $Req -Sel $Sel | Out-Null
$lines = Get-Content "$dir\$Name.log" | Where-Object { $_ -match 'GetInputReport 07: ' }
"reads: $($lines.Count)"
$first = ($lines | Select-Object -First 1); "first buffer: " + ($first -split ': ')[1]
$frames = foreach ($l in $lines) {
  $b = (($l -split ': ')[1] -split ' ') | ForEach-Object { [Convert]::ToByte($_,16) }
  for ($i = 3; $i -lt $b.Count - 5; $i++) {
    if ($b[$i] -eq 5 -and $b[$i+1] -in 0x5B,0x5C,0x5D) {
      $len = $b[$i+2] + 256*$b[$i+3]; if ($len -lt 2 -or $i+4+$len -gt $b.Count) { continue }
      $id = $b[$i+4] + 256*$b[$i+5]
      '{0:X2} id={1:X4} payload={2}' -f $b[$i+1], $id, (($b[($i+6)..($i+3+$len)] | ForEach-Object { $_.ToString('X2') }) -join ' ')
      $i += 3 + $len
    }
  }
}
$frames | Group-Object | Sort-Object Name | Format-Table Count, Name -AutoSize
