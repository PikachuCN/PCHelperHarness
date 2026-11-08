use super::explain::quoted_target;
use super::RiskLevel;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub(crate) struct RuleMatch {
    pub level: RiskLevel,
    pub rule_id: String,
    pub reason: String,
    pub requires_typed_confirmation: bool,
}

impl RuleMatch {
    pub(crate) fn new(
        level: RiskLevel,
        rule_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            level,
            rule_id: rule_id.into(),
            reason: reason.into(),
            requires_typed_confirmation: false,
        }
    }

    fn confirmed(mut self) -> Self {
        self.requires_typed_confirmation = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellKind {
    Cmd,
    PowerShell,
    Wsl,
    Python,
    Unknown,
}

impl ShellKind {
    fn parse(shell: &str) -> Self {
        let normalized = shell
            .trim()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(shell)
            .to_ascii_lowercase();
        match normalized.as_str() {
            "cmd" | "cmd.exe" | "command" | "bat" => Self::Cmd,
            "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" | "ps" => Self::PowerShell,
            "wsl" | "wsl.exe" | "bash" | "sh" | "linux" => Self::Wsl,
            "python" | "python.exe" | "python3" | "py" | "py.exe" => Self::Python,
            _ => Self::Unknown,
        }
    }
}

static ENCODED_COMMAND: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:^|\s)-(?:e|en|enc|enco|encod|encode|encoded|encodedc|encodedco|encodedcom|encodedcomm|encodedcomma|encodedcomman|encodedcommand)(?:\s|=|:|$)")
        .expect("valid EncodedCommand regex")
});
static POWERSHELL_IEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z0-9_])(?:iex|invoke-expression)(?:$|[^a-z0-9_])")
        .expect("valid Invoke-Expression regex")
});
static BASE64_DECODE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:frombase64string|base64\s*\.\s*b64decode|\bb64decode\s*\(|(?:^|\s)base64\s+(?:--decode|-d)(?:\s|$)|certutil(?:\.exe)?[^\r\n]*(?:-decode|-decodehex)|(?:base64|b64)[^\r\n]{0,80}(?:decode|encoding|iex|invoke))")
        .expect("valid Base64 regex")
});
static SCRIPTBLOCK_DYNAMIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\[scriptblock\]\s*::\s*create").expect("valid ScriptBlock regex")
});

pub(crate) fn classify_command(shell: &str, command: &str) -> Vec<RuleMatch> {
    let kind = ShellKind::parse(shell);
    if command.trim().is_empty() {
        return vec![unknown("empty.command", "空命令")];
    }
    if kind == ShellKind::Unknown {
        return vec![RuleMatch::new(
            RiskLevel::L3,
            "shell.unsupported_plugin",
            format!(
                "插件 Shell「{}」的脚本语法无法审计，可能执行任意系统修改，必须按 L3 审批。",
                shell.trim()
            ),
        )];
    }

    // 编码与动态执行直接判为 L3，避免拆分后把载荷误当成普通文本。
    if let Some(finding) = detect_obfuscation(kind, command) {
        return vec![finding];
    }

    let segments = split_compound(command);
    if segments.is_empty() {
        return vec![unknown("empty.command", "空命令")];
    }

    segments
        .iter()
        .map(|segment| classify_segment(kind, segment))
        .collect()
}

fn detect_obfuscation(kind: ShellKind, command: &str) -> Option<RuleMatch> {
    let lower = command.to_ascii_lowercase();
    let powershell_context =
        kind == ShellKind::PowerShell || lower.contains("powershell") || lower.contains("pwsh");

    if powershell_context && ENCODED_COMMAND.is_match(command) {
        return Some(RuleMatch::new(
            RiskLevel::L3,
            "obfuscation.powershell_encoded_command",
            "检测到 PowerShell EncodedCommand（含 -enc/-e 等缩写），实际命令被编码隐藏，直接按 L3 处理。",
        ));
    }
    if powershell_context && POWERSHELL_IEX.is_match(command) {
        return Some(RuleMatch::new(
            RiskLevel::L3,
            "obfuscation.invoke_expression",
            "检测到 iex/Invoke-Expression 动态执行，字符串内容可作为命令运行且容易隐藏真实行为，直接按 L3 处理。",
        ));
    }
    if BASE64_DECODE.is_match(command) {
        return Some(RuleMatch::new(
            RiskLevel::L3,
            "obfuscation.base64_decode",
            "检测到 Base64 解码或编码字符串拼接，解码后的内容无法可靠静态审计，直接按 L3 处理。",
        ));
    }
    if powershell_context && SCRIPTBLOCK_DYNAMIC.is_match(command) {
        return Some(RuleMatch::new(
            RiskLevel::L3,
            "obfuscation.dynamic_scriptblock",
            "检测到动态创建 ScriptBlock，字符串会被转换为可执行代码，直接按 L3 处理。",
        ));
    }
    None
}

fn split_compound(command: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut quote = None;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '`' {
            current.push(ch);
            escaped = true;
            continue;
        }
        if ch == '\\' && matches!(chars.peek(), Some(';' | '|' | '&' | '\n' | '\r')) {
            current.push(ch);
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            current.push(ch);
            if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            current.push(ch);
            continue;
        }

        let is_redirection_ampersand =
            ch == '&' && (current.trim_end().ends_with('>') || chars.peek() == Some(&'>'));
        if matches!(ch, ';' | '\n' | '\r')
            || ((ch == '|' || ch == '&') && !is_redirection_ampersand)
        {
            push_segment(&mut result, &mut current);
            if matches!(ch, '|' | '&') && chars.peek() == Some(&ch) {
                chars.next();
            }
        } else {
            current.push(ch);
        }
    }
    push_segment(&mut result, &mut current);
    result
}

fn push_segment(result: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        result.push(trimmed.to_string());
    }
    current.clear();
}

fn tokenize(segment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = segment.chars().peekable();
    let mut quote = None;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '`' {
            escaped = true;
            continue;
        }
        if ch == '\\' && quote == Some('"') && matches!(chars.peek(), Some('"' | '\\')) {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn classify_segment(kind: ShellKind, segment: &str) -> RuleMatch {
    let tokens = tokenize(segment);
    if tokens.is_empty() {
        return unknown("empty.command", "空命令片段");
    }

    if let Some(mut finding) = classify_wrapper(kind, &tokens) {
        if has_output_redirection(segment) && finding.level == RiskLevel::L0 {
            finding = output_redirection(segment);
        }
        return finding;
    }

    let mut finding = match kind {
        ShellKind::Cmd => classify_cmd(&tokens),
        ShellKind::PowerShell => classify_powershell(&tokens),
        ShellKind::Wsl => classify_wsl(&tokens),
        ShellKind::Python => classify_python(segment),
        ShellKind::Unknown => RuleMatch::new(
            RiskLevel::L3,
            "shell.unsupported_plugin",
            "插件 Shell 的脚本语法无法审计，必须按 L3 审批。",
        ),
    };

    if has_output_redirection(segment) && finding.level == RiskLevel::L0 {
        finding = output_redirection(segment);
    }
    if finding.level == RiskLevel::L1
        && (finding.rule_id.contains("file_write")
            || matches!(
                finding.rule_id.as_str(),
                "file.write"
                    | "shell.output_redirection"
                    | "file.create"
                    | "file.copy"
                    | "file.in_place_edit"
                    | "registry.export"
                    | "cmd.attribute_change"
                    | "cmd.sort_output"
            ))
        && contains_critical_path(segment)
    {
        finding = RuleMatch::new(
            RiskLevel::L3,
            "system.critical_path_write",
            format!(
                "命令将写入系统或磁盘关键路径{}，可能影响系统启动或稳定性。",
                quoted_target(first_operand(&tokens[1..], false))
            ),
        );
        if is_raw_disk_path(segment) {
            finding = RuleMatch::new(
                RiskLevel::L3,
                "disk.raw_device_write",
                "命令可能直接写入物理磁盘或块设备，可能破坏分区与全部数据，必须逐字输入确认。",
            )
            .confirmed();
        }
    }
    finding
}

fn classify_wrapper(kind: ShellKind, tokens: &[String]) -> Option<RuleMatch> {
    let name = command_name(&tokens[0]);
    let args = &tokens[1..];

    if matches!(
        name.as_str(),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
    ) {
        if let Some(index) = option_index(args, &["command", "c"]) {
            if let Some(nested) = args.get(index + 1) {
                return Some(highest_nested("powershell", nested));
            }
        }
        if option_index(args, &["file", "f"]).is_some() {
            return Some(unknown(
                "script.external_file",
                "将执行外部 PowerShell 脚本，但未提供脚本内容，无法按只读命令放行。",
            ));
        }
        return Some(unknown("shell.nested_unknown", tokens[0].as_str()));
    }

    if matches!(name.as_str(), "cmd" | "cmd.exe") {
        if let Some(index) = args.iter().position(|arg| {
            let value = arg.to_ascii_lowercase();
            value == "/c" || value == "/k"
        }) {
            if let Some(nested) = args.get(index + 1) {
                return Some(highest_nested("cmd", nested));
            }
        }
        return Some(unknown("shell.nested_unknown", tokens[0].as_str()));
    }

    if matches!(name.as_str(), "wsl" | "wsl.exe") {
        let mut index = 0;
        while index < args.len() {
            let arg = args[index].to_ascii_lowercase();
            if matches!(arg.as_str(), "-e" | "--exec") {
                index += 1;
                break;
            }
            if matches!(
                arg.as_str(),
                "-d" | "--distribution" | "-u" | "--user" | "--cd"
            ) {
                index += 2;
            } else if arg.starts_with('-') {
                index += 1;
            } else {
                break;
            }
        }
        if index < args.len() {
            let nested = args[index..].join(" ");
            return Some(highest_nested("wsl", &nested));
        }
        return Some(unknown("shell.nested_unknown", tokens[0].as_str()));
    }

    if matches!(
        name.as_str(),
        "python" | "python.exe" | "python3" | "py" | "py.exe"
    ) {
        if let Some(index) = option_index(args, &["c"]) {
            if let Some(nested) = args.get(index + 1) {
                return Some(highest_nested("python", nested));
            }
        }
        if args.first().map(String::as_str) == Some("-m")
            && args.get(1).map(|value| value.eq_ignore_ascii_case("pip")) == Some(true)
        {
            return Some(classify_package_manager("pip", &args[2..]));
        }
        return Some(RuleMatch::new(
            RiskLevel::L1,
            "script.external_file",
            format!(
                "将运行 Python 脚本或模块{}，未提供完整代码，按 L1 处理。",
                quoted_target(args.first().map(String::as_str))
            ),
        ));
    }

    if kind == ShellKind::Wsl && matches!(name.as_str(), "bash" | "sh" | "dash" | "zsh") {
        if let Some(index) = args
            .iter()
            .position(|arg| matches!(arg.as_str(), "-c" | "-lc" | "-ec"))
        {
            if let Some(nested) = args.get(index + 1) {
                return Some(highest_nested("wsl", nested));
            }
        }
        return Some(unknown("shell.nested_unknown", tokens[0].as_str()));
    }

    if kind == ShellKind::Wsl && matches!(name.as_str(), "sudo" | "doas" | "pkexec") {
        let nested = args
            .iter()
            .skip_while(|arg| arg.starts_with('-'))
            .cloned()
            .collect::<Vec<_>>();
        if nested.is_empty() {
            return Some(unknown("shell.nested_unknown", tokens[0].as_str()));
        }
        return Some(classify_segment(ShellKind::Wsl, &nested.join(" ")));
    }

    None
}

fn highest_nested(shell: &str, command: &str) -> RuleMatch {
    classify_command(shell, command)
        .into_iter()
        .reduce(|best, candidate| {
            if candidate.level > best.level
                || (candidate.level == best.level && candidate.requires_typed_confirmation)
            {
                candidate
            } else {
                best
            }
        })
        .unwrap_or_else(|| unknown("empty.command", "嵌套命令为空"))
}

fn classify_powershell(tokens: &[String]) -> RuleMatch {
    let name = command_name(&tokens[0]);
    let args = &tokens[1..];
    let target = powershell_target(args);

    match name.as_str() {
        "get-childitem"
        | "gci"
        | "ls"
        | "dir"
        | "get-content"
        | "gc"
        | "cat"
        | "type"
        | "get-location"
        | "gl"
        | "pwd"
        | "get-process"
        | "gps"
        | "ps"
        | "get-service"
        | "gsv"
        | "get-date"
        | "get-host"
        | "get-help"
        | "help"
        | "get-command"
        | "gcm"
        | "get-member"
        | "gm"
        | "get-item"
        | "gi"
        | "get-itemproperty"
        | "gp"
        | "get-itempropertyvalue"
        | "get-variable"
        | "gv"
        | "get-history"
        | "h"
        | "history"
        | "get-computerinfo"
        | "get-volume"
        | "get-disk"
        | "get-partition"
        | "get-filehash"
        | "test-path"
        | "resolve-path"
        | "select-object"
        | "select"
        | "where-object"
        | "where"
        | "sort-object"
        | "sort"
        | "format-table"
        | "ft"
        | "format-list"
        | "fl"
        | "format-wide"
        | "fw"
        | "out-string"
        | "measure-object"
        | "measure"
        | "compare-object"
        | "compare"
        | "diff"
        | "write-output"
        | "echo"
        | "write-host"
        | "write-warning"
        | "write-verbose"
        | "write-debug"
        | "write-information"
        | "out-default"
        | "out-host"
        | "get-executionpolicy"
        | "get-winevent"
        | "get-eventlog"
        | "get-ciminstance"
        | "get-wmiobject"
        | "get-netipaddress"
        | "get-netroute"
        | "get-dnsclientcache"
        | "test-netconnection"
        | "resolve-dnsname"
        | "get-acl"
        | "get-authenticodesignature"
        | "get-windowsfeature"
        | "get-windowsoptionalfeature"
        | "get-appxpackage"
        | "import-csv"
        | "import-clixml"
        | "convertto-json"
        | "convertfrom-json"
        | "set-location"
        | "sl"
        | "cd" => read_only("powershell.read_whitelist", &tokens[0]),

        "remove-item" | "rm" | "ri" | "del" | "erase" | "rmdir" | "rd" => {
            file_delete("powershell.remove_item", target, args, true)
        }
        "remove-itemproperty" | "remove-itempropertyvalue" | "rp" => RuleMatch::new(
            RiskLevel::L3,
            "registry.delete",
            format!(
                "将删除注册表或属性值{}，相关软件或系统配置可能失效。",
                quoted_target(target)
            ),
        ),
        "clear-content" | "clc" => RuleMatch::new(
            RiskLevel::L3,
            "file.clear_content",
            format!(
                "将清空文件{}的内容，原数据可能无法恢复。",
                quoted_target(target)
            ),
        ),
        "stop-process" | "kill" | "spps" => RuleMatch::new(
            RiskLevel::L1,
            "process.stop",
            format!(
                "将终止进程{}，可能造成未保存数据丢失或关键服务中断。",
                quoted_target(target)
            ),
        ),
        "stop-service" | "spsv" | "remove-service" => RuleMatch::new(
            RiskLevel::L3,
            "service.stop_or_remove",
            format!(
                "将停止或删除服务{}，可能导致系统功能或依赖服务不可用。",
                quoted_target(target)
            ),
        ),
        "restart-computer" | "stop-computer" => RuleMatch::new(
            RiskLevel::L3,
            "system.restart_or_shutdown",
            "将重启或关闭 Windows，可能中断任务并造成未保存数据丢失。",
        ),
        "clear-eventlog" | "remove-eventlog" | "clear-recyclebin" => RuleMatch::new(
            RiskLevel::L3,
            "system.destructive_clear",
            format!(
                "将清除日志或回收站项目{}，被清除的信息通常无法恢复。",
                quoted_target(target)
            ),
        ),
        "clear-disk" => RuleMatch::new(
            RiskLevel::L3,
            "disk.clear_disk",
            format!(
                "将清除磁盘{}上的分区与数据，可能造成整盘数据不可恢复，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "initialize-disk" => RuleMatch::new(
            RiskLevel::L3,
            "disk.initialize_disk",
            format!(
                "将初始化磁盘{}并重写分区结构，现有数据可能无法访问，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "format-volume" | "format" => RuleMatch::new(
            RiskLevel::L3,
            "disk.format_volume",
            format!(
                "将格式化磁盘卷{}，卷内文件将被清除，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "remove-partition" | "new-partition" | "resize-partition" | "set-partition" => {
            RuleMatch::new(
                RiskLevel::L3,
                "disk.partition_change",
                format!(
                    "将删除、创建或调整磁盘分区{}，可能破坏分区表与数据，必须逐字输入确认。",
                    quoted_target(target)
                ),
            )
            .confirmed()
        }
        "invoke-webrequest" | "iwr" | "curl" | "wget" | "start-bitstransfer" => {
            network_download(&tokens[0], args)
        }
        "install-package"
        | "install-module"
        | "install-script"
        | "install-packageprovider"
        | "update-module"
        | "update-package" => package_install("PowerShell 包管理器", target),
        "uninstall-package" | "uninstall-module" | "remove-appxpackage" => RuleMatch::new(
            RiskLevel::L3,
            "package.remove",
            format!(
                "将卸载或移除软件包{}，其功能与依赖它的软件可能不可用。",
                quoted_target(target)
            ),
        ),
        "set-content" | "sc" | "add-content" | "ac" | "out-file" | "new-item" | "ni"
        | "copy-item" | "cpi" | "cp" | "copy" | "move-item" | "mi" | "move" | "rename-item"
        | "rni" | "ren" | "set-item" | "si" | "set-itemproperty" | "sp" | "new-itemproperty"
        | "export-csv" | "export-clixml" | "tee-object" | "tee" | "new-alias" | "set-alias" => {
            file_write("powershell.file_write", target)
        }
        "start-process" | "saps" | "start" => {
            if let Some(nested) = classify_start_process(args) {
                nested
            } else {
                RuleMatch::new(
                    RiskLevel::L1,
                    "process.start",
                    format!(
                        "将启动进程{}，其实际副作用取决于目标程序与参数。",
                        quoted_target(target)
                    ),
                )
            }
        }
        "invoke-command" | "icm" | "enter-pssession" => unknown(
            "process.remote_or_nested_execution",
            "将执行嵌套或远程命令，当前静态规则无法完整确认脚本块行为。",
        ),
        "set-executionpolicy"
        | "set-service"
        | "start-service"
        | "sasv"
        | "restart-service"
        | "reservice"
        | "enable-windowsoptionalfeature"
        | "disable-windowsoptionalfeature"
        | "set-netipaddress"
        | "new-netipaddress"
        | "set-dnsclientserveraddress"
        | "netsh"
        | "icacls"
        | "set-acl" => system_change("powershell.system_configuration", target),
        "takeown" => classify_takeown(args, target),
        "set-disk" | "set-volume" | "repair-volume" | "optimize-volume" => {
            system_change("disk.non_destructive_maintenance", target)
        }
        _ => unknown("powershell.unknown", &tokens[0]),
    }
}

fn classify_cmd(tokens: &[String]) -> RuleMatch {
    let name = command_name(&tokens[0]);
    let args = &tokens[1..];
    let target = first_operand(args, true);

    if args.iter().any(|arg| arg == "/?")
        && matches!(
            name.as_str(),
            "dir"
                | "type"
                | "del"
                | "erase"
                | "rd"
                | "rmdir"
                | "format"
                | "diskpart"
                | "bcdedit"
                | "reg"
                | "sc"
                | "taskkill"
                | "shutdown"
                | "copy"
                | "xcopy"
                | "robocopy"
                | "icacls"
                | "takeown"
        )
    {
        return read_only("cmd.help", &tokens[0]);
    }

    match name.as_str() {
        "dir" | "type" | "more" | "find" | "findstr" | "ver" | "vol" | "whoami"
        | "hostname" | "ipconfig" | "systeminfo" | "tasklist" | "cd" | "chdir" | "pwd"
        | "echo" | "where" | "driverquery" | "getmac" | "netstat" | "arp" | "route"
        | "tracert" | "pathping" | "nslookup" | "doskey" | "cls" | "help" => {
            read_only("cmd.read_whitelist", &tokens[0])
        }
        "date" | "time" if args.iter().any(|arg| arg.eq_ignore_ascii_case("/t")) => {
            read_only("cmd.read_whitelist", &tokens[0])
        }
        "set" | "path" => {
            if args.iter().any(|arg| arg.contains('=')) {
                RuleMatch::new(
                    RiskLevel::L1,
                    "process.environment_change",
                    "将修改当前命令进程的的环境变量；虽未直接写盘，也不属于严格只读操作。",
                )
            } else {
                read_only("cmd.environment_read", &tokens[0])
            }
        }
        "sort" => {
            if args.iter().any(|arg| arg.eq_ignore_ascii_case("/o")) {
                file_write("cmd.sort_output", target)
            } else {
                read_only("cmd.read_whitelist", &tokens[0])
            }
        }
        "attrib" => {
            if args.iter().any(|arg| arg.starts_with('+') || arg.starts_with('-')) {
                file_write("cmd.attribute_change", target)
            } else {
                read_only("cmd.attribute_read", &tokens[0])
            }
        }
        "del" | "erase" | "rd" | "rmdir" => file_delete("cmd.file_delete", target, args, true),
        "format" => RuleMatch::new(
            RiskLevel::L3,
            "disk.format",
            format!(
                "将对磁盘或卷{}执行格式化，目标内数据将被清除，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "diskpart" => RuleMatch::new(
            RiskLevel::L3,
            "disk.diskpart",
            format!(
                "diskpart 可通过脚本{}清理磁盘、删除分区或格式化卷；即使未提供完整脚本也按磁盘级 L3 处理，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "bcdedit" => classify_bcdedit(args),
        "reg" => classify_registry(args),
        "sc" => classify_sc(args),
        "net" => classify_net(args),
        "taskkill" => RuleMatch::new(
            RiskLevel::L1,
            "process.stop",
            format!(
                "将强制或按条件终止进程{}，可能造成未保存数据丢失或服务中断。",
                quoted_target(target)
            ),
        ),
        "shutdown" => RuleMatch::new(
            RiskLevel::L3,
            "system.restart_or_shutdown",
            "将关闭或重启 Windows，可能中断正在进行的任务并造成数据丢失。",
        ),
        "md" | "mkdir" | "copy" | "move" | "ren" | "rename" | "type nul" => {
            file_write("cmd.file_write", target)
        }
        "xcopy" | "robocopy" => {
            if args.iter().any(|arg| {
                let value = arg.to_ascii_lowercase();
                matches!(value.as_str(), "/mir" | "/purge" | "/move" | "/mov")
            }) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "file.sync_delete",
                    format!(
                        "复制/同步参数包含镜像、清除或移动{}，可能删除源或目标目录中的文件。",
                        quoted_target(target)
                    ),
                )
            } else {
                file_write("file.copy", target)
            }
        }
        "icacls" | "cacls" => {
            if args.iter().any(|arg| {
                let value = arg.to_ascii_lowercase();
                value.starts_with("/grant")
                    || value.starts_with("/deny")
                    || value.starts_with("/remove")
                    || value.starts_with("/setowner")
                    || value == "/reset"
            }) {
                system_change("system.permission_change", target)
            } else {
                read_only("cmd.acl_read", &tokens[0])
            }
        }
        "takeown" => classify_takeown(args, target),
        "netsh" => system_change("system.permission_or_network_change", target),
        "curl" | "curl.exe" | "wget" | "wget.exe" | "bitsadmin" => {
            if name.starts_with("bitsadmin")
                && args.first().map(|arg| arg.eq_ignore_ascii_case("/list")) == Some(true)
            {
                read_only("cmd.bits_list", &tokens[0])
            } else {
                network_download(&tokens[0], args)
            }
        }
        "certutil" | "certutil.exe" => {
            if args.iter().any(|arg| arg.eq_ignore_ascii_case("-urlcache")) {
                network_download(&tokens[0], args)
            } else if args.iter().any(|arg| arg.eq_ignore_ascii_case("-delstore")) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "system.certificate_delete",
                    format!("将删除证书存储或证书{}，可能破坏加密、签名或安全连接。", quoted_target(target)),
                )
            } else if args.iter().any(|arg| {
                matches!(arg.to_ascii_lowercase().as_str(), "-verify" | "-dump" | "-store" | "-user")
            }) {
                read_only("cmd.certificate_read", &tokens[0])
            } else {
                unknown("cmd.certutil_unknown", &tokens[0])
            }
        }
        "winget" | "winget.exe" | "choco" | "choco.exe" | "pip" | "pip3" | "npm" => {
            classify_package_manager(&name, args)
        }
        "msiexec" | "msiexec.exe" => {
            if args.iter().any(|arg| arg.eq_ignore_ascii_case("/x")) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "package.remove",
                    format!("将通过 Windows Installer 卸载软件{}，相关功能将不可用。", quoted_target(target)),
                )
            } else {
                package_install("Windows Installer", target)
            }
        }
        "regsvr32" | "rundll32" => system_change("system.binary_registration", target),
        "pnputil" | "pnputil.exe" => {
            if args.iter().any(|arg| arg.to_ascii_lowercase().starts_with("/delete-driver")) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "system.driver_delete",
                    format!("将删除驱动程序包{}，对应硬件可能停止工作。", quoted_target(target)),
                )
            } else if args.iter().any(|arg| arg.eq_ignore_ascii_case("/enum-drivers")) {
                read_only("cmd.driver_read", &tokens[0])
            } else {
                system_change("system.driver_change", target)
            }
        }
        "dism" | "dism.exe" => {
            if args.iter().any(|arg| arg.to_ascii_lowercase().starts_with("/remove")) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "system.component_remove",
                    "将移除 Windows 组件或功能，可能破坏依赖它的系统能力。",
                )
            } else if args.iter().any(|arg| arg.to_ascii_lowercase().starts_with("/get")) {
                read_only("cmd.dism_read", &tokens[0])
            } else {
                system_change("system.image_change", target)
            }
        }
        "sfc" => {
            if args.iter().any(|arg| arg.eq_ignore_ascii_case("/verifyonly")) {
                read_only("cmd.sfc_verify", &tokens[0])
            } else {
                system_change("system.file_repair", target)
            }
        }
        "chkdsk" => {
            if args.iter().any(|arg| {
                let value = arg.to_ascii_lowercase();
                value == "/f" || value == "/r" || value == "/x"
            }) {
                system_change("disk.filesystem_repair", target)
            } else {
                read_only("cmd.disk_check_read", &tokens[0])
            }
        }
        "defrag" | "cleanmgr" | "cipher" => {
            if name == "cipher" && args.iter().any(|arg| arg.to_ascii_lowercase().starts_with("/w")) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "disk.free_space_wipe",
                    format!("将擦除卷{}的可用空间，已删除数据将无法恢复，必须逐字输入确认。", quoted_target(target)),
                )
                .confirmed()
            } else if name == "cleanmgr" {
                RuleMatch::new(
                    RiskLevel::L3,
                    "file.cleanup_delete",
                    "磁盘清理会删除临时文件、更新缓存或回收站内容，部分数据无法恢复。",
                )
            } else {
                system_change("disk.maintenance", target)
            }
        }
        _ => unknown("cmd.unknown", &tokens[0]),
    }
}

fn classify_wsl(tokens: &[String]) -> RuleMatch {
    let name = command_name(&tokens[0]);
    let args = &tokens[1..];
    let target = first_operand(args, false);

    match name.as_str() {
        "ls" | "pwd" | "cat" | "head" | "tail" | "less" | "more" | "file" | "stat" | "du"
        | "df" | "free" | "uname" | "whoami" | "id" | "hostname" | "date" | "uptime" | "ps"
        | "top" | "htop" | "env" | "printenv" | "which" | "whereis" | "type" | "echo"
        | "printf" | "grep" | "egrep" | "fgrep" | "rg" | "sort" | "uniq" | "wc" | "cut" | "tr"
        | "diff" | "cmp" | "ss" | "netstat" | "lscpu" | "lsblk" | "lspci" | "lsusb" | "dmesg"
        | "readlink" | "realpath" | "basename" | "dirname" | "getent" | "lsmod" | "modinfo"
        | "getfacl" | "true" | "false" => read_only("wsl.read_whitelist", &tokens[0]),
        "find" => {
            if args.iter().any(|arg| arg == "-delete") {
                RuleMatch::new(
                    RiskLevel::L3,
                    "file.find_delete",
                    format!(
                        "find 的 -delete 会删除匹配路径{}，数据可能无法恢复。",
                        quoted_target(target)
                    ),
                )
            } else if let Some(index) = args
                .iter()
                .position(|arg| arg == "-exec" || arg == "-execdir")
            {
                if index + 1 < args.len() {
                    let nested = args[index + 1..]
                        .iter()
                        .take_while(|arg| arg.as_str() != "+" && arg.as_str() != "\\;")
                        .cloned()
                        .collect::<Vec<_>>();
                    if !nested.is_empty() {
                        return classify_segment(ShellKind::Wsl, &nested.join(" "));
                    }
                }
                read_only("wsl.find_read", &tokens[0])
            } else {
                read_only("wsl.find_read", &tokens[0])
            }
        }
        "sed" => {
            if args.iter().any(|arg| arg == "-i" || arg.starts_with("-i")) {
                file_write("file.in_place_edit", target)
            } else {
                read_only("wsl.stream_read", &tokens[0])
            }
        }
        "awk" | "gawk" | "mawk" => {
            if tokens
                .iter()
                .any(|token| token.to_ascii_lowercase().contains("system("))
            {
                unknown(
                    "wsl.awk_system",
                    "awk 的 system() 可执行任意 Shell 命令，无法按只读操作放行。",
                )
            } else {
                read_only("wsl.stream_read", &tokens[0])
            }
        }
        "xargs" => {
            let nested = args
                .iter()
                .skip_while(|arg| arg.starts_with('-') || arg.contains('='))
                .cloned()
                .collect::<Vec<_>>();
            if nested.is_empty() {
                read_only("wsl.xargs_echo", &tokens[0])
            } else {
                classify_segment(ShellKind::Wsl, &nested.join(" "))
            }
        }
        "rm" | "rmdir" | "shred" | "unlink" => file_delete("wsl.file_delete", target, args, true),
        "truncate" => RuleMatch::new(
            RiskLevel::L3,
            "file.truncate",
            format!(
                "将截断文件{}，可能立即清除文件内容且无法恢复。",
                quoted_target(target)
            ),
        ),
        "mkfs" | "mkswap" | "wipefs" | "fdisk" | "sfdisk" | "parted" | "gdisk" | "sgdisk" => {
            RuleMatch::new(
                RiskLevel::L3,
                "disk.partition_or_filesystem_change",
                format!(
                    "磁盘工具 {} 将修改或擦除设备{}的分区/文件系统，必须逐字输入确认。",
                    name,
                    quoted_target(target)
                ),
            )
            .confirmed()
        }
        n if n.starts_with("mkfs.") => RuleMatch::new(
            RiskLevel::L3,
            "disk.mkfs",
            format!(
                "将在设备{}上创建文件系统并覆盖现有数据，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "dd" => RuleMatch::new(
            RiskLevel::L3,
            "disk.raw_copy",
            format!(
                "dd 将按原始块复制数据{}；错误的 if/of 参数可覆盖整盘，必须逐字输入确认。",
                quoted_target(target)
            ),
        )
        .confirmed(),
        "kill" | "killall" | "pkill" => RuleMatch::new(
            RiskLevel::L3,
            "process.stop",
            format!(
                "将终止进程{}，可能造成未保存数据丢失或服务中断。",
                quoted_target(target)
            ),
        ),
        "shutdown" | "reboot" | "poweroff" | "halt" => RuleMatch::new(
            RiskLevel::L3,
            "system.restart_or_shutdown",
            "将关闭或重启系统，可能中断任务并造成未保存数据丢失。",
        ),
        "apt" | "apt-get" | "aptitude" | "dpkg" | "pip" | "pip3" | "npm" | "yarn" | "pnpm"
        | "snap" | "dnf" | "yum" | "pacman" => classify_package_manager(&name, args),
        "curl" | "wget" | "aria2c" => network_download(&tokens[0], args),
        "git" => classify_git(args),
        "systemctl" | "service" => classify_linux_service(&name, args),
        "journalctl" => {
            if args.iter().any(|arg| arg.starts_with("--vacuum")) {
                RuleMatch::new(
                    RiskLevel::L3,
                    "system.log_delete",
                    "将清理 systemd 日志，历史诊断信息会被永久删除。",
                )
            } else {
                read_only("wsl.journal_read", &tokens[0])
            }
        }
        "chmod" | "chown" | "chgrp" | "chattr" | "setfacl" | "mount" | "umount" | "useradd"
        | "usermod" | "groupadd" | "passwd" | "timedatectl" | "hostnamectl" => {
            system_change("system.linux_configuration", target)
        }
        "userdel" | "groupdel" => RuleMatch::new(
            RiskLevel::L3,
            "system.account_delete",
            format!(
                "将删除账户或用户组{}，关联权限与数据访问可能丢失。",
                quoted_target(target)
            ),
        ),
        "cp" | "mv" | "touch" | "mkdir" | "ln" | "install" | "tee" | "patch" => {
            file_write("file.write", target)
        }
        "eval" => RuleMatch::new(
            RiskLevel::L3,
            "obfuscation.shell_eval",
            "eval 会把字符串再次作为 Shell 命令执行，真实行为可被变量与拼接隐藏，直接按 L3 处理。",
        ),
        "docker" | "podman" => classify_container(&name, args),
        _ => unknown("wsl.unknown", &tokens[0]),
    }
}

fn classify_python(segment: &str) -> RuleMatch {
    let lower = segment.to_ascii_lowercase();
    let compact: String = lower.chars().filter(|ch| !ch.is_whitespace()).collect();
    let destructive = [
        ("os.remove(", "os.remove"),
        ("os.unlink(", "os.unlink"),
        ("os.rmdir(", "os.rmdir"),
        ("os.removedirs(", "os.removedirs"),
        ("shutil.rmtree(", "shutil.rmtree"),
        (".unlink(", "Path.unlink"),
        (".rmtree(", "Path.rmtree"),
        (".rmdir(", "Path.rmdir"),
    ];
    for (pattern, api) in destructive {
        if compact.contains(pattern) {
            let argument = extract_call_argument(segment, pattern.trim_end_matches('('));
            return RuleMatch::new(
                RiskLevel::L3,
                "python.file_delete",
                format!(
                    "Python 调用 {} 将删除文件或目录{}，数据可能无法恢复。",
                    api,
                    quoted_target(argument.as_deref())
                ),
            );
        }
    }

    if compact.contains("eval(")
        || compact.contains("exec(")
        || compact.contains("execfile(")
        || compact.contains("__import__(") && compact.contains("decode")
    {
        return RuleMatch::new(
            RiskLevel::L3,
            "obfuscation.python_dynamic_execution",
            "Python eval/exec 会动态执行字符串代码，可能隐藏拼接或解码后的真实行为，直接按 L3 处理。",
        );
    }

    if compact.contains("os.system(")
        || compact.contains("subprocess.run(")
        || compact.contains("subprocess.popen(")
        || compact.contains("subprocess.call(")
        || compact.contains("subprocess.check_output(")
    {
        if compact.contains("rm-rf")
            || compact.contains("remove-item")
            || compact.contains("formatc:")
            || compact.contains("diskpart")
            || compact.contains("shutdown")
        {
            return RuleMatch::new(
                RiskLevel::L3,
                "python.nested_destructive_command",
                "Python 将启动包含删除、格式化或关机指令的子进程，可能破坏数据或中断系统。",
            );
        }
        return unknown(
            "python.subprocess",
            "Python 将启动子进程，但未对嵌套命令做完整静态审计，按 L1 处理。",
        );
    }

    let network_apis = [
        "requests.get(",
        "requests.post(",
        "requests.put(",
        "httpx.get(",
        "httpx.post(",
        "urllib.request.urlopen(",
        "urllib.request.urlretrieve(",
        "urlretrieve(",
        "wget.download(",
        "socket.create_connection(",
    ];
    if network_apis.iter().any(|api| compact.contains(api)) {
        let argument = network_apis
            .iter()
            .find_map(|api| extract_call_argument(segment, api.trim_end_matches('(')));
        return RuleMatch::new(
            RiskLevel::L2,
            "network.python_request",
            format!(
                "Python 将访问网络地址{}，可能下载外部内容或发送本地数据，请核对来源与参数。",
                quoted_target(argument.as_deref())
            ),
        );
    }

    if (compact.contains("open(")
        && (compact.contains("\"w\"")
            || compact.contains("'w'")
            || compact.contains("\"a\"")
            || compact.contains("'a'")
            || compact.contains("\"x\"")
            || compact.contains("'x'")
            || compact.contains("\"wb\"")
            || compact.contains("'wb'")
            || compact.contains("\"ab\"")
            || compact.contains("'ab'")))
        || compact.contains(".write_text(")
        || compact.contains(".write_bytes(")
        || compact.contains("os.mkdir(")
        || compact.contains("os.makedirs(")
        || compact.contains("shutil.copy")
        || compact.contains("shutil.move(")
    {
        return RuleMatch::new(
            RiskLevel::L1,
            "python.file_write",
            format!(
                "Python 将创建、覆盖或移动本地文件{}，请核对路径与写入模式。",
                quoted_target(None)
            ),
        )
        .into_critical_path_if_needed(segment);
    }

    if compact.starts_with("print(")
        || compact == "sys.version"
        || compact == "sys.platform"
        || compact == "os.getcwd()"
        || compact.starts_with("os.listdir(")
        || compact.starts_with("os.scandir(")
        || compact.contains(".read_text(")
        || compact.contains(".read_bytes(")
        || compact.contains(".exists()")
        || compact == "importos"
        || compact == "importsys"
        || compact == "importjson"
        || compact == "importpathlib"
    {
        return read_only("python.read_whitelist", "Python 只读表达式");
    }

    unknown("python.unknown", segment)
}

impl RuleMatch {
    fn into_critical_path_if_needed(self, segment: &str) -> Self {
        if self.level <= RiskLevel::L2 && contains_critical_path(segment) {
            Self::new(
                RiskLevel::L3,
                "system.critical_path_write",
                "Python 写入目标涉及系统或磁盘关键路径，可能破坏系统配置或稳定性。",
            )
        } else {
            self
        }
    }
}

fn classify_bcdedit(args: &[String]) -> RuleMatch {
    if args.is_empty()
        || args.iter().any(|arg| arg.eq_ignore_ascii_case("/enum"))
        || args.iter().any(|arg| arg == "/?")
    {
        return read_only("boot.bcdedit_read", "bcdedit /enum");
    }
    if args.iter().any(|arg| {
        let value = arg.to_ascii_lowercase();
        value == "/delete" || value == "/deletevalue"
    }) {
        return RuleMatch::new(
            RiskLevel::L3,
            "boot.delete",
            format!(
                "将删除 Windows 启动配置项或启动值{}，可能导致系统无法启动，必须逐字输入确认。",
                quoted_target(first_operand(args, true))
            ),
        )
        .confirmed();
    }
    RuleMatch::new(
        RiskLevel::L3,
        "boot.configuration_change",
        "将修改 Windows 启动配置，错误参数可能导致系统无法启动。",
    )
}

fn classify_registry(args: &[String]) -> RuleMatch {
    let Some(action) = args.first().map(|value| value.to_ascii_lowercase()) else {
        return unknown("cmd.registry_unknown", "reg");
    };
    let target = args.get(1).map(String::as_str);
    match action.as_str() {
        "query" | "compare" => read_only("registry.read", "reg query"),
        "delete" => RuleMatch::new(
            RiskLevel::L3,
            "registry.delete",
            format!(
                "将删除注册表项或值{}，可能导致 Windows 或软件无法正常工作。",
                quoted_target(target)
            ),
        ),
        "add" | "import" | "copy" | "restore" => system_change("registry.change", target),
        "export" | "save" => file_write("registry.export", target),
        _ => unknown("cmd.registry_unknown", action.as_str()),
    }
}

fn classify_sc(args: &[String]) -> RuleMatch {
    let action = args.first().map(|value| value.to_ascii_lowercase());
    let target = args.get(1).map(String::as_str);
    match action.as_deref() {
        Some(
            "query" | "queryex" | "qc" | "qdescription" | "qfailure" | "qtriggerinfo" | "enum",
        ) => read_only("service.read", "sc query"),
        Some("stop" | "delete") => RuleMatch::new(
            RiskLevel::L3,
            "service.stop_or_remove",
            format!(
                "将停止或删除 Windows 服务{}，可能导致系统功能不可用。",
                quoted_target(target)
            ),
        ),
        Some("start" | "config" | "create" | "failure" | "description") => {
            system_change("service.configuration", target)
        }
        _ => unknown("service.unknown", "sc"),
    }
}

fn classify_net(args: &[String]) -> RuleMatch {
    let lower = args
        .iter()
        .map(|arg| arg.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if lower.iter().any(|arg| arg == "/delete") {
        return RuleMatch::new(
            RiskLevel::L3,
            "system.account_or_share_delete",
            format!(
                "net 命令将删除账户、组或共享{}，访问权限与相关功能可能失效。",
                quoted_target(args.first().map(String::as_str))
            ),
        );
    }
    if lower.iter().any(|arg| arg == "/add")
        || matches!(lower.first().map(String::as_str), Some("start" | "config"))
    {
        return system_change(
            "system.account_or_service_change",
            args.get(1).map(String::as_str),
        );
    }
    if matches!(lower.first().map(String::as_str), Some("stop")) {
        return RuleMatch::new(
            RiskLevel::L3,
            "service.stop_or_remove",
            format!(
                "将停止 Windows 服务{}，依赖它的功能可能中断。",
                quoted_target(args.get(1).map(String::as_str))
            ),
        );
    }
    if matches!(
        lower.first().map(String::as_str),
        Some("user" | "localgroup" | "view" | "share" | "session" | "statistics" | "accounts")
    ) {
        return read_only("cmd.net_read", "net 查询");
    }
    unknown("cmd.net_unknown", "net")
}

fn classify_package_manager(manager: &str, args: &[String]) -> RuleMatch {
    let subcommand_index = args
        .iter()
        .position(|arg| !arg.starts_with('-') && !arg.starts_with('/') && !arg.contains('='));
    let action = subcommand_index
        .and_then(|index| args.get(index))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| {
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "--version" | "-v"))
            {
                "--version".to_string()
            } else {
                String::new()
            }
        });
    let package = subcommand_index.and_then(|index| {
        args[index + 1..]
            .iter()
            .find(|arg| !arg.starts_with('-') && !arg.starts_with('/'))
            .map(String::as_str)
    });

    match action.as_str() {
        "list" | "ls" | "show" | "search" | "info" | "freeze" | "policy" | "status"
        | "--version" | "-v" | "installed" | "outdated" => read_only("package.read", manager),
        "install" | "add" | "upgrade" | "update" | "full-upgrade" | "dist-upgrade" | "-i"
        | "--install" | "sync" | "-s" | "-syu" => package_install(manager, package),
        "download" => network_download(manager, args),
        "remove" | "uninstall" | "purge" | "erase" | "autoremove" | "-r" | "-p" | "--remove"
        | "--purge" | "-rns" => RuleMatch::new(
            RiskLevel::L3,
            "package.remove",
            format!(
                "将通过 {} 卸载或清除软件包{}，依赖它的功能可能不可用。",
                manager,
                quoted_target(package)
            ),
        ),
        _ => RuleMatch::new(
            RiskLevel::L1,
            "package.unknown",
            format!(
                "{} {}",
                manager,
                if action.is_empty() {
                    "（缺少子命令）"
                } else {
                    &action
                }
            ),
        ),
    }
}

fn classify_git(args: &[String]) -> RuleMatch {
    let action = args.first().map(|value| value.to_ascii_lowercase());
    match action.as_deref() {
        Some(
            "status" | "log" | "diff" | "show" | "rev-parse" | "ls-files" | "blame" | "shortlog",
        ) => read_only("git.read", "git 查询"),
        Some("clone" | "fetch" | "pull") => network_download("git", args),
        Some("clean" | "rm") => RuleMatch::new(
            RiskLevel::L3,
            "file.git_delete",
            "git clean/rm 会删除工作区或索引中的文件，未提交内容可能无法恢复。",
        ),
        Some("reset") if args.iter().any(|arg| arg == "--hard") => RuleMatch::new(
            RiskLevel::L3,
            "file.git_reset_hard",
            "git reset --hard 会丢弃工作区与暂存区修改，未提交内容可能无法恢复。",
        ),
        _ => unknown("git.unknown", "git"),
    }
}

fn classify_linux_service(manager: &str, args: &[String]) -> RuleMatch {
    let action = args.first().map(|value| value.to_ascii_lowercase());
    let target = args.get(1).map(String::as_str);
    match action.as_deref() {
        Some(
            "status" | "is-active" | "is-enabled" | "list-units" | "list-services" | "show" | "cat",
        ) => read_only("service.read", manager),
        Some("stop" | "kill") => RuleMatch::new(
            RiskLevel::L3,
            "service.stop_or_remove",
            format!(
                "将停止或终止服务{}，可能导致系统功能中断。",
                quoted_target(target)
            ),
        ),
        Some("start" | "restart" | "enable" | "disable" | "mask" | "unmask" | "daemon-reload") => {
            system_change("service.configuration", target)
        }
        _ => unknown("service.unknown", manager),
    }
}

fn classify_container(manager: &str, args: &[String]) -> RuleMatch {
    let action = args.first().map(|value| value.to_ascii_lowercase());
    let target = args.get(1).map(String::as_str);
    match action.as_deref() {
        Some("ps" | "images" | "inspect" | "logs" | "stats" | "version" | "info") => {
            read_only("container.read", manager)
        }
        Some("pull") => network_download(manager, args),
        Some("rm" | "rmi" | "prune" | "kill" | "stop") => RuleMatch::new(
            RiskLevel::L3,
            "container.destructive",
            format!(
                "将停止或删除容器/镜像{}，容器内未持久化数据可能丢失。",
                quoted_target(target)
            ),
        ),
        _ => unknown("container.unknown", manager),
    }
}

fn classify_start_process(args: &[String]) -> Option<RuleMatch> {
    let arglist_index = option_index(args, &["argumentlist", "args"])?;
    let executable = args[..arglist_index]
        .iter()
        .find(|arg| !arg.starts_with('-'))?
        .clone();
    let mut nested_tokens = vec![executable];
    for value in &args[arglist_index + 1..] {
        nested_tokens.extend(
            value
                .split(',')
                .filter(|part| !part.is_empty())
                .map(str::to_string),
        );
    }
    classify_wrapper(ShellKind::PowerShell, &nested_tokens)
}

fn read_only(rule_id: &str, command: &str) -> RuleMatch {
    RuleMatch::new(
        RiskLevel::L0,
        rule_id,
        format!(
            "{} 属于严格只读白名单，仅查询信息或输出文本，不主动修改系统。",
            quoted_target(Some(command))
        ),
    )
}

fn unknown(rule_id: &str, command: &str) -> RuleMatch {
    RuleMatch::new(
        RiskLevel::L1,
        rule_id,
        format!(
            "命令{}不在严格只读白名单内，或其参数影响无法完整确认，按未知风险 L1 处理。",
            quoted_target(Some(command))
        ),
    )
}

fn file_write(rule_id: &str, target: Option<&str>) -> RuleMatch {
    RuleMatch::new(
        RiskLevel::L1,
        rule_id,
        format!(
            "将创建、写入、移动或覆盖本地文件{}，请核对目标路径与覆盖范围。",
            quoted_target(target)
        ),
    )
}

fn file_delete(
    rule_id: &str,
    target: Option<&str>,
    args: &[String],
    powershell: bool,
) -> RuleMatch {
    let recursive = args.iter().any(|arg| {
        let value = arg.to_ascii_lowercase();
        value == "-recurse" || value == "-r" || value == "-rf" || value == "-fr" || value == "/s"
    });
    let force = args.iter().any(|arg| {
        let value = arg.to_ascii_lowercase();
        value == "-force"
            || value == "-f"
            || value == "-rf"
            || value == "-fr"
            || value == "/f"
            || value == "/q"
    });
    let critical = target.map(is_critical_target).unwrap_or(false);
    let mut detail = String::new();
    if recursive {
        detail.push_str("，包含递归删除参数");
    }
    if force {
        detail.push_str("，包含强制/静默参数");
    }
    if critical {
        detail.push_str("，且目标属于系统关键路径，可能导致系统无法启动");
    }
    let shell_label = if powershell {
        "文件/目录删除命令"
    } else {
        "删除命令"
    };
    RuleMatch::new(
        RiskLevel::L3,
        rule_id,
        format!(
            "{}将删除{}{}，数据可能无法恢复。",
            shell_label,
            quoted_target(target),
            detail
        ),
    )
}

fn network_download(command: &str, args: &[String]) -> RuleMatch {
    let url = args.iter().find_map(|arg| {
        let value = arg.trim_matches(|ch| ch == '\'' || ch == '"');
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("ftp://")
        {
            Some(value)
        } else {
            None
        }
    });
    RuleMatch::new(
        RiskLevel::L2,
        "network.download",
        format!(
            "{} 将下载或传输网络内容{}，请核验 URL 来源、保存路径及后续执行风险。",
            quoted_target(Some(command)),
            quoted_target(url)
        ),
    )
}

fn package_install(manager: &str, package: Option<&str>) -> RuleMatch {
    RuleMatch::new(
        RiskLevel::L2,
        "package.install",
        format!(
            "将通过 {} 安装、更新或获取软件包{}，可能修改系统并引入外部代码，请核对来源。",
            manager,
            quoted_target(package)
        ),
    )
}

fn classify_takeown(args: &[String], target: Option<&str>) -> RuleMatch {
    if args.iter().any(|arg| arg.eq_ignore_ascii_case("/r")) {
        RuleMatch::new(
            RiskLevel::L3,
            "system.takeown_recursive",
            format!(
                "将递归接管{}的所有权，可能破坏原有权限与访问控制。",
                quoted_target(target)
            ),
        )
    } else {
        system_change("system.permission_or_network_change", target)
    }
}

fn system_change(rule_id: &str, target: Option<&str>) -> RuleMatch {
    RuleMatch::new(
        RiskLevel::L1,
        rule_id,
        format!(
            "将修改系统、服务、权限、引导或磁盘配置{}，请核对参数与恢复方案。",
            quoted_target(target)
        ),
    )
}

fn output_redirection(segment: &str) -> RuleMatch {
    let critical = contains_critical_path(segment);
    RuleMatch::new(
        if critical {
            RiskLevel::L3
        } else {
            RiskLevel::L1
        },
        if critical {
            "system.critical_path_write"
        } else {
            "shell.output_redirection"
        },
        if contains_critical_path(segment) {
            "输出重定向将覆盖系统关键路径中的文件，可能影响 Windows 启动或稳定性。"
        } else {
            "命令包含输出重定向，会创建或覆盖文件，因此不能按只读操作放行。"
        },
    )
}

fn command_name(token: &str) -> String {
    let name = token
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .to_ascii_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

fn option_index(args: &[String], names: &[&str]) -> Option<usize> {
    args.iter().position(|arg| {
        let normalized = arg.trim_start_matches('-').to_ascii_lowercase();
        names.iter().any(|name| normalized == *name)
    })
}

fn option_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    for (index, arg) in args.iter().enumerate() {
        let trimmed = arg.trim_start_matches('-');
        let lower = trimmed.to_ascii_lowercase();
        for name in names {
            if lower == *name {
                if let Some(value) = args.get(index + 1) {
                    return Some(value.as_str());
                }
            }
            for separator in [':', '='] {
                if let Some((key, value)) = trimmed.split_once(separator) {
                    if key.eq_ignore_ascii_case(name) && !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

fn powershell_target(args: &[String]) -> Option<&str> {
    option_value(
        args,
        &[
            "literalpath",
            "path",
            "uri",
            "url",
            "outfile",
            "filepath",
            "number",
            "id",
            "name",
        ],
    )
    .or_else(|| first_operand(args, false))
}

fn first_operand(args: &[String], slash_options: bool) -> Option<&str> {
    args.iter()
        .find(|arg| {
            if arg.as_str() == "--" {
                return false;
            }
            !(arg.starts_with('-') || (slash_options && arg.starts_with('/')))
        })
        .map(String::as_str)
}

fn has_output_redirection(segment: &str) -> bool {
    let mut chars = segment.chars().peekable();
    let mut quote = None;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '`' || ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if ch == '>' && chars.peek() != Some(&'&') {
            // 2>&1 只是把 stderr 合并到 stdout，不会创建或覆盖文件。
            return true;
        }
    }
    false
}

fn contains_critical_path(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    let normalized = lower.replace('/', "\\");
    [
        "c:\\windows",
        "%systemroot%",
        "$env:systemroot",
        "\\windows\\system32",
        "c:\\boot",
        "c:\\program files",
        "c:\\users\\default",
        "c:\\users\\public",
        "\\boot\\",
        "\\etc\\",
        "\\dev\\sd",
        "\\dev\\nvme",
        "\\dev\\disk",
        "physicaldrive",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

fn is_critical_target(target: &str) -> bool {
    contains_critical_path(target)
}

fn is_raw_disk_path(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase().replace('/', "\\");
    lower.contains("physicaldrive")
        || lower.contains("\\dev\\sd")
        || lower.contains("\\dev\\nvme")
        || lower.contains("\\dev\\disk")
}

fn extract_call_argument(segment: &str, api: &str) -> Option<String> {
    let compact_api: String = api.chars().filter(|ch| !ch.is_whitespace()).collect();
    let compact: String = segment.chars().filter(|ch| !ch.is_whitespace()).collect();
    let start = compact
        .to_ascii_lowercase()
        .find(&compact_api.to_ascii_lowercase())?;
    let after_api = &compact[start + compact_api.len()..];
    let open = after_api.find('(')?;
    let args = &after_api[open + 1..];
    let end = args.find([')', ',']).unwrap_or(args.len());
    let value = args[..end]
        .trim()
        .trim_matches(|ch| ch == '\'' || ch == '"')
        .to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
pub(crate) fn split_for_test(command: &str) -> Vec<String> {
    split_compound(command)
}
