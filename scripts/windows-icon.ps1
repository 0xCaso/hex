# Regenerates resources/windows/hex.ico, the executable and shortcut icon,
# from the macOS icon's glyph on the same dark vertical gradient.
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$glyph = [Drawing.Image]::FromFile((Join-Path $root "app\AppIcon.icon\Assets\Image.png"))
$sizes = 256, 64, 48, 40, 32, 24, 20, 16
$pngs = foreach ($size in $sizes) {
    $bitmap = New-Object Drawing.Bitmap $size, $size
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = "AntiAlias"
    $graphics.InterpolationMode = "HighQualityBicubic"
    $graphics.PixelOffsetMode = "HighQuality"
    $inset = [Math]::Max(1, [Math]::Round($size / 32))
    $tile = $size - 2 * $inset
    $radius = $tile * 0.22
    $path = New-Object Drawing.Drawing2D.GraphicsPath
    $diameter = 2 * $radius
    $path.AddArc($inset, $inset, $diameter, $diameter, 180, 90)
    $path.AddArc($inset + $tile - $diameter, $inset, $diameter, $diameter, 270, 90)
    $path.AddArc($inset + $tile - $diameter, $inset + $tile - $diameter, $diameter, $diameter, 0, 90)
    $path.AddArc($inset, $inset + $tile - $diameter, $diameter, $diameter, 90, 90)
    $path.CloseFigure()
    # Gray 0.2 at the top fading to black at 70% of the tile, then black,
    # as the macOS fill does.
    $graphics.SetClip($path)
    for ($y = 0; $y -lt $size; $y++) {
        $t = [Math]::Min(1, [Math]::Max(0, ($y - $inset) / ($tile * 0.7)))
        $level = [int][Math]::Round(51 * (1 - $t))
        $row = New-Object Drawing.SolidBrush ([Drawing.Color]::FromArgb(255, $level, $level, $level))
        $graphics.FillRectangle($row, 0, $y, $size, 1)
        $row.Dispose()
    }
    $graphics.ResetClip()
    # The glyph fills half the tile, as in the macOS icon composition.
    $height = $tile * 0.5 * $glyph.Height / [Math]::Max($glyph.Width, $glyph.Height) * 1.12
    $width = $height * $glyph.Width / $glyph.Height
    $graphics.DrawImage($glyph, ($size - $width) / 2, ($size - $height) / 2, $width, $height)
    $graphics.Dispose()
    $stream = New-Object IO.MemoryStream
    if ($size -ge 256) {
        # Vista and later read the large frame as PNG.
        $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
    } else {
        # Small frames stay classic 32-bit DIBs: header, bottom-up BGRA rows,
        # then an all-zero AND mask padded to 32-bit rows.
        $dib = New-Object IO.BinaryWriter $stream
        $dib.Write([UInt32]40); $dib.Write([Int32]$size); $dib.Write([Int32]($size * 2))
        $dib.Write([UInt16]1); $dib.Write([UInt16]32); $dib.Write([UInt32]0)
        $dib.Write([UInt32]($size * $size * 4)); $dib.Write([Int32]0); $dib.Write([Int32]0)
        $dib.Write([UInt32]0); $dib.Write([UInt32]0)
        for ($y = $size - 1; $y -ge 0; $y--) {
            for ($x = 0; $x -lt $size; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                $dib.Write([byte]$pixel.B); $dib.Write([byte]$pixel.G)
                $dib.Write([byte]$pixel.R); $dib.Write([byte]$pixel.A)
            }
        }
        $dib.Write((New-Object byte[] ([Math]::Ceiling($size / 32) * 4 * $size)))
        $dib.Flush()
    }
    $bitmap.Dispose()
    , $stream.ToArray()
}
$glyph.Dispose()

# ICO container: header, one directory entry per image, then PNG payloads.
$out = New-Object IO.MemoryStream
$writer = New-Object IO.BinaryWriter $out
$writer.Write([UInt16]0); $writer.Write([UInt16]1); $writer.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dimension = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
    $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([UInt16]1); $writer.Write([UInt16]32)
    $writer.Write([UInt32]$pngs[$i].Length); $writer.Write([UInt32]$offset)
    $offset += $pngs[$i].Length
}
foreach ($png in $pngs) { $writer.Write($png) }
$writer.Flush()
[IO.File]::WriteAllBytes((Join-Path $root "resources\windows\hex.ico"), $out.ToArray())
"wrote resources\windows\hex.ico ($($out.Length) bytes)"
