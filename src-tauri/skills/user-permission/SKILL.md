---
name: user-permission
description: 排查文件访问 denied、账户权限、UAC 与 Windows 用户配置问题
---

# 用户与权限排查

## 只读检查

1. 用 `whoami /user`、`whoami /groups` 和 `whoami /priv` 检查当前令牌，不使用任何修改参数。
2. 用 `Get-LocalUser`、`Get-LocalGroupMember Administrators` 等只读命令确认账户和组成员关系。
3. 对具体路径使用 `Get-Acl` 查看所有者、访问规则和继承，不直接接管所有权。
4. 区分标准用户、已提升的管理员令牌和文件 ACL 拒绝，避免把所有“拒绝访问”都当成需要提权。

## 修复约束

- 首选最小权限修改，只针对明确路径或对象，记录原始 ACL。
- `takeown`、递归 `icacls`、修改管理员组或账户密码前，清楚说明安全影响并走审批。
- 不默认启用内置 Administrator，不关闭 UAC，不给 Everyone 完全控制权限。
- 需要管理员时建议用户通过界面“以管理员身份重启”，不要尝试绕过 UAC。
- 修复后重新读取 ACL 或权限结果，并给出恢复方法。
