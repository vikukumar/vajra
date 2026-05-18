$lines = Get-Content -Path "dist/vajra_unified.vj"
for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line.Contains("{")) {
        $col = $line.IndexOf("{") + 1
        Write-Host ("Line " + ($i + 1) + ", Col " + $col + ": " + $line)
    }
}
