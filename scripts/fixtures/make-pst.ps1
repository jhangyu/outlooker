<#
.SYNOPSIS
  Generates the Outlooker fixture PSTs of spec Part 04 §12.1 with Outlook desktop (COM).
.PARAMETER Out
  Output directory (created if missing). Existing fixture files there are replaced.
#>
param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$PR_MESSAGE_ID   = 'http://schemas.microsoft.com/mapi/proptag/0x1035001F'
$PR_SUBMIT_TIME  = 'http://schemas.microsoft.com/mapi/proptag/0x00390040'
$PR_SENDER_NAME  = 'http://schemas.microsoft.com/mapi/proptag/0x0C1A001F'
$PR_SENDER_EMAIL = 'http://schemas.microsoft.com/mapi/proptag/0x0C1F001F'
$PR_HTML         = 'http://schemas.microsoft.com/mapi/proptag/0x10130102'
$PR_INET_CPID    = 'http://schemas.microsoft.com/mapi/proptag/0x3FDE0003'
$PR_CONTENT_ID   = 'http://schemas.microsoft.com/mapi/proptag/0x3712001F'

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path $env:TEMP ("outlooker-fixtures-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$ol = New-Object -ComObject Outlook.Application
$ns = $ol.GetNamespace('MAPI')

function Set-Prop($item, $tag, $value) {
  $item.PropertyAccessor.SetProperty($tag, $value)
  $back = $item.PropertyAccessor.GetProperty($tag)
  if (($value -is [byte[]] -and -not [System.Linq.Enumerable]::SequenceEqual([byte[]]$back, [byte[]]$value)) -or
      ($value -isnot [byte[]] -and "$back" -ne "$value")) {
    throw "Outlook did not keep property $tag (wrote '$value', read '$back')"
  }
}

function New-Store($name) {
  $path = Join-Path $Out "$name.pst"
  if (Test-Path $path) { Remove-Item $path }
  $ns.AddStoreEx($path, 2)   # olStoreUnicode
  $store = $ns.Stores | Where-Object { $_.FilePath -eq $path }
  if (-not $store) { throw "store $path not mounted" }
  return @{ Path = $path; Store = $store; Root = $store.GetRootFolder() }
}

function Close-Store($s) { $ns.RemoveStore($s.Root) }

function Get-Folder($parent, $name) {
  foreach ($f in $parent.Folders) { if ($f.Name -eq $name) { return $f } }
  return $parent.Folders.Add($name)
}

function New-Mail($folder, [hashtable]$m) {
  $item = $ol.CreateItem(0)   # olMailItem
  $item.Subject = $m.Subject
  $item.To = $m.To
  if ($m.Cc) { $item.CC = $m.Cc }
  switch ($m.Body) {
    'html' { $item.HTMLBody = $m.Html }
    'text' { $item.BodyFormat = 1; $item.Body = $m.Text }
    'rtf'  { $item.BodyFormat = 3; $item.RTFBody = [Text.Encoding]::ASCII.GetBytes($m.Rtf) }
  }
  if ($m.Attach) { foreach ($a in $m.Attach) { $att = $item.Attachments.Add($a.Path, $a.Type, 1, $a.Name); if ($a.Cid) { Set-Prop $att $PR_CONTENT_ID $a.Cid } } }
  if ($m.Embed) { $item.Attachments.Add($m.Embed, 5) | Out-Null }   # olEmbeddeditem
  $item.Save()
  if ($m.MessageId) { Set-Prop $item $PR_MESSAGE_ID $m.MessageId }
  Set-Prop $item $PR_SUBMIT_TIME $m.Sent
  Set-Prop $item $PR_SENDER_NAME $m.FromName
  Set-Prop $item $PR_SENDER_EMAIL $m.FromEmail
  if ($m.RawHtml) { Set-Prop $item $PR_HTML $m.RawHtml; Set-Prop $item $PR_INET_CPID $m.Codepage }
  $item.Save()
  $moved = $item.Move($folder)
  return $moved
}

function New-Png($path, $w, $h, $color) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear($color); $g.Dispose()
  $fmt = switch ([IO.Path]::GetExtension($path)) { '.jpg' { [System.Drawing.Imaging.ImageFormat]::Jpeg } '.gif' { [System.Drawing.Imaging.ImageFormat]::Gif } default { [System.Drawing.Imaging.ImageFormat]::Png } }
  $bmp.Save($path, $fmt); $bmp.Dispose()
}

$base = [datetime]::new(2023, 3, 1, 9, 0, 0, [DateTimeKind]::Utc)
function Sent($i) { return $base.AddHours($i) }
$rtfOnly = '{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0 Arial;}}\f0\fs20 RTF only body number NUM.\par}'

# ---- basic.pst: 50 mails, Inbox / Sent Items / Inbox/Projects, HTML + plain + RTF-only bodies
$s = New-Store 'basic'
$inbox = Get-Folder $s.Root 'Inbox'; $sent = Get-Folder $s.Root 'Sent Items'; $proj = Get-Folder $inbox 'Projects'
for ($i = 0; $i -lt 50; $i++) {
  $kind = @('html', 'text', 'rtf')[$i % 3]
  $folder = @($inbox, $sent, $proj)[$i % 3]
  New-Mail $folder @{ Subject = "Basic mail $i"; To = "alice$i@example.com"; Body = $kind
    Html = "<html><body><p>Basic HTML body $i invoice</p></body></html>"; Text = "Basic plain body $i invoice"
    Rtf = $rtfOnly.Replace('NUM', "$i"); MessageId = "<basic-$i@fixtures.outlooker>"; Sent = (Sent $i)
    FromName = 'Basic Sender'; FromEmail = 'sender@example.com' } | Out-Null
}
Close-Store $s

# ---- cjk.pst: 30 mails in Traditional Chinese, Big5 and UTF-8 HTML, mixed EN/ZH subjects
$s = New-Store 'cjk'
$inbox = Get-Folder $s.Root 'Inbox'
$big5 = [Text.Encoding]::GetEncoding(950)
for ($i = 0; $i -lt 30; $i++) {
  $html = "<html><head><meta charset=""" + $(if ($i % 2) { 'big5' } else { 'utf-8' }) + """></head><body><p>報價單 第 $i 號，請查收郵件系統通知。</p></body></html>"
  $bytes = if ($i % 2) { $big5.GetBytes($html) } else { [Text.Encoding]::UTF8.GetBytes($html) }
  New-Mail $inbox @{ Subject = $(if ($i % 3) { "報價單 $i" } else { "Quotation 報價 $i" }); To = "客戶$i <client$i@example.com.tw>"
    Body = 'html'; Html = $html; RawHtml = $bytes; Codepage = $(if ($i % 2) { 950 } else { 65001 })
    MessageId = "<cjk-$i@fixtures.outlooker>"; Sent = (Sent $i); FromName = '王小明'; FromEmail = 'wang@example.com.tw' } | Out-Null
}
Close-Store $s

# ---- inline.pst: 20 mails with cid images; the same logo in 10 mails
$img = Join-Path $work 'img'; New-Item -ItemType Directory -Path $img | Out-Null
New-Png (Join-Path $img 'logo.png') 120 40 ([System.Drawing.Color]::FromArgb(15, 108, 189))
New-Png (Join-Path $img 'photo.jpg') 800 600 ([System.Drawing.Color]::Orange)
New-Png (Join-Path $img 'anim.gif') 64 64 ([System.Drawing.Color]::Green)
New-Png (Join-Path $img 'pixel.png') 1 1 ([System.Drawing.Color]::White)
New-Png (Join-Path $img 'unused.png') 32 32 ([System.Drawing.Color]::Red)
$s = New-Store 'inline'
$inbox = Get-Folder $s.Root 'Inbox'
$dataUri = 'data:image/png;base64,' + [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $img 'pixel.png')))
for ($i = 0; $i -lt 20; $i++) {
  $att = @()
  $html = "<html><body><p>Inline image mail $i</p>"
  if ($i -lt 10) { $att += @{ Path = (Join-Path $img 'logo.png'); Type = 1; Name = 'logo.png'; Cid = "logo$i@x" }; $html += "<img src=""cid:logo$i@x"">" }
  switch ($i % 5) {
    0 { $att += @{ Path = (Join-Path $img 'photo.jpg'); Type = 1; Name = 'photo.jpg'; Cid = "photo$i@x" }; $html += "<img src=""cid:photo$i@x"">" }
    1 { $att += @{ Path = (Join-Path $img 'anim.gif'); Type = 1; Name = 'anim.gif'; Cid = "gif$i@x" }; $html += "<img src=""cid:gif$i@x"">" }
    2 { $att += @{ Path = (Join-Path $img 'pixel.png'); Type = 1; Name = 'pixel.png'; Cid = "px$i@x" }; $html += "<img src=""cid:px$i@x"" width=1 height=1>" }
    3 { $att += @{ Path = (Join-Path $img 'unused.png'); Type = 1; Name = 'unused.png'; Cid = "unused$i@x" }; $html += "<img src=""$dataUri"">" }
    4 { $html += "<img src=""https://tracker.example.com/p.gif"">" }
  }
  New-Mail $inbox @{ Subject = "Inline $i"; To = 'bob@example.com'; Body = 'html'; Html = ($html + '</body></html>'); Attach = $att
    MessageId = "<inline-$i@fixtures.outlooker>"; Sent = (Sent $i); FromName = 'Image Sender'; FromEmail = 'img@example.com' } | Out-Null
}
Close-Store $s

# ---- attach.pst: file, embedded .msg, missing MIME tag, long file name
$s = New-Store 'attach'
$inbox = Get-Folder $s.Root 'Inbox'
$file = Join-Path $work 'report.pdf'; [IO.File]::WriteAllBytes($file, [byte[]](1..200))
$bin = Join-Path $work 'blob.unknownext'; [IO.File]::WriteAllBytes($bin, [byte[]](1..64))
$long = Join-Path $work (('L' * 180) + '.txt'); [IO.File]::WriteAllText($long, 'long name')
$embed = $ol.CreateItem(0); $embed.Subject = 'Embedded message'; $embed.Body = 'inner'; $embed.Save()
New-Mail $inbox @{ Subject = 'Attach file'; To = 'c@example.com'; Body = 'text'; Text = 'see file'; Attach = @(@{ Path = $file; Type = 1; Name = 'report.pdf' })
  MessageId = '<attach-1@fixtures.outlooker>'; Sent = (Sent 1); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
New-Mail $inbox @{ Subject = 'Attach embedded'; To = 'c@example.com'; Body = 'text'; Text = 'see msg'; Embed = $embed
  MessageId = '<attach-2@fixtures.outlooker>'; Sent = (Sent 2); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
New-Mail $inbox @{ Subject = 'Attach no mime'; To = 'c@example.com'; Body = 'text'; Text = 'see blob'; Attach = @(@{ Path = $bin; Type = 1; Name = 'blob.unknownext' })
  MessageId = '<attach-3@fixtures.outlooker>'; Sent = (Sent 3); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
New-Mail $inbox @{ Subject = 'Attach long name'; To = 'c@example.com'; Body = 'text'; Text = 'see long'; Attach = @(@{ Path = $long; Type = 1; Name = [IO.Path]::GetFileName($long) })
  MessageId = '<attach-4@fixtures.outlooker>'; Sent = (Sent 4); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
$embed.Delete()
Close-Store $s

# ---- dup-a.pst / dup-b.pst: 40 each; 25 shared by Message-ID, 5 shared without Message-ID; one dup-a mail also in a 2nd folder
function Dup-Mail($folder, $key, [bool]$withId) {
  $m = @{ Subject = "Dup $key"; To = 'd@example.com'; Body = 'text'; Text = "Dup body $key"; Sent = (Sent ([int]($key -replace '\D', '')))
          FromName = 'Dup Sender'; FromEmail = 'dup@example.com' }
  if ($withId) { $m.MessageId = "<dup-$key@fixtures.outlooker>" }
  return New-Mail $folder $m
}
foreach ($name in 'dup-a', 'dup-b') {
  $s = New-Store $name
  $inbox = Get-Folder $s.Root 'Inbox'
  for ($i = 0; $i -lt 25; $i++) { Dup-Mail $inbox "shared$i" $true | Out-Null }
  for ($i = 0; $i -lt 5; $i++) { Dup-Mail $inbox "nomid$i" $false | Out-Null }
  for ($i = 0; $i -lt 10; $i++) { Dup-Mail $inbox "$name-own$i" $true | Out-Null }
  if ($name -eq 'dup-a') {
    $archive = Get-Folder $s.Root 'Archive'
    $first = $inbox.Items | Where-Object { $_.Subject -eq 'Dup shared0' } | Select-Object -First 1
    $first.Copy().Move($archive) | Out-Null
  }
  Close-Store $s
}

# ---- hostile.pst: sanitizer corpus
$s = New-Store 'hostile'
$inbox = Get-Folder $s.Root 'Inbox'
$hostile = @(
  '<script>alert(1)</script><p>script</p>',
  '<img src="x" onerror="alert(1)"><p>onerror</p>',
  '<a href="javascript:alert(1)">js link</a>',
  '<div style="background:url(https://evil.example/x.png)">css url</div>',
  '<div style="position:fixed;top:0;left:0;width:100%;height:100%">overlay</div>',
  '<iframe src="https://evil.example"></iframe><p>iframe</p>',
  '<form action="https://evil.example"><input name="p"></form>'
)
for ($i = 0; $i -lt $hostile.Count; $i++) {
  New-Mail $inbox @{ Subject = "Hostile $i"; To = 'e@example.com'; Body = 'html'; Html = "<html><body>$($hostile[$i])</body></html>"
    MessageId = "<hostile-$i@fixtures.outlooker>"; Sent = (Sent $i); FromName = 'H'; FromEmail = 'h@example.com' } | Out-Null
}
Close-Store $s

# ---- corrupt.pst: basic.pst with 512 bytes zeroed at 60% of the file (block-aligned)
$src = Join-Path $Out 'basic.pst'; $dst = Join-Path $Out 'corrupt.pst'
Copy-Item $src $dst -Force
$bytes = [IO.File]::ReadAllBytes($dst)
$offset = [int]([math]::Floor($bytes.Length * 0.6 / 512) * 512)
for ($k = 0; $k -lt 512; $k++) { $bytes[$offset + $k] = 0 }
[IO.File]::WriteAllBytes($dst, $bytes)

# ---- checks
$expected = 'basic', 'cjk', 'inline', 'attach', 'dup-a', 'dup-b', 'hostile', 'corrupt'
foreach ($n in $expected) {
  $f = Get-Item (Join-Path $Out "$n.pst")
  if ($f.Length -ge 5MB) { throw "$n.pst is $($f.Length) bytes (limit 5 MB)" }
}
Remove-Item -Recurse -Force $work
Write-Output "fixtures ok: $($expected -join ', ')"
