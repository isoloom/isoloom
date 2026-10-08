# The project into a Windows machine: unpacks the tar.gz $archive into $dest (replacing an older
# copy), leaving out the archive's first $strip folders. Plain .NET (GZipStream), so it runs on
# every Windows Isoloom supports: PowerShell 2.0 and later, and no tar.exe before Windows Server
# 2019. Symbolic links are left out (counted).
& {
  $ErrorActionPreference = 'Stop'
  $sep = [IO.Path]::DirectorySeparatorChar
  $new = "$dest.new"
  if (Test-Path $new) { Remove-Item $new -Recurse -Force }
  [void][IO.Directory]::CreateDirectory($new)
  $file = New-Object IO.FileStream($archive, [IO.FileMode]::Open, [IO.FileAccess]::Read)
  $in = New-Object IO.Compression.GZipStream($file, [IO.Compression.CompressionMode]::Decompress)
  $utf8 = New-Object Text.UTF8Encoding($false)
  $header = New-Object byte[] 512
  $chunk = New-Object byte[] 65536
  function Fill([byte[]]$b, [int]$n) {
    $got = 0
    while ($got -lt $n) {
      $r = $in.Read($b, $got, $n - $got)
      if ($r -le 0) { throw "$archive ends early" }
      $got += $r
    }
  }
  function Field([int]$at, [int]$len) {
    $end = [Array]::IndexOf($header, [byte]0, $at, $len)
    if ($end -lt 0) { $end = $at + $len }
    $utf8.GetString($header, $at, $end - $at)
  }
  # Copies $size bytes to $out (or skips them, with no $out), then the padding after them.
  function Body([long]$size, $out) {
    $left = $size
    while ($left -gt 0) {
      $n = [int][Math]::Min([long]$chunk.Length, $left)
      Fill $chunk $n
      if ($out) { $out.Write($chunk, 0, $n) }
      $left -= $n
    }
    $pad = [int]((512 - $size % 512) % 512)
    if ($pad) { Fill $chunk $pad }
  }
  $pax = @{}
  $links = 0
  $files = 0
  while ($true) {
    Fill $header 512
    if ($header[148] -eq 0) { break } # the end: zero blocks have no checksum
    $type = [char]$header[156]
    $sizeText = (Field 124 12).Trim()
    $size = [long]0
    if ($sizeText) { $size = [Convert]::ToInt64($sizeText, 8) }
    if ($pax['size']) { $size = [long]$pax['size'] }
    if ($type -eq 'x') {
      # pax records ("<length> key=value\n"): a long path, a large size, for the next entry.
      $ms = New-Object IO.MemoryStream
      Body $size $ms
      $b = $ms.ToArray()
      $i = 0
      while ($i -lt $b.Length) {
        $sp = [Array]::IndexOf($b, [byte]32, $i)
        $len = [int]$utf8.GetString($b, $i, $sp - $i)
        $eq = [Array]::IndexOf($b, [byte]61, $sp)
        $pax[$utf8.GetString($b, $sp + 1, $eq - $sp - 1)] = $utf8.GetString($b, $eq + 1, $i + $len - $eq - 2)
        $i += $len
      }
      continue
    }
    if ($type -eq 'L' -or $type -eq 'K') {
      # GNU tar's long name (or link target) for the next entry.
      $ms = New-Object IO.MemoryStream
      Body $size $ms
      if ($type -eq 'L') { $pax['path'] = $utf8.GetString($ms.ToArray()).TrimEnd([char]0) }
      continue
    }
    $name = Field 0 100
    $prefix = Field 345 155
    if ($prefix) { $name = "$prefix/$name" }
    if ($pax['path']) { $name = $pax['path'] }
    $pax = @{}
    $parts = @($name.Split('/') | Where-Object { $_ -ne '' -and $_ -ne '.' })
    if ($parts.Count -le $strip) { Body $size $null; continue }
    $path = Join-Path $new ([string]::Join([string]$sep, [string[]]($parts[$strip..($parts.Count - 1)])))
    if ($type -eq '5' -or $type -eq '0' -or $type -eq [char]0) {
      # A name Windows can't take (`:`, `*`, too long...) is left out, with a warning.
      $out = $null
      try {
        if ($type -eq '5') {
          [void][IO.Directory]::CreateDirectory($path)
        } else {
          [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
          $out = [IO.File]::Create($path)
        }
      } catch {
        Write-Warning "Not copied: $name ($($_.Exception.Message))"
        Body $size $null
        continue
      }
      if ($out) {
        try { Body $size $out } finally { $out.Close() }
        $files++
      } else {
        Body $size $null
      }
    } else {
      if ($type -eq '2') { $links++ }
      Body $size $null
    }
  }
  $in.Close()
  if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
  Move-Item $new $dest
  Remove-Item $archive -Force
  $note = ''
  if ($links) { $note = " ($links symbolic links left out)" }
  "$files files in $dest$note"
}
