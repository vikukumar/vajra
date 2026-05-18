$lines = Get-Content -Path "dist/vajra_unified.vj"
$stripped_count = 0
for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i].Trim()
    if ($line -ne "" -and -not $line.StartsWith("#")) {
        $stripped_count++
        if ($stripped_count -le 100) {
            Write-Host ("Stripped Line " + $stripped_count + " (Original " + ($i + 1) + "): " + $line)
        }
    }
}
