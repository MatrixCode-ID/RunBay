# Renders the RunBay logo (same geometry as runbay.svg) into runbay.ico with PNG frames.
# Run with Windows PowerShell or pwsh on Windows: ./make-ico.ps1
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

function New-RoundedRect([float] $x, [float] $y, [float] $w, [float] $h, [float] $r) {
    $p = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    $p
}

function New-Frame([int] $size) {
    $bmp = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.ScaleTransform($size / 256.0, $size / 256.0)
    $g.FillPath([System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(0x67, 0x50, 0xA4)), (New-RoundedRect 8 8 240 240 72))
    $play = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $play.AddBezier(100, 70, 128, 87, 156, 105, 184, 122)
    $play.AddBezier(184, 122, 192, 127, 192, 129, 184, 134)
    $play.AddBezier(184, 134, 156, 151, 128, 169, 100, 186)
    $play.AddBezier(100, 186, 92, 191, 88, 187, 88, 179)
    $play.AddLine(88, 179, 88, 77)
    $play.AddBezier(88, 77, 88, 69, 92, 65, 100, 70)
    $play.CloseFigure()
    $g.FillPath([System.Drawing.Brushes]::White, $play)
    $g.FillPath([System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(0xEA, 0xDD, 0xFF)), (New-RoundedRect 64 204 128 14 7))
    $g.Dispose()
    $ms = [System.IO.MemoryStream]::new()
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    , $ms.ToArray()
}

$sizes = 16, 20, 24, 32, 40, 48, 64, 256
$frames = $sizes | ForEach-Object { , (New-Frame $_) }
$out = [System.IO.MemoryStream]::new()
$w = [System.IO.BinaryWriter]::new($out)
$w.Write([uint16] 0); $w.Write([uint16] 1); $w.Write([uint16] $sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]
    $dim = if ($s -ge 256) { 0 } else { $s }
    $w.Write([byte] $dim); $w.Write([byte] $dim); $w.Write([byte] 0); $w.Write([byte] 0)
    $w.Write([uint16] 1); $w.Write([uint16] 32)
    $w.Write([uint32] $frames[$i].Length); $w.Write([uint32] $offset)
    $offset += $frames[$i].Length
}
foreach ($f in $frames) { $w.Write($f) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $PSScriptRoot 'runbay.ico'), $out.ToArray())
# A 64 px PNG for the window and tray icon.
[System.IO.File]::WriteAllBytes((Join-Path $PSScriptRoot 'runbay-64.png'), (New-Frame 64))
Write-Host 'runbay.ico written'
