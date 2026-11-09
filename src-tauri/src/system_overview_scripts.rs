pub(crate) const PREAMBLE: &str = r#"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Convert-ToIsoDate($Value) {
    if ($null -eq $Value -or [string]::IsNullOrWhiteSpace([string]$Value)) { return $null }
    try {
        if ($Value -is [DateTime]) { return $Value.ToString('o') }
        return ([DateTime]::Parse([string]$Value)).ToString('o')
    } catch {
        return $null
    }
}

function Convert-RegistryInstallDate($Value) {
    if ($null -eq $Value) { return $null }
    $text = [string]$Value
    if ($text -notmatch '^\d{8}$') { return $null }
    try {
        return [DateTime]::ParseExact(
            $text,
            'yyyyMMdd',
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::None
        ).ToString('yyyy-MM-dd')
    } catch {
        return $null
    }
}

function Get-LicenseStatus($Code) {
    if ($null -eq $Code) { return 'Unknown' }
    switch ([int]$Code) {
        0 { return 'Unlicensed' }
        1 { return 'Licensed' }
        2 { return 'Grace' }
        3 { return 'Grace' }
        4 { return 'Notification' }
        5 { return 'Grace' }
        default { return 'Unknown' }
    }
}
"#;

const BASIC: &str = r#"
function Get-WebView2Version {
    $paths = @(
        'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    foreach ($path in $paths) {
        try {
            $version = Get-ItemPropertyValue -LiteralPath $path -Name 'pv' -ErrorAction Stop
            if (-not [string]::IsNullOrWhiteSpace([string]$version)) { return [string]$version }
        } catch {}
    }
    return $null
}

$result = [ordered]@{
    osName = $null
    displayVersion = $null
    version = $null
    build = $null
    architecture = $null
    deviceName = $null
    userName = $null
    manufacturer = $null
    model = $null
    domain = $null
    installDate = $null
    lastBootTime = $null
    uptimeSeconds = 0
    timeZone = $null
    locale = $null
    powerShellVersion = $PSVersionTable.PSVersion.ToString()
    webView2Version = $null
}
$os = $null
$computer = $null
$versionInfo = $null
try { $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop } catch {}
try { $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop } catch {}
try {
    $versionInfo = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction Stop
} catch {}

if ($null -ne $versionInfo) {
    $result.osName = $versionInfo.ProductName
    if ([string]::IsNullOrWhiteSpace([string]$versionInfo.DisplayVersion)) {
        $result.displayVersion = $versionInfo.ReleaseId
    } else {
        $result.displayVersion = $versionInfo.DisplayVersion
    }
    $build = [string]$versionInfo.CurrentBuild
    if ([string]::IsNullOrWhiteSpace($build)) { $build = [string]$versionInfo.CurrentBuildNumber }
    if (-not [string]::IsNullOrWhiteSpace($build) -and $null -ne $versionInfo.UBR) {
        $build = "$build.$($versionInfo.UBR)"
    }
    $result.build = $build
    if ([string]::IsNullOrWhiteSpace([string]$result.version)) {
        $result.version = $versionInfo.CurrentVersion
    }
}
if ($null -ne $os) {
    if ([string]::IsNullOrWhiteSpace([string]$result.osName)) { $result.osName = $os.Caption }
    $result.version = $os.Version
    if ([string]::IsNullOrWhiteSpace([string]$result.build)) { $result.build = $os.BuildNumber }
    $result.architecture = $os.OSArchitecture
    $result.installDate = Convert-ToIsoDate $os.InstallDate
    $result.lastBootTime = Convert-ToIsoDate $os.LastBootUpTime
    try { $result.uptimeSeconds = [Math]::Max(0, [Math]::Floor(((Get-Date) - $os.LastBootUpTime).TotalSeconds)) } catch {}
}
if ([string]::IsNullOrWhiteSpace([string]$result.architecture)) {
    $result.architecture = $env:PROCESSOR_ARCHITECTURE
}
if ($null -ne $computer) {
    $result.deviceName = $computer.Name
    $result.manufacturer = $computer.Manufacturer
    $result.model = $computer.Model
    $result.domain = $computer.Domain
}
if ([string]::IsNullOrWhiteSpace([string]$result.deviceName)) { $result.deviceName = $env:COMPUTERNAME }
$result.userName = [Environment]::UserName
try { $result.timeZone = (Get-TimeZone -ErrorAction Stop).Id } catch {
    try { $result.timeZone = [TimeZoneInfo]::Local.Id } catch {}
}
try { $result.locale = (Get-Culture -ErrorAction Stop).Name } catch {}
$result.webView2Version = Get-WebView2Version
ConvertTo-Json -InputObject $result -Depth 4 -Compress
"#;

const ACTIVATION: &str = r#"
$windows = $null
$office = @()
$warnings = @()
try {
    $products = @(Get-CimInstance -Namespace 'root/cimv2' -ClassName SoftwareLicensingProduct -ErrorAction Stop)
    $windowsProduct = $products |
        Where-Object { $_.ApplicationID -eq '55c92734-d682-4d71-983e-d6ec3f16059f' } |
        Sort-Object @{ Expression = { $_.LicenseStatus -eq 1 }; Descending = $true } |
        Select-Object -First 1
    if ($null -ne $windowsProduct) {
        $expiresAt = $null
        if ($null -ne $windowsProduct.GracePeriodRemaining -and [int]$windowsProduct.GracePeriodRemaining -gt 0) {
            $expiresAt = Convert-ToIsoDate ((Get-Date).AddMinutes([int]$windowsProduct.GracePeriodRemaining))
        }
        $statusCode = $null
        if ($null -ne $windowsProduct.LicenseStatus) { $statusCode = [int]$windowsProduct.LicenseStatus }
        $windows = [ordered]@{
            name = $windowsProduct.Name
            version = $windowsProduct.Version
            status = Get-LicenseStatus $windowsProduct.LicenseStatus
            statusCode = $statusCode
            licenseChannel = $windowsProduct.ProductKeyChannel
            licenseDescription = $windowsProduct.Description
            expiresAt = $expiresAt
            isDigitalLicense = [bool]([string]$windowsProduct.Description -match '(?i)digital|数字')
        }
    }
    $office = @($products |
        Where-Object { $_.ApplicationID -eq '0ff1ce15-a989-479d-af46-f275c6370663' } |
        ForEach-Object {
            $expiresAt = $null
            if ($null -ne $_.GracePeriodRemaining -and [int]$_.GracePeriodRemaining -gt 0) {
                $expiresAt = Convert-ToIsoDate ((Get-Date).AddMinutes([int]$_.GracePeriodRemaining))
            }
            $statusCode = $null
            if ($null -ne $_.LicenseStatus) { $statusCode = [int]$_.LicenseStatus }
            [pscustomobject][ordered]@{
                name = $_.Name
                status = Get-LicenseStatus $_.LicenseStatus
                statusCode = $statusCode
                licenseChannel = $_.ProductKeyChannel
                licenseDescription = $_.Description
                expiresAt = $expiresAt
            }
        })
} catch {
    $warnings += "授权信息读取失败：$($_.Exception.Message)"
}
ConvertTo-Json -InputObject ([ordered]@{ windows = $windows; office = $office; warnings = $warnings }) -Depth 5 -Compress
"#;

const HARDWARE: &str = r#"
$warnings = @()
$processors = @()
$memoryModules = @()
$graphics = @()
$audio = @()
$batteries = @()
$motherboard = [ordered]@{ manufacturer = $null; product = $null; version = $null; serialNumber = $null }
$bios = [ordered]@{ manufacturer = $null; version = $null; releaseDate = $null; serialNumber = $null }
$memoryTotalBytes = 0
try {
    $processors = @(Get-CimInstance -ClassName Win32_Processor -ErrorAction Stop | ForEach-Object {
        [pscustomobject][ordered]@{
            name = $_.Name
            manufacturer = $_.Manufacturer
            cores = $_.NumberOfCores
            logicalProcessors = $_.NumberOfLogicalProcessors
            maxClockMHz = $_.MaxClockSpeed
        }
    })
} catch { $warnings += "处理器信息读取失败：$($_.Exception.Message)" }
try {
    $item = Get-CimInstance -ClassName Win32_BaseBoard -ErrorAction Stop | Select-Object -First 1
    if ($null -ne $item) {
        $motherboard.manufacturer = $item.Manufacturer
        $motherboard.product = $item.Product
        $motherboard.version = $item.Version
        $motherboard.serialNumber = $item.SerialNumber
    }
} catch { $warnings += "主板信息读取失败：$($_.Exception.Message)" }
try {
    $item = Get-CimInstance -ClassName Win32_BIOS -ErrorAction Stop | Select-Object -First 1
    if ($null -ne $item) {
        $bios.manufacturer = $item.Manufacturer
        $biosVersion = $item.SMBIOSBIOSVersion
        if ([string]::IsNullOrWhiteSpace([string]$biosVersion)) { $biosVersion = $item.Version }
        $bios.version = $biosVersion
        $bios.releaseDate = Convert-ToIsoDate $item.ReleaseDate
        $bios.serialNumber = $item.SerialNumber
    }
} catch { $warnings += "BIOS 信息读取失败：$($_.Exception.Message)" }
try {
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
    if ($null -ne $computer.TotalPhysicalMemory) { $memoryTotalBytes = $computer.TotalPhysicalMemory }
} catch { $warnings += "物理内存总量读取失败：$($_.Exception.Message)" }
try {
    $memoryModules = @(Get-CimInstance -ClassName Win32_PhysicalMemory -ErrorAction Stop | ForEach-Object {
        [pscustomobject][ordered]@{
            capacityBytes = $_.Capacity
            speedMHz = $_.Speed
            manufacturer = $_.Manufacturer
            partNumber = if ($null -eq $_.PartNumber) { $null } else { ([string]$_.PartNumber).Trim() }
            bankLabel = $_.BankLabel
            deviceLocator = $_.DeviceLocator
            serialNumber = $_.SerialNumber
        }
    })
} catch { $warnings += "内存条信息读取失败：$($_.Exception.Message)" }
if ([double]$memoryTotalBytes -eq 0 -and $memoryModules.Count -gt 0) {
    $memoryTotalBytes = ($memoryModules | Measure-Object -Property capacityBytes -Sum).Sum
}
try {
    $graphics = @(Get-CimInstance -ClassName Win32_VideoController -ErrorAction Stop | ForEach-Object {
        $resolution = $null
        if ($null -ne $_.CurrentHorizontalResolution -and $null -ne $_.CurrentVerticalResolution) {
            $resolution = "$($_.CurrentHorizontalResolution) x $($_.CurrentVerticalResolution)"
        }
        [pscustomobject][ordered]@{
            name = $_.Name
            adapterRamBytes = $_.AdapterRAM
            driverVersion = $_.DriverVersion
            videoMode = $_.VideoModeDescription
            resolution = $resolution
        }
    })
} catch { $warnings += "显卡信息读取失败：$($_.Exception.Message)" }
try {
    $audio = @(Get-CimInstance -ClassName Win32_SoundDevice -ErrorAction Stop | ForEach-Object {
        [pscustomobject][ordered]@{ name = $_.Name; manufacturer = $_.Manufacturer; status = $_.Status }
    })
} catch { $warnings += "音频设备信息读取失败：$($_.Exception.Message)" }
try {
    $batteries = @(Get-CimInstance -ClassName Win32_Battery -ErrorAction Stop | ForEach-Object {
        [pscustomobject][ordered]@{
            name = $_.Name
            chargePercent = $_.EstimatedChargeRemaining
            status = $_.Status
            designVoltageMv = $_.DesignVoltage
        }
    })
} catch { $warnings += "电池信息读取失败：$($_.Exception.Message)" }
ConvertTo-Json -InputObject ([ordered]@{
    processors = $processors
    motherboard = $motherboard
    bios = $bios
    memoryTotalBytes = $memoryTotalBytes
    memoryModules = $memoryModules
    graphics = $graphics
    audio = $audio
    batteries = $batteries
    warnings = $warnings
}) -Depth 5 -Compress
"#;

const DRIVERS: &str = r#"
$warnings = @()
$devices = @()
$drivers = @()
$problems = @()
try {
    $devices = @(Get-CimInstance -ClassName Win32_PnPEntity -ErrorAction Stop)
} catch {
    $warnings += "设备信息读取失败：$($_.Exception.Message)"
}
$devicesById = @{}
foreach ($device in $devices) {
    if (-not [string]::IsNullOrWhiteSpace([string]$device.DeviceID)) {
        $devicesById[[string]$device.DeviceID] = $device
    }
}
try {
    $drivers = @(Get-CimInstance -ClassName Win32_PnPSignedDriver -ErrorAction Stop | ForEach-Object {
        $device = $devicesById[[string]$_.DeviceID]
        $problemCode = 0
        if ($null -ne $device -and $null -ne $device.ConfigManagerErrorCode) {
            $problemCode = [int]$device.ConfigManagerErrorCode
        }
        [pscustomobject][ordered]@{
            deviceId = $_.DeviceID
            deviceName = $_.DeviceName
            className = $_.DeviceClass
            manufacturer = $_.Manufacturer
            provider = $_.DriverProviderName
            version = $_.DriverVersion
            driverDate = Convert-ToIsoDate $_.DriverDate
            infName = $_.InfName
            isSigned = $_.IsSigned
            status = $device.Status
            problemCode = $problemCode
        }
    } | Sort-Object deviceName)
} catch {
    $warnings += "驱动清单读取失败：$($_.Exception.Message)"
}
$driversById = @{}
foreach ($driver in $drivers) {
    if (-not [string]::IsNullOrWhiteSpace([string]$driver.deviceId)) {
        $driversById[[string]$driver.deviceId] = $driver
    }
}
$problems = @($devices | Where-Object {
    $null -ne $_.ConfigManagerErrorCode -and [int]$_.ConfigManagerErrorCode -notin @(0, 22, 45)
} | ForEach-Object {
    $driver = $driversById[[string]$_.DeviceID]
    [pscustomobject][ordered]@{
        deviceId = $_.DeviceID
        deviceName = $_.Name
        className = $_.ClassGuid
        manufacturer = $_.Manufacturer
        provider = $driver.provider
        version = $driver.version
        driverDate = $driver.driverDate
        status = $_.Status
        problemCode = [int]$_.ConfigManagerErrorCode
    }
} | Sort-Object problemCode, deviceName)
$missingDriverCount = @($problems | Where-Object { $_.problemCode -eq 28 }).Count
$unsignedDriverCount = @($drivers | Where-Object { $_.isSigned -eq $false -or $_.problemCode -eq 52 }).Count
$summary = [ordered]@{
    totalDevices = $devices.Count
    driverCount = $drivers.Count
    problemCount = $problems.Count
    missingDriverCount = $missingDriverCount
    unsignedDriverCount = $unsignedDriverCount
}
ConvertTo-Json -InputObject ([ordered]@{
    summary = $summary
    problems = $problems
    drivers = $drivers
    updateScan = $null
    warnings = $warnings
}) -Depth 5 -Compress
"#;

pub(crate) const DRIVER_UPDATE_SCAN: &str = r#"
function Get-ComProperty($Object, $Name) {
    if ($null -eq $Object) { return $null }
    try { return $Object.$Name } catch { return $null }
}
function Release-ComObject($Value) {
    if ($null -ne $Value -and [Runtime.InteropServices.Marshal]::IsComObject($Value)) {
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($Value)
    }
}
$session = $null
$searcher = $null
$searchResult = $null
$updates = @()
try {
    $session = New-Object -ComObject Microsoft.Update.Session
    $session.ClientApplicationID = 'Harness Driver Update Scan'
    $searcher = $session.CreateUpdateSearcher()
    $searchResult = $searcher.Search("IsInstalled=0 and Type='Driver' and IsHidden=0")
    for ($index = 0; $index -lt $searchResult.Updates.Count; $index++) {
        $update = $searchResult.Updates.Item($index)
        $identity = Get-ComProperty $update 'Identity'
        $installation = Get-ComProperty $update 'InstallationBehavior'
        $rebootBehavior = Get-ComProperty $installation 'RebootBehavior'
        $rebootRequired = $null
        if ($null -ne $rebootBehavior) { $rebootRequired = [int]$rebootBehavior -gt 0 }
        $kbArticles = @()
        $kbValues = Get-ComProperty $update 'KBArticleIDs'
        if ($null -ne $kbValues) { $kbArticles = @($kbValues | ForEach-Object { [string]$_ }) }
        $hardwareId = Get-ComProperty $update 'HardwareID'
        if ($hardwareId -is [array]) { $hardwareId = $hardwareId -join ', ' }
        $updates += [pscustomobject][ordered]@{
            id = Get-ComProperty $identity 'UpdateID'
            title = Get-ComProperty $update 'Title'
            manufacturer = Get-ComProperty $update 'DriverManufacturer'
            model = Get-ComProperty $update 'DriverModel'
            className = Get-ComProperty $update 'DriverClass'
            driverDate = Convert-ToIsoDate (Get-ComProperty $update 'DriverVerDate')
            deviceId = Get-ComProperty $update 'DeviceID'
            hardwareId = $hardwareId
            isDownloaded = Get-ComProperty $update 'IsDownloaded'
            isMandatory = Get-ComProperty $update 'IsMandatory'
            isOptional = Get-ComProperty $update 'BrowseOnly'
            rebootRequired = $rebootRequired
            kbArticles = $kbArticles
        }
        Release-ComObject $installation
        Release-ComObject $identity
        Release-ComObject $update
    }
} catch {
    throw "Windows 更新驱动扫描失败：$($_.Exception.Message)"
} finally {
    Release-ComObject $searchResult
    Release-ComObject $searcher
    Release-ComObject $session
}
ConvertTo-Json -InputObject ([ordered]@{
    scannedAt = (Get-Date).ToString('o')
    updates = $updates
    warnings = @()
}) -Depth 5 -Compress
"#;

const SOFTWARE: &str = r#"
$warnings = @()
$installed = @()
$paths = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
foreach ($path in $paths) {
    if (-not (Test-Path $path)) { continue }
    try {
        $installed += @(Get-ItemProperty -Path $path -ErrorAction Stop |
            Where-Object {
                -not [string]::IsNullOrWhiteSpace([string]$_.DisplayName) -and
                $_.SystemComponent -ne 1 -and
                [string]::IsNullOrWhiteSpace([string]$_.ParentKeyName) -and
                [string]$_.ReleaseType -notmatch '(?i)Update|Hotfix' -and
                [string]$_.DisplayName -notmatch '(?i)^(Update|Hotfix|Security Update)\b'
            } |
            ForEach-Object {
                [pscustomobject][ordered]@{
                    id = [string]$_.PSChildName
                    name = ([string]$_.DisplayName).Trim()
                    version = $_.DisplayVersion
                    publisher = $_.Publisher
                    installDate = Convert-RegistryInstallDate $_.InstallDate
                }
            })
    } catch {
        $warnings += "软件清单读取失败（$path）：$($_.Exception.Message)"
    }
}
$installed = @($installed | Sort-Object name, version, publisher -Unique)
$definitions = @(
    @{ id = 'microsoft-edge'; name = 'Microsoft Edge'; category = 'Browser'; pattern = '(?i)^Microsoft Edge$' },
    @{ id = 'google-chrome'; name = 'Google Chrome'; category = 'Browser'; pattern = '(?i)^Google Chrome$' },
    @{ id = 'mozilla-firefox'; name = 'Mozilla Firefox'; category = 'Browser'; pattern = '(?i)^Mozilla Firefox' },
    @{ id = 'wechat'; name = '微信'; category = 'Communication'; pattern = '(?i)微信|^WeChat|^Weixin' },
    @{ id = 'qq'; name = 'QQ'; category = 'Communication'; pattern = '(?i)^QQ$|腾讯QQ' },
    @{ id = 'wecom'; name = '企业微信'; category = 'Communication'; pattern = '(?i)企业微信|^WeCom|WXWork' },
    @{ id = 'dingtalk'; name = '钉钉'; category = 'Communication'; pattern = '(?i)钉钉|DingTalk' },
    @{ id = 'feishu'; name = '飞书'; category = 'Communication'; pattern = '(?i)飞书|Feishu|^Lark' },
    @{ id = 'wps'; name = 'WPS Office'; category = 'Office'; pattern = '(?i)WPS Office|WPS 办公软件' },
    @{ id = 'microsoft-office'; name = 'Microsoft Office'; category = 'Office'; pattern = '(?i)^Microsoft Office|^Microsoft 365' },
    @{ id = '7-zip'; name = '7-Zip'; category = 'Compression'; pattern = '(?i)^7-Zip' },
    @{ id = 'winrar'; name = 'WinRAR'; category = 'Compression'; pattern = '(?i)^WinRAR' },
    @{ id = 'potplayer'; name = 'PotPlayer'; category = 'Media'; pattern = '(?i)^PotPlayer' },
    @{ id = 'vlc'; name = 'VLC'; category = 'Media'; pattern = '(?i)^VLC media player|^VLC$' },
    @{ id = 'baidu-netdisk'; name = '百度网盘'; category = 'CloudStorage'; pattern = '(?i)百度网盘|Baidu Netdisk' },
    @{ id = 'tencent-meeting'; name = '腾讯会议'; category = 'Meeting'; pattern = '(?i)腾讯会议|VooV Meeting|Tencent Meeting' }
)
$common = @()
foreach ($definition in $definitions) {
    $found = $installed |
        Where-Object { $_.name -match $definition.pattern } |
        Sort-Object @{ Expression = { $_.name.Length } } |
        Select-Object -First 1
    $version = $null
    $publisher = $null
    if ($null -ne $found) {
        $version = $found.version
        $publisher = $found.publisher
    }
    $common += [pscustomobject][ordered]@{
        id = $definition.id
        name = $definition.name
        category = $definition.category
        installed = ($null -ne $found)
        version = $version
        publisher = $publisher
    }
}
ConvertTo-Json -InputObject ([ordered]@{ common = $common; installed = $installed; warnings = $warnings }) -Depth 5 -Compress
"#;

const STORAGE: &str = r#"
$warnings = @()
$volumes = @()
$physicalDisks = @()
try {
    $volumes = @(Get-Volume -ErrorAction Stop |
        Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_.DriveLetter) } |
        Sort-Object DriveLetter |
        ForEach-Object {
            $label = $_.FileSystemLabel
            if ([string]::IsNullOrWhiteSpace([string]$label)) { $label = $null }
            $driveType = switch ([string]$_.DriveType) {
                'Removable' { 2 }
                'Fixed' { 3 }
                'Network' { 4 }
                'CD-ROM' { 5 }
                'RAM' { 6 }
                default { 0 }
            }
            [pscustomobject][ordered]@{
                driveLetter = "$($_.DriveLetter):"
                name = $label
                fileSystem = $_.FileSystem
                sizeBytes = $_.Size
                freeBytes = $_.SizeRemaining
                driveType = $driveType
                healthStatus = [string]$_.HealthStatus
            }
        })
} catch { $warnings += "卷信息读取失败：$($_.Exception.Message)" }
try {
    $physicalDisks = @(Get-PhysicalDisk -ErrorAction Stop | ForEach-Object {
        $model = $_.FriendlyName
        if (-not [string]::IsNullOrWhiteSpace([string]$_.Model)) { $model = $_.Model }
        [pscustomobject][ordered]@{
            model = $model
            mediaType = [string]$_.MediaType
            busType = [string]$_.BusType
            sizeBytes = $_.Size
            healthStatus = [string]$_.HealthStatus
            operationalStatus = [string]($_.OperationalStatus -join ', ')
            serialNumber = $_.SerialNumber
            firmwareVersion = $_.FirmwareVersion
        }
    })
} catch {
    $warnings += "物理磁盘信息读取失败：$($_.Exception.Message)"
    try {
        $physicalDisks = @(Get-CimInstance -ClassName Win32_DiskDrive -ErrorAction Stop | ForEach-Object {
            [pscustomobject][ordered]@{
                model = $_.Model
                mediaType = $_.MediaType
                busType = $_.InterfaceType
                sizeBytes = $_.Size
                healthStatus = $_.Status
                operationalStatus = $_.Status
                serialNumber = $_.SerialNumber
                firmwareVersion = $_.FirmwareRevision
            }
        })
    } catch { $warnings += "物理磁盘备用信息读取失败：$($_.Exception.Message)" }
}
ConvertTo-Json -InputObject ([ordered]@{ volumes = $volumes; physicalDisks = $physicalDisks; warnings = $warnings }) -Depth 5 -Compress
"#;

const NETWORK: &str = r#"
$warnings = @()
$profiles = @()
$adapters = @()
$isConnected = $false
try {
    $connectionProfiles = @(Get-NetConnectionProfile -ErrorAction Stop)
    $profiles = @($connectionProfiles | ForEach-Object {
        [pscustomobject][ordered]@{
            name = $_.Name
            interfaceAlias = $_.InterfaceAlias
            networkCategory = [string]$_.NetworkCategory
            ipv4Connectivity = [string]$_.IPv4Connectivity
        }
    })
    $isConnected = @($connectionProfiles | Where-Object {
        [string]$_.IPv4Connectivity -eq 'Internet'
    }).Count -gt 0
} catch { $warnings += "网络配置文件读取失败：$($_.Exception.Message)" }
try {
    $networkAdapters = @(Get-NetAdapter -ErrorAction Stop)
    foreach ($adapter in $networkAdapters) {
        $ipAddresses = @()
        $gateways = @()
        $dnsServers = @()
        try {
            $ipAddresses = @(Get-NetIPAddress -InterfaceAlias $adapter.Name -AddressFamily IPv4 -ErrorAction Stop |
                Where-Object { $_.AddressState -eq 'Preferred' -and -not [string]::IsNullOrWhiteSpace([string]$_.IPAddress) } |
                Select-Object -ExpandProperty IPAddress -Unique)
        } catch { $warnings += "网卡 $($adapter.Name) 的 IP 地址读取失败：$($_.Exception.Message)" }
        try {
            $gateways = @(Get-NetRoute -InterfaceAlias $adapter.Name -ErrorAction Stop |
                Where-Object {
                    $_.DestinationPrefix -in @('0.0.0.0/0', '::/0') -and
                    $_.NextHop -notin @('0.0.0.0', '::') -and
                    -not [string]::IsNullOrWhiteSpace([string]$_.NextHop)
                } |
                Select-Object -ExpandProperty NextHop -Unique)
        } catch { $warnings += "网卡 $($adapter.Name) 的网关读取失败：$($_.Exception.Message)" }
        try {
            $dnsServers = @(Get-DnsClientServerAddress -InterfaceAlias $adapter.Name -ErrorAction Stop |
                Select-Object -ExpandProperty ServerAddresses -Unique)
        } catch { $warnings += "网卡 $($adapter.Name) 的 DNS 读取失败：$($_.Exception.Message)" }
        $adapters += [pscustomobject][ordered]@{
            name = $adapter.Name
            description = $adapter.InterfaceDescription
            status = [string]$adapter.Status
            linkSpeed = [string]$adapter.LinkSpeed
            macAddress = $adapter.MacAddress
            ipAddresses = $ipAddresses
            gateways = $gateways
            dnsServers = $dnsServers
        }
    }
} catch { $warnings += "网卡信息读取失败：$($_.Exception.Message)" }
ConvertTo-Json -InputObject ([ordered]@{
    isConnected = [bool]$isConnected
    profiles = $profiles
    adapters = $adapters
    warnings = $warnings
}) -Depth 5 -Compress
"#;

const SECURITY: &str = r#"
$warnings = @()
$defender = $null
$firewallProfiles = @()
$uacEnabled = $null
$tpmResult = $null
$secureBoot = $null
$lastUpdate = $null
$pendingReboot = $null
try {
    $status = Get-MpComputerStatus -ErrorAction Stop
    $service = Get-Service -Name 'WinDefend' -ErrorAction SilentlyContinue
    $serviceEnabled = [bool]$status.AMServiceEnabled
    if ($null -ne $service) { $serviceEnabled = $service.Status -eq 'Running' }
    $defender = [ordered]@{
        serviceEnabled = [bool]$serviceEnabled
        antivirusEnabled = [bool]$status.AntivirusEnabled
        realTimeProtectionEnabled = [bool]$status.RealTimeProtectionEnabled
        tamperProtected = [bool]$status.IsTamperProtected
        signatureLastUpdated = Convert-ToIsoDate $status.AntivirusSignatureLastUpdated
    }
} catch { $warnings += "Defender 状态读取失败：$($_.Exception.Message)" }
try {
    $firewallProfiles = @(Get-NetFirewallProfile -ErrorAction Stop | ForEach-Object {
        [pscustomobject][ordered]@{ name = $_.Name; enabled = [bool]$_.Enabled }
    })
} catch { $warnings += "防火墙状态读取失败：$($_.Exception.Message)" }
try {
    $uac = Get-ItemPropertyValue -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' -Name 'EnableLUA' -ErrorAction Stop
    $uacEnabled = [bool]([int]$uac)
} catch { $warnings += "UAC 状态读取失败：$($_.Exception.Message)" }
try {
    $tpm = Get-Tpm -ErrorAction Stop
    $specVersion = $null
    if ($tpm.PSObject.Properties['SpecVersion']) { $specVersion = [string]$tpm.SpecVersion }
    if ([string]::IsNullOrWhiteSpace([string]$specVersion)) {
        try {
            $tpmCim = Get-CimInstance -Namespace 'root/cimv2/security/microsofttpm' -ClassName Win32_Tpm -ErrorAction Stop | Select-Object -First 1
            if ($null -ne $tpmCim) { $specVersion = $tpmCim.SpecVersion }
        } catch {}
    }
    $tpmResult = [ordered]@{
        present = [bool]$tpm.TpmPresent
        ready = [bool]$tpm.TpmReady
        enabled = [bool]$tpm.TpmEnabled
        activated = [bool]$tpm.TpmActivated
        specVersion = $specVersion
    }
} catch { $warnings += "TPM 状态读取失败：$($_.Exception.Message)" }
try {
    $enabled = Confirm-SecureBootUEFI -ErrorAction Stop
    $secureBoot = [ordered]@{ supported = $true; enabled = [bool]$enabled }
} catch {
    $secureBootError = $_.Exception.Message
    try {
        $enabled = Get-ItemPropertyValue -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\State' -Name 'UEFISecureBootEnabled' -ErrorAction Stop
        $secureBoot = [ordered]@{ supported = $true; enabled = [bool]([int]$enabled) }
    } catch {
        $unsupported = $secureBootError -match '(?i)not supported|不支持|BIOS'
        $secureBoot = [ordered]@{ supported = (-not $unsupported); enabled = $null }
        $warnings += "Secure Boot 状态读取失败：$secureBootError"
    }
}
try {
    $hotfix = Get-HotFix -ErrorAction Stop | Sort-Object InstalledOn -Descending | Select-Object -First 1
    if ($null -ne $hotfix) {
        $lastUpdate = [ordered]@{
            hotfixId = $hotfix.HotFixID
            installedOn = Convert-ToIsoDate $hotfix.InstalledOn
            description = $hotfix.Description
        }
    }
} catch { $warnings += "最近更新读取失败：$($_.Exception.Message)" }
try {
    $pendingReboot =
        (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') -or
        (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')
    if (-not $pendingReboot) {
        $pendingRename = Get-ItemPropertyValue -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -Name 'PendingFileRenameOperations' -ErrorAction SilentlyContinue
        $pendingReboot = $null -ne $pendingRename -and $pendingRename.Count -gt 0
    }
} catch { $warnings += "待重启状态读取失败：$($_.Exception.Message)" }
ConvertTo-Json -InputObject ([ordered]@{
    defender = $defender
    firewallProfiles = $firewallProfiles
    uacEnabled = $uacEnabled
    tpm = $tpmResult
    secureBoot = $secureBoot
    lastUpdate = $lastUpdate
    pendingReboot = $pendingReboot
    warnings = $warnings
}) -Depth 5 -Compress
"#;

pub(crate) fn get(section: &str) -> Option<&'static str> {
    match section {
        "basic" => Some(BASIC),
        "activation" => Some(ACTIVATION),
        "hardware" => Some(HARDWARE),
        "drivers" => Some(DRIVERS),
        "software" => Some(SOFTWARE),
        "storage" => Some(STORAGE),
        "network" => Some(NETWORK),
        "security" => Some(SECURITY),
        _ => None,
    }
}
