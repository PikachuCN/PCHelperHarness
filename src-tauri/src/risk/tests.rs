use super::rules::split_for_test;
use super::{classify, RiskAssessment, RiskLevel};

fn check(shell: &str, command: &str, expected: RiskLevel) -> RiskAssessment {
    let assessment = classify(shell, command, None);
    assert_eq!(
        assessment.level, expected,
        "shell={shell}, command={command}, reasons={:?}",
        assessment.reasons
    );
    assessment
}

#[test]
fn risk_levels_are_ordered() {
    assert!(RiskLevel::L0 < RiskLevel::L1);
    assert!(RiskLevel::L1 < RiskLevel::L2);
    assert!(RiskLevel::L2 < RiskLevel::L3);
}

#[test]
fn risk_level_max_selects_highest() {
    assert_eq!(RiskLevel::L1.max(RiskLevel::L3), RiskLevel::L3);
}

#[test]
fn risk_level_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&RiskLevel::L0).unwrap(), "\"l0\"");
    assert_eq!(serde_json::to_string(&RiskLevel::L3).unwrap(), "\"l3\"");
}

#[test]
fn risk_level_deserializes() {
    let level: RiskLevel = serde_json::from_str("\"l2\"").unwrap();
    assert_eq!(level, RiskLevel::L2);
}

#[test]
fn assessment_is_serializable() {
    let assessment = check("cmd", "dir", RiskLevel::L0);
    let json = serde_json::to_string(&assessment).unwrap();
    assert!(json.contains("summaryZh"));
    assert!(json.contains("requiresTypedConfirmation"));
    let decoded: RiskAssessment = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, assessment);
}

#[test]
fn empty_command_is_l1() {
    check("cmd", "", RiskLevel::L1);
}

#[test]
fn whitespace_command_is_l1() {
    check("powershell", "  \n\t ", RiskLevel::L1);
}

#[test]
fn unknown_cmd_is_l1() {
    check("cmd", "mystery-tool --scan", RiskLevel::L1);
}

#[test]
fn unknown_powershell_is_l1() {
    check("powershell", "Frobnicate-Computer", RiskLevel::L1);
}

#[test]
fn unknown_wsl_is_l1() {
    check("wsl", "mystery --verbose", RiskLevel::L1);
}

#[test]
fn unknown_python_is_l1() {
    check("python", "custom_module.run()", RiskLevel::L1);
}

#[test]
fn unsupported_plugin_shell_requires_l3() {
    check("zsh-unknown", "dir", RiskLevel::L3);
    check(
        "node",
        "require('fs').rmSync('D:/important', { recursive: true })",
        RiskLevel::L3,
    );
}

#[test]
fn cmd_dir_is_l0() {
    check("cmd", "dir", RiskLevel::L0);
}

#[test]
fn cmd_dir_system32_is_l0() {
    check("cmd", r"dir C:\Windows\System32 /a", RiskLevel::L0);
}

#[test]
fn cmd_type_is_l0() {
    check("cmd", r"type C:\Windows\win.ini", RiskLevel::L0);
}

#[test]
fn cmd_echo_is_l0() {
    check("cmd", "echo hello", RiskLevel::L0);
}

#[test]
fn cmd_whoami_is_l0() {
    check("cmd", "whoami /groups", RiskLevel::L0);
}

#[test]
fn cmd_ipconfig_is_l0() {
    check("cmd", "ipconfig /all", RiskLevel::L0);
}

#[test]
fn cmd_systeminfo_is_l0() {
    check("cmd", "systeminfo", RiskLevel::L0);
}

#[test]
fn cmd_tasklist_is_l0() {
    check("cmd", "tasklist /v", RiskLevel::L0);
}

#[test]
fn cmd_reg_query_is_l0() {
    check("cmd", r"reg query HKLM\SOFTWARE /s", RiskLevel::L0);
}

#[test]
fn cmd_sc_query_is_l0() {
    check("cmd", "sc query wuauserv", RiskLevel::L0);
}

#[test]
fn cmd_bcdedit_enum_is_l0() {
    check("cmd", "bcdedit /enum all", RiskLevel::L0);
}

#[test]
fn cmd_chkdsk_without_repair_is_l0() {
    check("cmd", "chkdsk C:", RiskLevel::L0);
}

#[test]
fn cmd_attrib_read_is_l0() {
    check("cmd", r"attrib C:\Windows\win.ini", RiskLevel::L0);
}

#[test]
fn cmd_set_without_assignment_is_l0() {
    check("cmd", "set", RiskLevel::L0);
}

#[test]
fn cmd_date_t_is_l0() {
    check("cmd", "date /t", RiskLevel::L0);
}

#[test]
fn cmd_format_help_is_l0() {
    check("cmd", "format /?", RiskLevel::L0);
}

#[test]
fn powershell_get_childitem_is_l0() {
    check("powershell", "Get-ChildItem", RiskLevel::L0);
}

#[test]
fn powershell_is_case_insensitive() {
    check("powershell", "gEt-ChIlDiTeM C:\\", RiskLevel::L0);
}

#[test]
fn powershell_gci_alias_is_l0() {
    check("powershell", "gci -Force", RiskLevel::L0);
}

#[test]
fn powershell_ls_alias_is_l0() {
    check("powershell", "ls /", RiskLevel::L0);
}

#[test]
fn powershell_dir_alias_is_l0() {
    check("powershell", "dir", RiskLevel::L0);
}

#[test]
fn powershell_get_content_is_l0() {
    check(
        "powershell",
        "Get-Content .\\log.txt -Tail 20",
        RiskLevel::L0,
    );
}

#[test]
fn powershell_gc_alias_is_l0() {
    check("powershell", "gc .\\log.txt", RiskLevel::L0);
}

#[test]
fn powershell_get_process_is_l0() {
    check("powershell", "Get-Process", RiskLevel::L0);
}

#[test]
fn powershell_gps_alias_is_l0() {
    check("powershell", "gps explorer", RiskLevel::L0);
}

#[test]
fn powershell_get_service_is_l0() {
    check("powershell", "Get-Service wuauserv", RiskLevel::L0);
}

#[test]
fn powershell_get_disk_is_l0_not_destructive() {
    let result = check("powershell", "Get-Disk", RiskLevel::L0);
    assert!(!result.requires_typed_confirmation);
}

#[test]
fn powershell_get_partition_is_l0() {
    check("powershell", "Get-Partition", RiskLevel::L0);
}

#[test]
fn powershell_get_volume_is_l0() {
    check("powershell", "Get-Volume", RiskLevel::L0);
}

#[test]
fn powershell_get_filehash_is_l0() {
    check("powershell", "Get-FileHash .\\setup.exe", RiskLevel::L0);
}

#[test]
fn powershell_test_path_is_l0() {
    check("powershell", "Test-Path C:\\Windows", RiskLevel::L0);
}

#[test]
fn powershell_read_pipeline_select_is_l0() {
    check(
        "powershell",
        "Get-ChildItem | Select-Object Name,Length",
        RiskLevel::L0,
    );
}

#[test]
fn powershell_read_pipeline_where_is_l0() {
    check(
        "powershell",
        "Get-Process | Where-Object { $_.Name -eq 'explorer' }",
        RiskLevel::L0,
    );
}

#[test]
fn powershell_read_pipeline_format_is_l0() {
    check("powershell", "Get-Service | Format-List", RiskLevel::L0);
}

#[test]
fn powershell_write_output_is_l0() {
    check("powershell", "Write-Output 'Remove-Item'", RiskLevel::L0);
}

#[test]
fn powershell_help_for_delete_is_still_l0() {
    check("powershell", "Get-Help Remove-Item", RiskLevel::L0);
}

#[test]
fn powershell_get_item_is_l0() {
    check("powershell", "Get-Item .\\file.txt", RiskLevel::L0);
}

#[test]
fn powershell_get_acl_is_l0() {
    check("powershell", "Get-Acl C:\\Windows", RiskLevel::L0);
}

#[test]
fn powershell_dns_query_is_l0() {
    check("powershell", "Resolve-DnsName example.com", RiskLevel::L0);
}

#[test]
fn invoke_webrequest_is_l2() {
    check(
        "powershell",
        "Invoke-WebRequest -Uri https://example.com/a.zip",
        RiskLevel::L2,
    );
}

#[test]
fn iwr_alias_is_l2() {
    check("powershell", "iwr http://example.com/a.exe", RiskLevel::L2);
}

#[test]
fn powershell_curl_alias_is_l2() {
    check(
        "powershell",
        "curl https://example.com -OutFile a.zip",
        RiskLevel::L2,
    );
}

#[test]
fn powershell_wget_alias_is_l2() {
    check(
        "powershell",
        "wget https://example.com/a.zip",
        RiskLevel::L2,
    );
}

#[test]
fn cmd_curl_is_l2() {
    check(
        "cmd",
        "curl.exe -L https://example.com/a.zip -o a.zip",
        RiskLevel::L2,
    );
}

#[test]
fn bitsadmin_transfer_is_l2() {
    check(
        "cmd",
        "bitsadmin /transfer job https://example.com/a.zip C:\\a.zip",
        RiskLevel::L2,
    );
}

#[test]
fn certutil_urlcache_is_l2() {
    check(
        "cmd",
        "certutil -urlcache -split -f https://example.com/a.exe a.exe",
        RiskLevel::L2,
    );
}

#[test]
fn winget_install_is_l2() {
    let result = check(
        "cmd",
        "winget install --id Microsoft.PowerToys",
        RiskLevel::L2,
    );
    assert!(result.reasons[0].contains("Microsoft.PowerToys"));
}

#[test]
fn choco_install_is_l2() {
    check("cmd", "choco install 7zip -y", RiskLevel::L2);
}

#[test]
fn pip_install_from_cmd_is_l2() {
    check("cmd", "pip install requests", RiskLevel::L2);
}

#[test]
fn npm_install_is_l2() {
    check("cmd", "npm install lodash", RiskLevel::L2);
}

#[test]
fn apt_install_is_l2() {
    check("wsl", "sudo apt install curl", RiskLevel::L2);
}

#[test]
fn apt_update_is_l2() {
    check("wsl", "apt-get update", RiskLevel::L2);
}

#[test]
fn pip_install_from_wsl_is_l2() {
    check("wsl", "pip3 install rich", RiskLevel::L2);
}

#[test]
fn git_clone_is_l2() {
    check(
        "wsl",
        "git clone https://example.com/repo.git",
        RiskLevel::L2,
    );
}

#[test]
fn install_module_is_l2() {
    check(
        "powershell",
        "Install-Module PSReadLine -Force",
        RiskLevel::L2,
    );
}

#[test]
fn python_requests_get_is_l2() {
    check(
        "python",
        "requests.get('https://example.com/a.zip')",
        RiskLevel::L2,
    );
}

#[test]
fn python_urlretrieve_is_l2() {
    check(
        "python",
        "urllib.request.urlretrieve('https://example.com/a.zip', 'a.zip')",
        RiskLevel::L2,
    );
}

#[test]
fn docker_pull_is_l2() {
    check("wsl", "docker pull ubuntu:latest", RiskLevel::L2);
}

#[test]
fn winget_list_is_l0() {
    check("cmd", "winget list", RiskLevel::L0);
}

#[test]
fn pip_list_is_l0() {
    check("wsl", "pip list", RiskLevel::L0);
}

#[test]
fn apt_list_is_l0() {
    check("wsl", "apt list --installed", RiskLevel::L0);
}

#[test]
fn npm_list_is_l0() {
    check("cmd", "npm list --depth=0", RiskLevel::L0);
}

#[test]
fn cmd_del_is_l3() {
    check("cmd", "del C:\\Temp\\a.txt", RiskLevel::L3);
}

#[test]
fn cmd_erase_is_l3() {
    check("cmd", "erase /q C:\\Temp\\a.txt", RiskLevel::L3);
}

#[test]
fn cmd_recursive_rd_is_l3() {
    let result = check("cmd", "rd /s /q C:\\Temp\\old", RiskLevel::L3);
    assert!(result.reasons[0].contains("递归"));
}

#[test]
fn powershell_remove_item_is_l3() {
    check("powershell", "Remove-Item C:\\Temp\\a.txt", RiskLevel::L3);
}

#[test]
fn powershell_rm_alias_is_l3() {
    check("powershell", "rm .\\a.txt", RiskLevel::L3);
}

#[test]
fn powershell_ri_alias_is_l3() {
    check("powershell", "ri .\\a.txt", RiskLevel::L3);
}

#[test]
fn powershell_del_alias_is_l3() {
    check("powershell", "del .\\a.txt", RiskLevel::L3);
}

#[test]
fn get_childitem_pipe_remove_is_l3() {
    check(
        "powershell",
        "Get-ChildItem .\\old | Remove-Item -Recurse",
        RiskLevel::L3,
    );
}

#[test]
fn remove_item_case_insensitive_is_l3() {
    check("powershell", "rEmOvE-iTeM .\\a.txt", RiskLevel::L3);
}

#[test]
fn remove_item_reason_mentions_recursive() {
    let result = check(
        "powershell",
        "Remove-Item -Path C:\\Temp\\old -Recurse -Force",
        RiskLevel::L3,
    );
    assert!(result.reasons[0].contains("递归"));
    assert!(result.reasons[0].contains("强制"));
}

#[test]
fn delete_reason_mentions_system_path() {
    let result = check(
        "powershell",
        "Remove-Item C:\\Windows\\System32\\drivers\\etc\\hosts",
        RiskLevel::L3,
    );
    assert!(result.reasons[0].contains("系统关键路径"));
}

#[test]
fn wsl_rm_rf_is_l3() {
    check("wsl", "rm -rf /tmp/build", RiskLevel::L3);
}

#[test]
fn wsl_rmdir_is_l3() {
    check("wsl", "rmdir /tmp/empty", RiskLevel::L3);
}

#[test]
fn wsl_find_delete_is_l3() {
    check("wsl", "find /tmp -name '*.tmp' -delete", RiskLevel::L3);
}

#[test]
fn wsl_find_exec_rm_is_l3() {
    check("wsl", "find /tmp -type f -exec rm -f {} \\;", RiskLevel::L3);
}

#[test]
fn wsl_truncate_is_l3() {
    check("wsl", "truncate -s 0 /var/log/app.log", RiskLevel::L3);
}

#[test]
fn powershell_clear_content_is_l3() {
    check(
        "powershell",
        "Clear-Content .\\important.txt",
        RiskLevel::L3,
    );
}

#[test]
fn powershell_stop_process_is_l1() {
    check("powershell", "Stop-Process -Name notepad", RiskLevel::L1);
}

#[test]
fn cmd_taskkill_is_l1() {
    check("cmd", "taskkill /f /im explorer.exe", RiskLevel::L1);
}

#[test]
fn powershell_stop_service_is_l3() {
    check("powershell", "Stop-Service wuauserv", RiskLevel::L3);
}

#[test]
fn cmd_sc_delete_is_l3() {
    check("cmd", "sc delete MyService", RiskLevel::L3);
}

#[test]
fn cmd_reg_delete_is_l3() {
    check("cmd", "reg delete HKCU\\Software\\Bad /f", RiskLevel::L3);
}

#[test]
fn cmd_shutdown_is_l3() {
    check("cmd", "shutdown /r /t 0", RiskLevel::L3);
}

#[test]
fn wsl_reboot_is_l3() {
    check("wsl", "reboot", RiskLevel::L3);
}

#[test]
fn git_reset_hard_is_l3() {
    check("wsl", "git reset --hard HEAD~1", RiskLevel::L3);
}

#[test]
fn pip_uninstall_is_l3() {
    check("wsl", "pip uninstall requests", RiskLevel::L3);
}

#[test]
fn winget_uninstall_is_l3() {
    check("cmd", "winget uninstall Microsoft.PowerToys", RiskLevel::L3);
}

#[test]
fn cmd_format_requires_typed_confirmation() {
    let result = check("cmd", "format D: /q", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
    assert!(result.reasons[0].contains("D:"));
}

#[test]
fn format_volume_requires_typed_confirmation() {
    let result = check("powershell", "Format-Volume -DriveLetter D", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn diskpart_requires_typed_confirmation() {
    let result = check("cmd", "diskpart /s clean.txt", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
    assert!(result.matched_rules.contains(&"disk.diskpart".to_string()));
}

#[test]
fn clear_disk_requires_typed_confirmation() {
    let result = check(
        "powershell",
        "Clear-Disk -Number 1 -RemoveData",
        RiskLevel::L3,
    );
    assert!(result.requires_typed_confirmation);
    assert!(result.reasons[0].contains("1"));
}

#[test]
fn initialize_disk_requires_typed_confirmation() {
    let result = check("powershell", "Initialize-Disk -Number 2", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn remove_partition_requires_typed_confirmation() {
    let result = check(
        "powershell",
        "Remove-Partition -DiskNumber 1 -PartitionNumber 2",
        RiskLevel::L3,
    );
    assert!(result.requires_typed_confirmation);
}

#[test]
fn bcdedit_delete_requires_typed_confirmation() {
    let result = check("cmd", "bcdedit /delete {current}", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
    assert!(result.reasons[0].contains("{current}"));
}

#[test]
fn bcdedit_deletevalue_requires_typed_confirmation() {
    let result = check(
        "cmd",
        "bcdedit /deletevalue {current} safeboot",
        RiskLevel::L3,
    );
    assert!(result.requires_typed_confirmation);
}

#[test]
fn wsl_mkfs_requires_typed_confirmation() {
    let result = check("wsl", "mkfs.ext4 /dev/sdb1", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn wsl_dd_requires_typed_confirmation() {
    let result = check("wsl", "dd if=image.iso of=/dev/sdb", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn wsl_fdisk_requires_typed_confirmation() {
    let result = check("wsl", "fdisk /dev/sdb", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn cipher_wipe_requires_typed_confirmation() {
    let result = check("cmd", "cipher /w:C:\\Temp", RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn reg_add_is_l1() {
    check(
        "cmd",
        "reg add HKCU\\Software\\App /v Enabled /d 1",
        RiskLevel::L1,
    );
}

#[test]
fn bcdedit_set_is_l3_without_typed_confirmation() {
    let result = check(
        "cmd",
        "bcdedit /set {current} safeboot minimal",
        RiskLevel::L3,
    );
    assert!(!result.requires_typed_confirmation);
}

#[test]
fn set_execution_policy_is_l1() {
    check(
        "powershell",
        "Set-ExecutionPolicy RemoteSigned",
        RiskLevel::L1,
    );
}

#[test]
fn icacls_grant_is_l1() {
    check("cmd", "icacls C:\\Data /grant Users:R", RiskLevel::L1);
}

#[test]
fn wsl_chmod_is_l1() {
    check("wsl", "chmod 600 /home/user/key", RiskLevel::L1);
}

#[test]
fn chkdsk_repair_is_l1() {
    check("cmd", "chkdsk C: /f", RiskLevel::L1);
}

#[test]
fn set_content_system_path_is_l3() {
    check(
        "powershell",
        "Set-Content -Path C:\\Windows\\System32\\drivers\\etc\\hosts -Value value",
        RiskLevel::L3,
    );
}

#[test]
fn redirection_to_system_path_is_l3() {
    check(
        "cmd",
        "echo data > C:\\Windows\\System32\\config.txt",
        RiskLevel::L3,
    );
}

#[test]
fn echo_redirection_is_l1() {
    check("cmd", "echo data > C:\\Temp\\a.txt", RiskLevel::L1);
}

#[test]
fn set_content_is_l1() {
    check("powershell", "Set-Content .\\a.txt hello", RiskLevel::L1);
}

#[test]
fn new_item_is_l1() {
    check(
        "powershell",
        "New-Item -ItemType Directory -Path .\\new",
        RiskLevel::L1,
    );
}

#[test]
fn wsl_touch_is_l1() {
    check("wsl", "touch /tmp/a.txt", RiskLevel::L1);
}

#[test]
fn wsl_copy_is_l1() {
    check("wsl", "cp /tmp/a /tmp/b", RiskLevel::L1);
}

#[test]
fn sed_in_place_is_l1() {
    check("wsl", "sed -i 's/a/b/' /tmp/a.txt", RiskLevel::L1);
}

#[test]
fn python_open_write_is_l1() {
    check(
        "python",
        "open('/tmp/a.txt', 'w').write('x')",
        RiskLevel::L1,
    );
}

#[test]
fn encoded_command_is_l3() {
    check(
        "powershell",
        "powershell -EncodedCommand RwBlAHQALQ...",
        RiskLevel::L3,
    );
}

#[test]
fn enc_abbreviation_is_l3() {
    check("powershell", "pwsh -enc RwBlAHQALQ...", RiskLevel::L3);
}

#[test]
fn e_abbreviation_is_l3() {
    check("powershell", "powershell -e RwBlAHQALQ...", RiskLevel::L3);
}

#[test]
fn encoded_command_case_insensitive_is_l3() {
    check(
        "powershell",
        "POWERSHELL -ENCODEDCOMMAND AAAA",
        RiskLevel::L3,
    );
}

#[test]
fn iex_is_l3() {
    check("powershell", "iex $payload", RiskLevel::L3);
}

#[test]
fn invoke_expression_is_l3() {
    check("powershell", "Invoke-Expression $code", RiskLevel::L3);
}

#[test]
fn from_base64_string_is_l3() {
    check(
        "powershell",
        "[Convert]::FromBase64String($s)",
        RiskLevel::L3,
    );
}

#[test]
fn python_base64_decode_is_l3() {
    check("python", "base64.b64decode(payload)", RiskLevel::L3);
}

#[test]
fn wsl_base64_decode_is_l3() {
    check("wsl", "base64 -d payload.txt", RiskLevel::L3);
}

#[test]
fn certutil_decode_is_l3() {
    check(
        "cmd",
        "certutil -decode payload.txt payload.exe",
        RiskLevel::L3,
    );
}

#[test]
fn dynamic_scriptblock_is_l3() {
    check("powershell", "[scriptblock]::Create($code)", RiskLevel::L3);
}

#[test]
fn wsl_eval_is_l3() {
    check("wsl", "eval $payload", RiskLevel::L3);
}

#[test]
fn python_exec_is_l3() {
    check("python", "exec(user_text)", RiskLevel::L3);
}

#[test]
fn nested_encoded_command_is_l3() {
    check("cmd", "powershell.exe -EncodedCommand AAAA", RiskLevel::L3);
}

#[test]
fn semicolon_compound_takes_highest() {
    check("cmd", "dir; del C:\\Temp\\a", RiskLevel::L3);
}

#[test]
fn and_compound_takes_highest() {
    check("cmd", "dir && format D:", RiskLevel::L3);
}

#[test]
fn or_compound_takes_highest() {
    check("wsl", "ls || rm -rf /tmp/a", RiskLevel::L3);
}

#[test]
fn pipe_compound_takes_highest() {
    check("powershell", "Get-ChildItem | Remove-Item", RiskLevel::L3);
}

#[test]
fn single_ampersand_compound_takes_highest() {
    check("cmd", "dir & del a.txt", RiskLevel::L3);
}

#[test]
fn newline_compound_takes_highest() {
    check("wsl", "ls\nrm /tmp/a", RiskLevel::L3);
}

#[test]
fn all_read_compound_is_l0() {
    check(
        "powershell",
        "Get-Location; Get-Date; Get-Process",
        RiskLevel::L0,
    );
}

#[test]
fn quoted_semicolon_does_not_split() {
    check("cmd", "echo \"a;b\"", RiskLevel::L0);
}

#[test]
fn stderr_redirection_does_not_create_unknown_fragment() {
    check("cmd", "dir C:\\ 2>&1", RiskLevel::L0);
}

#[test]
fn splitter_handles_all_operators() {
    let parts = split_for_test("dir && whoami || ver; hostname & ipconfig | findstr x\nsysteminfo");
    assert_eq!(parts.len(), 7);
}

#[test]
fn nested_cmd_delete_is_l3() {
    check(
        "powershell",
        "cmd /c \"del C:\\Temp\\a.txt\"",
        RiskLevel::L3,
    );
}

#[test]
fn nested_powershell_delete_is_l3() {
    check(
        "cmd",
        "powershell -Command \"Remove-Item C:\\Temp\\a.txt\"",
        RiskLevel::L3,
    );
}

#[test]
fn nested_python_delete_is_l3() {
    check(
        "cmd",
        "python -c \"import os; os.remove('a.txt')\"",
        RiskLevel::L3,
    );
}

#[test]
fn nested_bash_delete_is_l3() {
    check("wsl", "bash -c \"rm -rf /tmp/a\"", RiskLevel::L3);
}

#[test]
fn python_module_pip_install_is_l2() {
    check("cmd", "python -m pip install requests", RiskLevel::L2);
    check("python", "python -m pip install requests", RiskLevel::L2);
    check("python", "python -m pip list", RiskLevel::L0);
    check("python", "python -m pip --version", RiskLevel::L0);
}

#[test]
fn wsl_wrapper_delete_is_l3() {
    check("cmd", "wsl -e rm -rf /tmp/a", RiskLevel::L3);
}

#[test]
fn sudo_delete_is_l3() {
    check("wsl", "sudo rm -rf /var/tmp/a", RiskLevel::L3);
}

#[test]
fn start_process_nested_delete_is_l3() {
    check(
        "powershell",
        "Start-Process powershell -ArgumentList '-Command','Remove-Item C:\\Temp\\a'",
        RiskLevel::L3,
    );
}

#[test]
fn python_print_is_l0() {
    check("python", "print('hello')", RiskLevel::L0);
}

#[test]
fn python_import_os_is_l0() {
    check("python", "import os", RiskLevel::L0);
}

#[test]
fn python_listdir_is_l0() {
    check("python", "os.listdir('/tmp')", RiskLevel::L0);
}

#[test]
fn python_path_read_is_l0() {
    check("python", "Path('/tmp/a').read_text()", RiskLevel::L0);
}

#[test]
fn python_print_delete_text_is_l0() {
    check("python", "print('rm -rf /')", RiskLevel::L0);
}

#[test]
fn python_os_remove_is_l3() {
    let result = check("python", "os.remove('/tmp/a.txt')", RiskLevel::L3);
    assert!(result.reasons[0].contains("/tmp/a.txt"));
}

#[test]
fn python_path_unlink_is_l3() {
    check("python", "Path('/tmp/a.txt').unlink()", RiskLevel::L3);
}

#[test]
fn python_rmtree_is_l3() {
    check("python", "shutil.rmtree('/tmp/old')", RiskLevel::L3);
}

#[test]
fn python_os_system_rm_is_l3() {
    check("python", "os.system('rm -rf /tmp/a')", RiskLevel::L3);
}

#[test]
fn python_subprocess_remove_item_is_l3() {
    check(
        "python",
        "subprocess.run(['powershell','-Command','Remove-Item a'])",
        RiskLevel::L3,
    );
}

#[test]
fn self_report_can_raise_l0_to_l2() {
    let result = classify("cmd", "dir", Some(RiskLevel::L2));
    assert_eq!(result.level, RiskLevel::L2);
    assert!(result.matched_rules.contains(&"self_report.l2".to_string()));
}

#[test]
fn lower_self_report_does_not_downgrade_rule() {
    let result = classify("cmd", "format D:", Some(RiskLevel::L0));
    assert_eq!(result.level, RiskLevel::L3);
    assert!(result.requires_typed_confirmation);
}

#[test]
fn equal_self_report_keeps_rule_result() {
    let result = classify("cmd", "dir", Some(RiskLevel::L0));
    assert_eq!(result.level, RiskLevel::L0);
    assert!(result
        .matched_rules
        .iter()
        .all(|rule| !rule.starts_with("self_report")));
}

#[test]
fn self_report_l3_raises_unknown() {
    let result = classify("wsl", "unknown-tool", Some(RiskLevel::L3));
    assert_eq!(result.level, RiskLevel::L3);
}

#[test]
fn self_report_l1_raises_readonly() {
    let result = classify("powershell", "Get-ChildItem", Some(RiskLevel::L1));
    assert_eq!(result.level, RiskLevel::L1);
}

#[test]
fn self_report_does_not_remove_disk_confirmation() {
    let result = classify("powershell", "Clear-Disk -Number 0", Some(RiskLevel::L3));
    assert!(result.requires_typed_confirmation);
}

#[test]
fn reasons_are_nonempty_chinese() {
    let result = check("cmd", "unknown-command", RiskLevel::L1);
    assert!(!result.reasons.is_empty());
    assert!(result.reasons[0].contains("命令"));
}

#[test]
fn summary_is_nonempty_chinese() {
    let result = check("powershell", "Get-ChildItem", RiskLevel::L0);
    assert!(result.summary_zh.contains("只读"));
}

#[test]
fn matched_rules_are_exposed() {
    let result = check("powershell", "Get-ChildItem", RiskLevel::L0);
    assert!(result
        .matched_rules
        .contains(&"powershell.read_whitelist".to_string()));
}

#[test]
fn download_reason_contains_url() {
    let result = check(
        "powershell",
        "iwr https://example.com/tool.exe",
        RiskLevel::L2,
    );
    assert!(result.reasons[0].contains("https://example.com/tool.exe"));
}

#[test]
fn install_summary_is_specific() {
    let result = check("cmd", "winget install Git.Git", RiskLevel::L2);
    assert!(result.summary_zh.contains("安装"));
}

#[test]
fn disk_summary_mentions_typed_confirmation() {
    let result = check("powershell", "Initialize-Disk -Number 1", RiskLevel::L3);
    assert!(result.summary_zh.contains("逐字输入确认"));
}

#[test]
fn repeated_segments_are_deduplicated() {
    let result = check("powershell", "Get-Date; Get-Date; Get-Date", RiskLevel::L0);
    assert_eq!(result.reasons.len(), 1);
    assert_eq!(result.matched_rules.len(), 1);
}

#[test]
fn mixed_case_alias_is_readonly() {
    check("powershell", "GcI", RiskLevel::L0);
}

#[test]
fn full_cmd_path_is_recognized() {
    check("cmd", "C:\\Windows\\System32\\whoami.exe", RiskLevel::L0);
}

#[test]
fn recursive_takeown_cmd_is_l3() {
    check("cmd", "takeown /r /f C:\\Data", RiskLevel::L3);
}

#[test]
fn recursive_takeown_powershell_is_l3() {
    check("powershell", "takeown /r /f C:\\Data", RiskLevel::L3);
}

#[test]
fn ordinary_write_file_is_l1() {
    let result = super::classify_write_file("D:\\Temp\\repair.log");
    assert_eq!(result.level, RiskLevel::L1);
    assert!(!result.requires_typed_confirmation);
}

#[test]
fn windows_write_file_is_l3() {
    let result = super::classify_write_file("C:\\Windows\\Temp\\repair.log");
    assert_eq!(result.level, RiskLevel::L3);
    assert!(result.summary_zh.contains("系统"));
}

#[test]
fn program_files_write_is_l3() {
    let result = super::classify_write_file("C:\\Program Files\\App\\config.ini");
    assert_eq!(result.level, RiskLevel::L3);
}

#[test]
fn python_system_write_is_l3() {
    check(
        "python",
        r"open('C:\Windows\Temp\a.txt', 'w').write('x')",
        RiskLevel::L3,
    );
}

#[test]
fn powershell_program_files_write_is_l3() {
    check(
        "powershell",
        "Set-Content -Path 'C:\\Program Files\\App\\config.ini' -Value value",
        RiskLevel::L3,
    );
}

#[test]
fn write_file_never_requires_disk_typing() {
    assert!(!super::classify_write_file("C:\\Windows\\system.ini").requires_typed_confirmation);
}
