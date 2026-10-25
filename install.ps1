# agent-mail installer for Windows x64.
#
#   irm https://github.com/ollie-dev/agent-mail/releases/latest/download/install.ps1 | iex
#
# Downloads the latest Windows x64 release, verifies its SHA-256 checksum,
# and only then installs agent-mail.exe to %USERPROFILE%\.local\bin. Never
# elevates, never runs what it downloads and never changes PATH. The
# download location can be overridden with AGENT_MAIL_DOWNLOAD_URL.
# Runs under Windows PowerShell 5.1 and PowerShell 7.

# Everything runs in its own scope, so settings made here do not leak into
# the human's shell when run through `iex`.
& {
    $ErrorActionPreference = 'Stop'
    Set-StrictMode -Version 2.0

    # Run as a file (-File), failures set a non-zero exit code; run through
    # `iex`, they only print, so the human's shell stays open.
    $AgentMailFromFile = [bool]$PSCommandPath

    function Install-AgentMail {
        $asset = 'agent-mail-x86_64-pc-windows-msvc.zip'
        $baseUrl = $env:AGENT_MAIL_DOWNLOAD_URL
        if (-not $baseUrl) {
            $baseUrl = 'https://github.com/ollie-dev/agent-mail/releases/latest/download'
        }
        $baseUrl = $baseUrl.TrimEnd('/')

        $arch = $env:PROCESSOR_ARCHITECTURE
        if ($arch -ne 'AMD64') {
            throw "unsupported architecture $arch; agent-mail for Windows is built for x64 (AMD64) only"
        }
        if (-not $env:USERPROFILE) {
            throw 'USERPROFILE is not set; nothing was installed'
        }
        $binDir = Join-Path $env:USERPROFILE '.local\bin'
        $dest = Join-Path $binDir 'agent-mail.exe'

        # TLS 1.2 for Windows PowerShell 5.1, whose default may be older.
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

        $tmp = Join-Path ([IO.Path]::GetTempPath()) ('agent-mail-install-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tmp | Out-Null
        $staged = $null
        try {
            $zip = Join-Path $tmp $asset
            $sumFile = "$zip.sha256"
            Write-Host "Downloading $asset ..."
            # WebClient handles both https:// and file:// on 5.1 and 7.
            $client = New-Object System.Net.WebClient
            try {
                try {
                    $client.DownloadFile("$baseUrl/$asset", $zip)
                } catch {
                    throw "could not download $baseUrl/$asset; nothing was installed"
                }
                try {
                    $client.DownloadFile("$baseUrl/$asset.sha256", $sumFile)
                } catch {
                    throw "could not download $baseUrl/$asset.sha256; nothing was installed"
                }
            } finally {
                $client.Dispose()
            }

            # The checksum file must name exactly this asset, so the check below
            # cannot be pointed at another file.
            $expected = [IO.File]::ReadAllText($sumFile)
            $pattern = '\A([0-9a-f]{64})  ' + [Regex]::Escape($asset) + '\n?\z'
            $match = [Regex]::Match($expected, $pattern)
            if (-not $match.Success) {
                throw "checksum file for $asset is malformed; nothing was installed"
            }
            $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLowerInvariant()
            if ($actual -cne $match.Groups[1].Value) {
                throw "checksum mismatch for $asset; nothing was installed"
            }
            Write-Host 'Checksum verified.'

            # Only now is anything extracted, and only the one expected entry.
            Add-Type -AssemblyName System.IO.Compression
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $binary = Join-Path $tmp 'agent-mail.exe'
            $archive = [IO.Compression.ZipFile]::OpenRead($zip)
            try {
                $entries = @($archive.Entries)
                if ($entries.Count -ne 1 -or $entries[0].FullName -cne 'agent-mail.exe') {
                    throw "$asset must hold exactly agent-mail.exe; nothing was installed"
                }
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $binary)
            } finally {
                $archive.Dispose()
            }

            New-Item -ItemType Directory -Force -Path $binDir | Out-Null
            # Stage next to the destination so the final step is a rename.
            $staged = Join-Path $binDir ('.agent-mail.install.' + [Guid]::NewGuid().ToString('N'))
            [IO.File]::Copy($binary, $staged)
            if (Test-Path -LiteralPath $dest) {
                [IO.File]::Replace($staged, $dest, [NullString]::Value)
            } else {
                [IO.File]::Move($staged, $dest)
            }
            $staged = $null
        } finally {
            if ($staged -and (Test-Path -LiteralPath $staged)) {
                Remove-Item -LiteralPath $staged -Force
            }
            Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
        }

        Write-Host "Installed agent-mail to $dest"
        $onPath = @($env:PATH -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -eq $binDir) }).Count -gt 0
        if (-not $onPath) {
            Write-Host "Note: $binDir is not on your PATH. Add it to your user Path environment variable to run agent-mail from any terminal."
        }
    }

    try {
        Install-AgentMail
    } catch {
        [Console]::Error.WriteLine("agent-mail installer: $($_.Exception.Message)")
        if ($AgentMailFromFile) {
            exit 1
        }
    }
}
