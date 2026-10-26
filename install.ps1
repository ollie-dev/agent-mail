# agent-mail installer for Windows x64.
#
#   irm https://github.com/ollie-dev/agent-mail/releases/latest/download/install.ps1 | iex
#
# Installs agent-mail system-wide and registers the Broker (ADR-0004), the
# service `agent-mail` under `NT SERVICE\agent-mail`. Asks for administrator
# rights once (UAC): run without them, it starts one elevated copy of its
# own text and waits for it. Rerun it to update in place; the stored
# Mandates are kept. Remove agent-mail with uninstall.ps1.
#
# The elevated copy downloads the latest Windows x64 release into a fresh
# folder inside %ProgramFiles%\agent-mail, which only administrators can
# write, verifies its SHA-256 checksum, only then extracts agent-mail.exe,
# checks that it runs (`--version`), installs it, registers and starts the
# Broker with delayed automatic start, and appends the folder to the
# machine Path. Nothing is staged under %TEMP%.
#
# -DownloadUrl <url> replaces the release download location (tests use a
# file:// directory). It is only ever read from the arguments, never from
# the environment. Runs under Windows PowerShell 5.1 and PowerShell 7.

param([string]$DownloadUrl, [switch]$Relaunched)

# Everything runs in its own scope, so settings made here do not leak into
# the operator's shell when run through `iex`.
& {
    param([string]$DownloadUrl, [switch]$Relaunched)
    $ErrorActionPreference = 'Stop'
    Set-StrictMode -Version 2.0

    # Elevated, only the modules this PowerShell ships with are loaded, and
    # nothing is autoloaded: the user's own PSModulePath entries (such as
    # Documents\WindowsPowerShell\Modules) are writable by every process of
    # the user, so a module planted there must never run as administrator.
    # Only .NET and built-in commands run before this.
    $IsAdministrator = ([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    $SavedModulePath = $env:PSModulePath
    if ($IsAdministrator) {
        $env:PSModulePath = [IO.Path]::Combine($PSHOME, 'Modules')
        foreach ($module in @('Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility')) {
            Import-Module -Name ([IO.Path]::Combine($PSHOME, 'Modules', $module, "$module.psd1"))
        }
        $PSModuleAutoLoadingPreference = 'None'
    }

    # This block's own text: the elevated copy runs exactly this, never a
    # file another process could swap in the meantime.
    $AgentMailText = $MyInvocation.MyCommand.ScriptBlock.ToString()
    # Run as a file or as the elevated copy, the script ends with an exit
    # code; run through `iex`, failures only print, so the shell stays open.
    $AgentMailExits = [bool]$PSCommandPath -or $Relaunched

    $ServiceName = 'agent-mail'
    $Asset = 'agent-mail-x86_64-pc-windows-msvc.zip'
    $Prefix = 'agent-mail installer'
    # Only SYSTEM and Administrators may start, stop or reconfigure the
    # Broker; interactive and service users may query it.
    $ServiceSddl = 'D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)'
    $EnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'

    # Runs `file` with the command line `arguments`, passed verbatim, and
    # returns its exit code and output.
    function Invoke-Native([string]$File, [string]$Arguments) {
        $info = New-Object Diagnostics.ProcessStartInfo
        $info.FileName = $File
        $info.Arguments = $Arguments
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        $process = [Diagnostics.Process]::Start($info)
        $err = $process.StandardError.ReadToEndAsync()
        $out = $process.StandardOutput.ReadToEnd()
        $process.WaitForExit()
        [pscustomobject]@{ Code = $process.ExitCode; Output = $out + $err.Result }
    }

    function Invoke-Sc([string]$Arguments) {
        Invoke-Native (Join-Path ([Environment]::SystemDirectory) 'sc.exe') $Arguments
    }

    function Get-Broker {
        Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    }

    # Waits up to `seconds` for the Broker to reach `state`.
    function Wait-Broker([string]$State, [int]$Seconds) {
        $deadline = (Get-Date).AddSeconds($Seconds)
        do {
            $service = Get-Broker
            if ($service -and $service.Status.ToString() -eq $State) { return $true }
            Start-Sleep -Milliseconds 200
        } while ((Get-Date) -lt $deadline)
        $false
    }

    function Stop-Broker {
        $service = Get-Broker
        if (-not $service -or $service.Status.ToString() -eq 'Stopped') { return }
        if ($service.Status.ToString() -ne 'StopPending') { $service.Stop() }
        if (-not (Wait-Broker 'Stopped' 30)) { throw 'the Broker did not stop; nothing was changed' }
    }

    # Starts the Broker; whether it reached Running within 30 seconds.
    function Start-Broker {
        try { (Get-Broker).Start() } catch { return $false }
        Wait-Broker 'Running' 30
    }

    # The `--version` line of `exe` as `x.y.z`, or $null if it does not run
    # or prints anything else.
    function Get-BinaryVersion([string]$Exe) {
        try {
            $run = Invoke-Native $Exe '--version'
        } catch {
            return $null
        }
        $match = [Regex]::Match($run.Output, '\Aagent-mail (\d+\.\d+\.\d+)\r?\n?\z')
        if ($run.Code -ne 0 -or -not $match.Success) { return $null }
        $match.Groups[1].Value
    }

    # Downloads, verifies and extracts the release into `staging`; returns
    # the extracted agent-mail.exe.
    function Get-Release([string]$Staging) {
        $baseUrl = $DownloadUrl
        if (-not $baseUrl) { $baseUrl = 'https://github.com/ollie-dev/agent-mail/releases/latest/download' }
        $baseUrl = $baseUrl.TrimEnd('/')
        # TLS 1.2 for Windows PowerShell 5.1, whose default may be older.
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        $zip = Join-Path $Staging $Asset
        $sumFile = "$zip.sha256"
        Write-Host "Downloading $Asset ..."
        # WebClient handles both https:// and file:// on 5.1 and 7.
        $client = New-Object System.Net.WebClient
        try {
            try { $client.DownloadFile("$baseUrl/$Asset", $zip) }
            catch { throw "could not download $baseUrl/$Asset; nothing was installed" }
            try { $client.DownloadFile("$baseUrl/$Asset.sha256", $sumFile) }
            catch { throw "could not download $baseUrl/$Asset.sha256; nothing was installed" }
        } finally {
            $client.Dispose()
        }

        # The checksum file must name exactly this asset, so the check below
        # cannot be pointed at another file.
        $expected = [IO.File]::ReadAllText($sumFile)
        $match = [Regex]::Match($expected, '\A([0-9a-f]{64})  ' + [Regex]::Escape($Asset) + '\n?\z')
        if (-not $match.Success) { throw "checksum file for $Asset is malformed; nothing was installed" }
        $sha = [Security.Cryptography.SHA256]::Create()
        $stream = [IO.File]::OpenRead($zip)
        try {
            $actual = -join ($sha.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') })
        } finally {
            $stream.Dispose()
            $sha.Dispose()
        }
        if ($actual -cne $match.Groups[1].Value) { throw "checksum mismatch for $Asset; nothing was installed" }
        Write-Host 'Checksum verified.'

        # Only now is anything extracted, and only the one expected entry.
        Add-Type -AssemblyName System.IO.Compression
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $binary = Join-Path $Staging 'agent-mail.exe'
        $archive = [IO.Compression.ZipFile]::OpenRead($zip)
        try {
            $entries = @($archive.Entries)
            if ($entries.Count -ne 1 -or $entries[0].FullName -cne 'agent-mail.exe') {
                throw "$Asset must hold exactly agent-mail.exe; nothing was installed"
            }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $binary)
        } finally {
            $archive.Dispose()
        }
        $binary
    }

    function Test-SameFolder([string]$Entry, [string]$Folder) {
        [string]::Equals($Entry.TrimEnd('\'), $Folder.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
    }

    # Appends `folder` to the raw machine Path unless an entry already
    # names it, keeping every other entry and the value kind.
    function Add-MachinePath([string]$Folder) {
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($EnvKey, $true)
        try {
            $raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
            if ($null -ne $key.GetValue('Path')) { $kind = $key.GetValueKind('Path') }
            if (@($raw -split ';' | Where-Object { Test-SameFolder $_ $Folder }).Count -gt 0) { return }
            # Always `;` first, so removing the entry restores the value.
            $value = if ($raw) { "$raw;$Folder" } else { $Folder }
            $key.SetValue('Path', $value, $kind)
        } finally {
            $key.Close()
        }
        # Tell Explorer, so new terminals see the change. Deleting a value
        # that does not exist changes nothing, but .NET then broadcasts
        # WM_SETTINGCHANGE itself, with no C# compiled at run time.
        try {
            [Environment]::SetEnvironmentVariable('AGENT_MAIL_INSTALLER_UNUSED', $null, 'Machine')
        } catch {
            Write-Host 'Could not announce the Path change; sign out and in again if a new terminal does not find agent-mail.'
        }
    }

    function Set-Broker([string]$Exe, [bool]$Create) {
        $binPath = 'binPath= "\"' + $Exe + '\" broker"'
        if ($Create) {
            $run = Invoke-Sc "create $ServiceName $binPath obj= `"NT SERVICE\$ServiceName`" start= delayed-auto type= own DisplayName= `"agent-mail Broker`""
        } else {
            $run = Invoke-Sc "config $ServiceName $binPath obj= `"NT SERVICE\$ServiceName`" start= delayed-auto type= own"
        }
        if ($run.Code -ne 0) { throw "could not register the Broker service: $($run.Output.Trim())" }
        $run = Invoke-Sc "sdset $ServiceName $ServiceSddl"
        if ($run.Code -ne 0) { throw "could not set the Broker service's access list: $($run.Output.Trim())" }
        [void](Invoke-Sc "description $ServiceName `"Holds the agent-mail Mandates and serves agent-mail clients.`"")
    }

    # Why this machine or host cannot run the installer, or $null.
    function Get-ArchitectureProblem {
        $arch = $env:PROCESSOR_ARCHITECTURE
        if ($arch -ne 'AMD64') {
            return "unsupported architecture $arch; agent-mail for Windows is built for x64 (AMD64) only; nothing was installed"
        }
        if (-not [Environment]::Is64BitProcess) { return 'run the installer from 64-bit PowerShell; nothing was installed' }
        $null
    }

    # Deletes `path`, retrying while a process that just stopped still
    # holds it.
    function Remove-Retrying([string]$Path) {
        for ($try = 1; ; $try++) {
            try {
                if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Force }
                return
            } catch {
                if ($try -ge 25) { throw }
                Start-Sleep -Milliseconds 200
            }
        }
    }

    # Puts the previous binary `old` back as `exe` with the Broker stopped,
    # then starts the Broker again.
    function Restore-Previous([string]$Old, [string]$Exe) {
        try { Stop-Broker } catch { }
        Remove-Retrying $Exe
        [IO.File]::Move($Old, $Exe)
        [void](Start-Broker)
    }

    function Install-AgentMail {
        $folder = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'agent-mail'
        $exe = Join-Path $folder 'agent-mail.exe'
        $service = Get-Broker
        $createdFolder = -not (Test-Path -LiteralPath $folder)
        if ($createdFolder) { New-Item -ItemType Directory -Path $folder | Out-Null }
        $staging = $null
        $old = $null
        try {
            # Only SYSTEM and Administrators may write the folder; Users may
            # read and run what it holds. Set explicitly, inherited by all.
            $acl = Invoke-Native (Join-Path ([Environment]::SystemDirectory) 'icacls.exe') "`"$folder`" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F *S-1-5-32-545:(OI)(CI)RX"
            if ($acl.Code -ne 0) { throw "could not set the access list of $folder; nothing was installed" }
            # Leftovers of earlier runs: staging folders and renamed copies
            # that were still in use.
            Get-ChildItem -LiteralPath $folder -Force | Where-Object { $_.Name -like '.staging-*' -or $_.Name -like 'agent-mail.exe.old-*' } |
                ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue }

            $staging = Join-Path $folder ('.staging-' + [Guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path $staging | Out-Null
            $binary = Get-Release $staging
            $version = Get-BinaryVersion $binary
            if (-not $version) {
                if (Test-Path -LiteralPath $exe) { throw 'the new binary did not start; the previous version was restored' }
                throw 'the new binary did not start; nothing was installed'
            }

            if ($service) {
                Write-Host 'Stopping the Broker ...'
                Stop-Broker
            }
            if (Test-Path -LiteralPath $exe) {
                # A running agent-mail.exe (an MCP server, say) can be renamed
                # but not replaced.
                $old = Join-Path $folder ('agent-mail.exe.old-' + [Guid]::NewGuid().ToString('N'))
                [IO.File]::Move($exe, $old)
            }
            [IO.File]::Move($binary, $exe)
            Set-Broker $exe (-not $service)
            Write-Host 'Starting the Broker ...'
            if (-not (Start-Broker)) {
                if ($old) {
                    $previous = $old
                    $old = $null
                    Restore-Previous $previous $exe
                    throw 'the new binary did not start; the previous version was restored'
                }
                throw 'the new binary did not start; nothing was installed'
            }
            if ($old) {
                # The old Broker's process may still be exiting; a copy an
                # MCP server still runs from stays until the next run.
                $previous = $old
                $old = $null
                try { Remove-Retrying $previous } catch { }
            }
            Add-MachinePath $folder
        } catch {
            # A failed install leaves what was there before: no service it
            # registered, no folder it created, the previous binary.
            $failure = $_
            try {
                if (-not $service -and (Get-Broker)) {
                    try { Stop-Broker } catch { }
                    [void](Invoke-Sc "delete $ServiceName")
                }
                if ($old) { Restore-Previous $old $exe }
                if ($createdFolder) { Remove-Item -LiteralPath $folder -Recurse -Force -ErrorAction SilentlyContinue }
            } catch { }
            throw $failure
        } finally {
            if ($staging -and (Test-Path -LiteralPath $staging)) {
                Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
            }
        }
        Write-Host "Installed agent-mail $version to $exe; the Broker is running."
        Write-Host "Open a new terminal to use agent-mail from the Path."
    }

    $code = 0
    $problem = Get-ArchitectureProblem
    if ($problem) {
        [Console]::Error.WriteLine("${Prefix}: $problem")
        $code = 1
    } elseif ($IsAdministrator) {
        try {
            Install-AgentMail
        } catch {
            [Console]::Error.WriteLine("${Prefix}: $($_.Exception.Message)")
            $code = 1
        }
        if ($Relaunched) {
            Write-Host ''
            [void](Read-Host 'Press Enter to close this window')
        }
    } elseif ($Relaunched) {
        # The elevated copy never relaunches.
        [Console]::Error.WriteLine("${Prefix}: administrator rights are needed; nothing was installed")
        $code = 1
    } else {
        # One elevated copy of this block's own text, with the arguments
        # inside its command line, gzip-compressed to stay well under the
        # 32,767-character limit.
        $url = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$DownloadUrl))
        $inner = "& {$AgentMailText} -DownloadUrl ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('$url'))) -Relaunched"
        $buffer = New-Object IO.MemoryStream
        $gzip = New-Object IO.Compression.GZipStream($buffer, [IO.Compression.CompressionMode]::Compress)
        $bytes = [Text.Encoding]::UTF8.GetBytes($inner)
        $gzip.Write($bytes, 0, $bytes.Length)
        $gzip.Close()
        $payload = [Convert]::ToBase64String($buffer.ToArray())
        $boot = "`$z = [IO.Compression.GZipStream]::new([IO.MemoryStream]::new([Convert]::FromBase64String('$payload')), [IO.Compression.CompressionMode]::Decompress); & ([ScriptBlock]::Create([IO.StreamReader]::new(`$z, [Text.Encoding]::UTF8).ReadToEnd()))"
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($boot))
        # Always the system's Windows PowerShell, which only administrators
        # can replace, never a host the user could have installed.
        $hostExe = [IO.Path]::Combine([Environment]::SystemDirectory, 'WindowsPowerShell\v1.0\powershell.exe')
        Write-Host 'Asking for administrator rights; the installer continues in a new window.'
        try {
            $elevated = Start-Process -FilePath $hostExe -Verb RunAs -Wait -PassThru -ArgumentList "-NoProfile -EncodedCommand $encoded"
            $code = $elevated.ExitCode
            if ($code -eq 0) {
                Write-Host 'agent-mail was installed. Open a new terminal to use it.'
            } else {
                [Console]::Error.WriteLine("${Prefix}: the installation failed; its window showed why")
            }
        } catch {
            [Console]::Error.WriteLine("${Prefix}: administrator rights are needed; nothing was installed")
            $code = 1
        }
    }
    $env:PSModulePath = $SavedModulePath
    if ($AgentMailExits) { exit $code }
} -DownloadUrl $DownloadUrl -Relaunched:$Relaunched
