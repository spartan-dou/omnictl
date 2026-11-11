# Genere assets\omnictl.ico : un carre arrondi en degrade, portant une touche de clavier
# eclairee. Dessine par programme (System.Drawing), en plusieurs tailles, puis assemble en
# .ico (entrees PNG). A relancer seulement si l'on veut changer l'icone.
Add-Type -AssemblyName System.Drawing

function New-RoundedPath([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    $p
}

function New-IconPng([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = $size / 256.0

    # Fond : carre arrondi, degrade bleu -> violet.
    $back = New-RoundedPath (12 * $s) (12 * $s) (232 * $s) (232 * $s) (54 * $s)
    $rect = New-Object System.Drawing.RectangleF(0, 0, $size, $size)
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect,
        [System.Drawing.Color]::FromArgb(255, 10, 132, 255), [System.Drawing.Color]::FromArgb(255, 124, 58, 237), 55.0)
    $g.FillPath($brush, $back)

    # Halo de lumiere sous la touche, puis la touche elle-meme.
    $glow = New-RoundedPath (58 * $s) (66 * $s) (140 * $s) (140 * $s) (34 * $s)
    $g.FillPath((New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(90, 255, 255, 255))), $glow)
    $cap = New-RoundedPath (70 * $s) (70 * $s) (116 * $s) (116 * $s) (26 * $s)
    $g.FillPath((New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))), $cap)

    # Anneau au centre de la touche : le « o » d'omnictl.
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 70, 90, 235), [single](17 * $s))
    $g.DrawEllipse($pen, [single](103 * $s), [single](103 * $s), [single](50 * $s), [single](50 * $s))

    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    , $ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 256
$images = $sizes | ForEach-Object { , (New-IconPng $_) }

$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($out)
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }   # 0 signifie 256
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $w.Write($img) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $PSScriptRoot 'omnictl.ico'), $out.ToArray())
"omnictl.ico : $($out.Length) octets, tailles $($sizes -join ', ')"
