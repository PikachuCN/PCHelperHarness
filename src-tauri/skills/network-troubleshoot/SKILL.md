---
name: network-troubleshoot
description: 分层排查无法上网、DNS 异常、代理冲突、Wi-Fi 与网卡问题
---

# 网络连接排查

先确认影响范围：所有设备都不能上网时优先检查路由器或运营商，不要立即修改本机设置。

## 只读诊断顺序

1. 用 `get_system_info` 记录系统与网卡概况。
2. 用 `ipconfig /all` 检查地址、网关、DHCP 与 DNS，不要先执行 `/release`。
3. 用 `ping <默认网关>`、`ping 1.1.1.1` 和 `nslookup www.microsoft.com` 区分链路、外网和 DNS 故障；每个 ping 都必须限制次数，例如 `-n 4`。
4. 用 `Get-NetAdapter`、`Get-NetIPConfiguration`、`Get-DnsClientServerAddress` 检查适配器状态与配置。
5. 检查系统代理和可能冲突的 VPN、安全软件。读取最近 24 小时 System 日志中的网络相关错误。

## 修复原则

- 一次只改一个变量，每次修改后重新测试。
- 重置 Winsock、TCP/IP、刷新 DNS 或重启适配器前，说明网络会短暂中断，并正常走审批。
- 不关闭防火墙，不下载来路不明的“网络修复工具”。
- 结束时列出根因、已改配置和仍需用户配合的步骤。
