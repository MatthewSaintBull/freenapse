# Draws the freenapse icon (green open ring on a dark tile, same drawing as assets/logo.svg) and writes assets/freenapse.ico
# with 16/24/32/48 px BMP entries and a 256 px PNG entry. Run: powershell -File assets/make-icon.ps1
Add-Type -AssemblyName System.Drawing
$out = Join-Path $PSScriptRoot "freenapse.ico"

function Draw([int]$n) {
    $bmp = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = $n / 64.0
    # Tile
    $r = [Math]::Max(2, 12 * $s)
    $tile = New-Object System.Drawing.Drawing2D.GraphicsPath
    $tile.AddArc(0, 0, 2*$r, 2*$r, 180, 90); $tile.AddArc($n-2*$r, 0, 2*$r, 2*$r, 270, 90)
    $tile.AddArc($n-2*$r, $n-2*$r, 2*$r, 2*$r, 0, 90); $tile.AddArc(0, $n-2*$r, 2*$r, 2*$r, 90, 90); $tile.CloseFigure()
    $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 24, 24, 24))), $tile)
    # Open ring with a dot leaving it (same drawing as assets/logo.svg): centre (32,32), r 17, gap at the top right
    $mint = [System.Drawing.Color]::FromArgb(255, 29, 185, 84)
    $light = [System.Drawing.Color]::FromArgb(255, 63, 214, 118)
    $pen = New-Object System.Drawing.Pen $mint, ([Math]::Max(1.5, 5 * $s))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round; $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawArc($pen, 15 * $s, 15 * $s, 34 * $s, 34 * $s, -57, -336)
    $g.FillEllipse((New-Object System.Drawing.SolidBrush $light), 45.5 * $s, 9.5 * $s, 9 * $s, 9 * $s)
    $g.Dispose()
    return $bmp
}

function BmpEntry([System.Drawing.Bitmap]$bmp) {
    $n = $bmp.Width
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter $ms
    $bw.Write([int32]40); $bw.Write([int32]$n); $bw.Write([int32]($n*2)); $bw.Write([int16]1); $bw.Write([int16]32)
    $bw.Write([int32]0); $bw.Write([int32]($n*$n*4)); $bw.Write([int32]0); $bw.Write([int32]0); $bw.Write([int32]0); $bw.Write([int32]0)
    for ($yy = $n-1; $yy -ge 0; $yy--) { for ($xx = 0; $xx -lt $n; $xx++) { $c = $bmp.GetPixel($xx, $yy); $bw.Write([byte]$c.B); $bw.Write([byte]$c.G); $bw.Write([byte]$c.R); $bw.Write([byte]$c.A) } }
    $maskRow = [int][Math]::Ceiling($n / 32.0) * 4
    $bw.Write((New-Object byte[] ($maskRow * $n)))
    $bw.Flush(); return ,$ms.ToArray()
}

function PngEntry([System.Drawing.Bitmap]$bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    return ,$ms.ToArray()
}

$entries = New-Object System.Collections.ArrayList
foreach ($n in 16, 24, 32, 48) { $b = Draw $n; [void]$entries.Add(@{ n = $n; data = [byte[]](BmpEntry $b) }); $b.Dispose() }
$b = Draw 256; [void]$entries.Add(@{ n = 256; data = [byte[]](PngEntry $b) }); $b.Save((Join-Path $PSScriptRoot "freenapse-256.png")); $b.Dispose()

$fs = [System.IO.File]::Create($out)
$w = New-Object System.IO.BinaryWriter $fs
$w.Write([int16]0); $w.Write([int16]1); $w.Write([int16]$entries.Count)
$offset = 6 + 16 * $entries.Count
foreach ($e in $entries) {
    $n = $e.n; $data = $e.data
    $w.Write([byte]($(if ($n -ge 256) { 0 } else { $n }))); $w.Write([byte]($(if ($n -ge 256) { 0 } else { $n })))
    $w.Write([byte]0); $w.Write([byte]0); $w.Write([int16]1); $w.Write([int16]32); $w.Write([int32]$data.Length); $w.Write([int32]$offset)
    $offset += $data.Length
}
foreach ($e in $entries) { $w.Write([byte[]]$e.data) }
$w.Flush(); $fs.Close()
"wrote $out ($offset bytes)"
