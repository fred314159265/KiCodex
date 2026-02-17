Get-CimInstance Win32_Process | Where-Object { $_.Name -like 'kicad*' } | Select-Object ProcessId, Name, CommandLine | Format-List
