# agent-mail uninstaller for Windows.
#
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-KeepData] [-Force]
#
# Stops and removes the Broker (the service `agent-mail`, ADR-0004),
# deletes %ProgramFiles%\agent-mail and its machine Path entry, and deletes
# the stored Mandates and the audit log (%ProgramData%\agent-mail).
#
# The data is deleted only after a confirmation, asked in your own window
# before elevation. -Force deletes it without asking; -KeepData keeps it
# and wins over -Force. Asks for administrator rights once (UAC): run
# without them, it starts one elevated copy of its own text with -Force or
# -KeepData and waits for it. Runs under Windows PowerShell 5.1 and
# PowerShell 7.

param([switch]$KeepData, [switch]$Force, [switch]$Relaunched)

& {
    param([switch]$KeepData, [switch]$Force, [switch]$Relaunched)
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
        foreach ($module in @('Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'CimCmdlets')) {
            Import-Module -Name ([IO.Path]::Combine($PSHOME, 'Modules', $module, "$module.psd1"))
        }
        $PSModuleAutoLoadingPreference = 'None'
    }

    # This block's own text: the elevated copy runs exactly this.
    $AgentMailText = $MyInvocation.MyCommand.ScriptBlock.ToString()
    $AgentMailExits = [bool]$PSCommandPath -or $Relaunched

    $ServiceName = 'agent-mail'
    $Prefix = 'uninstall.ps1'
    $EnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
    $Folder = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'agent-mail'

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
        if (-not (Wait-Broker 'Stopped' 30)) { throw 'the Broker did not stop; nothing was removed' }
    }

    function Test-SameFolder([string]$Entry) {
        [string]::Equals($Entry.TrimEnd('\'), $Folder.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
    }

    function Get-RawMachinePath {
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($EnvKey, $false)
        try {
            [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        } finally {
            $key.Close()
        }
    }

    function Test-Installed {
        (Get-Broker) -or (Test-Path -LiteralPath $Folder) -or
            @((Get-RawMachinePath) -split ';' | Where-Object { Test-SameFolder $_ }).Count -gt 0
    }

    # Removes every machine Path entry naming the install folder; all other
    # entries stay in order with their raw values, and the value kind stays.
    function Remove-MachinePath {
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($EnvKey, $true)
        try {
            if ($null -eq $key.GetValue('Path')) { return }
            $raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $kind = $key.GetValueKind('Path')
            $entries = @($raw -split ';')
            $kept = @($entries | Where-Object { -not (Test-SameFolder $_) })
            if ($kept.Count -eq $entries.Count) { return }
            $key.SetValue('Path', ($kept -join ';'), $kind)
        } finally {
            $key.Close()
        }
        try {
            Add-Type -Namespace AgentMail -Name Env -MemberDefinition '[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);'
            $result = [UIntPtr]::Zero
            [void][AgentMail.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
        } catch { }
    }

    # The Broker started with the single argument `purge` deletes every
    # stored Mandate and the audit log, opens no pipe and stops: exit code
    # 0, or service-specific code 2 on any failure.
    function Invoke-Purge {
        if ((Invoke-Sc "start $ServiceName purge").Code -ne 0) { return $false }
        if (-not (Wait-Broker 'Stopped' 60)) { return $false }
        $status = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
        $status.ExitCode -eq 0 -and $status.ServiceSpecificExitCode -eq 0
    }

    function Uninstall-AgentMail([bool]$DeleteData) {
        $purged = $false
        if (Get-Broker) {
            Write-Host 'Stopping the Broker ...'
            Stop-Broker
            if ($DeleteData) {
                Write-Host 'Deleting the stored Mandates and the audit log ...'
                if (-not (Invoke-Purge)) {
                    try { (Get-Broker).Start() } catch { }
                    throw 'could not delete the stored Mandates; nothing else was removed'
                }
                $purged = $true
            }
            $run = Invoke-Sc "delete $ServiceName"
            if ($run.Code -ne 0) { throw "could not remove the Broker service: $($run.Output.Trim())" }
            $deadline = (Get-Date).AddSeconds(30)
            while ((Get-Broker) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
        }
        if (Test-Path -LiteralPath $Folder) {
            # The Broker's process may still be exiting for a moment.
            for ($try = 0; $try -lt 25 -and (Test-Path -LiteralPath $Folder); $try++) {
                if ($try) { Start-Sleep -Milliseconds 200 }
                Remove-Item -LiteralPath $Folder -Recurse -Force -ErrorAction SilentlyContinue
            }
            if (Test-Path -LiteralPath $Folder) {
                throw "could not delete $Folder; close every agent-mail program and rerun uninstall.ps1"
            }
        }
        Remove-MachinePath
        if ($purged) {
            Write-Host 'Deleted the stored Mandates and the audit log.'
        } elseif ($DeleteData) {
            Write-Host 'The Broker was not registered, so the stored Mandates and the audit log could not be deleted.'
        } else {
            Write-Host 'Kept the stored Mandates and the audit log.'
        }
        Write-Host 'agent-mail was removed.'
    }

    $code = 0
    if (-not (Test-Installed)) {
        Write-Host 'agent-mail is not installed.'
    } else {
        $deleteData = $null
        if ($KeepData) {
            $deleteData = $false
        } elseif ($Force) {
            $deleteData = $true
        } elseif (-not $Relaunched) {
            try {
                $answer = Read-Host 'Delete all stored Mandates and the audit log? Agents lose access and this cannot be undone. [y/N]'
                $deleteData = @('y', 'yes') -contains ([string]$answer).Trim().ToLowerInvariant()
                if (-not $deleteData) { Write-Host 'Keeping the stored Mandates and the audit log.' }
            } catch {
                $deleteData = $null
            }
        }
        if ($null -eq $deleteData) {
            [Console]::Error.WriteLine("${Prefix}: cannot ask for confirmation; rerun with -KeepData or -Force")
            $code = 1
        } elseif ($IsAdministrator) {
            try {
                Uninstall-AgentMail $deleteData
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
            [Console]::Error.WriteLine("${Prefix}: administrator rights are needed; nothing was removed")
            $code = 1
        } else {
            # One elevated copy of this block's own text, told the answer
            # with -Force or -KeepData, gzip-compressed to stay well under
            # the 32,767-character command line limit.
            $choice = if ($deleteData) { '-Force' } else { '-KeepData' }
            $inner = "& {$AgentMailText} $choice -Relaunched"
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
            Write-Host 'Asking for administrator rights; the uninstaller continues in a new window.'
            try {
                $elevated = Start-Process -FilePath $hostExe -Verb RunAs -Wait -PassThru -ArgumentList "-NoProfile -EncodedCommand $encoded"
                $code = $elevated.ExitCode
                if ($code -eq 0) {
                    Write-Host 'agent-mail was removed.'
                } else {
                    [Console]::Error.WriteLine("${Prefix}: the removal failed; its window showed why")
                }
            } catch {
                [Console]::Error.WriteLine("${Prefix}: administrator rights are needed; nothing was removed")
                $code = 1
            }
        }
    }
    $env:PSModulePath = $SavedModulePath
    if ($AgentMailExits) { exit $code }
} -KeepData:$KeepData -Force:$Force -Relaunched:$Relaunched
