# Harness 电脑维修助手

Harness 是一款基于 **Rust + Tauri 2 + React + TypeScript** 的 Windows 中文桌面维修助手。它通过 OpenAI 兼容的大模型接口，将自然语言描述的电脑问题转化为可审查、可审批、可追踪的本机诊断与修复流程。

应用不仅生成排查建议，还能在用户授权后调用系统工具、执行命令、读取诊断信息并持续根据结果调整方案。所有模型工具调用均由 Rust 后端进行风险判断和审批控制。

![Harness 浅色主界面](unslop/final-light.live.png)

## 核心功能

- **OpenAI 兼容接口**：支持 Chat Completions 与 Responses 协议，可获取模型列表、测试连接并使用自定义模型。
- **Agent 诊断循环**：支持流式正文与推理内容、工具结果回传、最多 25 步执行、主动停止及会话自动保存。
- **本机维修工具**：覆盖 CMD、PowerShell、WSL、Python、系统信息、进程、服务、事件日志、文件和注册表等能力。
- **风险分级审批**：区分只读、系统修改、下载安装和破坏性操作；高风险操作由后端强制审批，必要时要求二次确认。
- **执行保护**：非交互命令、实时输出、中文编码处理、超时控制、结果截断及进程树回收。
- **会话管理**：新建、重命名、删除、恢复和导出 Markdown；可复制消息并查看用户消息对应的实际提示词。
- **中文桌面界面**：提供浅色/深色主题、工具状态卡片、内嵌审批、实时运行状态和应用内右键菜单。
- **内置 Skills**：包含网络排查、磁盘清理、蓝屏分析、开机缓慢、打印机和用户权限六类维修流程。

## 安全设计

所有模型发起的工具调用都经过统一的后端执行链：

```text
工具参数解析 → 本地风险规则 → 与模型自报等级取较高值 → 审批闸门 → 执行器
```

| 等级 | 操作类型 | 示例 |
| --- | --- | --- |
| L0 | 只读诊断 | 查询系统信息、读取日志、查看进程 |
| L1 | 本机修改 | 修改配置、管理服务或写入文件 |
| L2 | 下载安装 | 下载组件、安装或卸载软件 |
| L3 | 破坏性操作 | 磁盘级操作、动态执行或其他高风险命令 |

其他保护措施：

- 前端没有绕过审批的独立命令执行入口，Skill 脚本也经过相同风控链路。
- L3 首次允许不会直接执行，后端会要求二次确认；部分磁盘操作还会校验确认文本。
- 接口密钥使用 Windows DPAPI 加密保存，不写入源码。
- 命令支持超时、取消和输出限制，Windows 下通过 Job Object 回收进程树。
- 无法可靠分析的命令采用收紧策略；编码或动态执行类命令按高风险处理。

> **注意：** 本应用可以真实修改系统状态。使用前应确认模型、命令和审批提示，重要数据请提前备份。自动审批模式不等于无风险模式。

## 技术栈

| 层级 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2、Rust 2021 |
| 前端 | React 19、TypeScript、Vite 6 |
| 状态与展示 | Zustand、React Markdown、Lucide |
| 模型通信 | OpenAI 兼容 Chat Completions / Responses、SSE |
| Windows 能力 | DPAPI、注册表、服务、事件日志、Job Object |
| 扩展运行时 | 可选 Python DLL 插件、外部便携 Python 环境 |

## 环境要求

### 直接运行构建产物

- Windows 10/11
- Microsoft Edge WebView2 Runtime
- 一个可用的 OpenAI 兼容接口及其 API Key

### 从源码开发

- Windows 10/11
- Node.js 20 或更高版本
- pnpm 12.18.0
- Rust stable 工具链
- Tauri 2 所需的 Windows 构建依赖与 WebView2

## 本地开发

```bash
git clone https://github.com/PikachuCN/PCHelperHarness.git
cd PCHelperHarness
pnpm install
pnpm tauri dev
```

首次启动后，在设置页填写接口地址、API Key、接口协议和模型。密钥不会随仓库提交。

常用命令：

```bash
# 启动前端开发服务器
pnpm dev

# TypeScript 类型检查
pnpm typecheck

# 构建前端资源
pnpm build

# 构建 Python DLL 插件
pnpm build:plugins

# 运行 Rust 测试
cargo test --manifest-path src-tauri/Cargo.toml
```

## 构建发布版本

```bash
pnpm tauri build --no-bundle
```

该命令会依次构建前端、Python 插件和 Tauri 生产程序，主要产物位于：

```text
src-tauri/target/release/harness.exe
src-tauri/target/release/python.dll
```

生产 EXE 应通过上述 Tauri 命令完整构建。不要仅用 `cargo build --release` 覆盖发布产物，否则可能将开发服务器地址编译进程序，导致开发服务关闭后无法加载界面。

基础功能只需要 `harness.exe`；需要 Python 工具时，将 `python.dll` 与 EXE 放在同一目录并重启应用。未签名程序可能触发 Windows SmartScreen 提示。

## 配置与数据

- 配置优先保存在 EXE 同目录；目录不可写时回退到 `%APPDATA%\Harness`。
- 会话使用本地 JSON 保存，可从界面导出为 Markdown。
- 首次启动会生成 `config.json` 和 `skills/`，运行数据按需写入 `data/`。
- `config.json`、运行数据、构建产物和依赖目录均已加入 `.gitignore`。

### Python 运行时插件

`python.dll` 是可选的本机运行时插件。主程序只会向模型暴露实际可用的 shell；未检测到 Python 插件或运行时时，不会声明 Python 工具可用。

Python 按以下顺序解析：

1. 应用同级 `python/python.exe`
2. 应用同级 `python.exe`
3. DLL 插件提供的运行时
4. 系统 `PATH` 中的 `python`、`py -3` 或 `python3`

DLL 具有完整本机代码权限，只应使用来源可信的插件。

## 项目结构

```text
.
├── src/                    # React 界面、状态管理与 Tauri IPC
├── src-tauri/
│   ├── src/agent/          # Agent 循环、审批与会话
│   ├── src/llm/            # 两种模型协议与流式响应解析
│   ├── src/risk/           # 风险规则、中文说明与专项测试
│   ├── src/tools/          # Shell、Python 和系统诊断工具
│   ├── src/platform/       # DPAPI、提权与快照等 Windows 能力
│   ├── src/plugins/        # DLL 插件加载
│   └── plugins/python/     # Python 运行时插件
├── assets/                 # 项目资源
├── unslop/                 # 界面验证截图
├── WALKTHROUGH.md          # 实施、修复与验证记录
└── package.json
```

## 验证与使用边界

项目已覆盖风险引擎、模型工具调用链、命令执行、DPAPI、前端类型检查和原生冒烟测试，详细记录见 [WALKTHROUGH.md](WALKTHROUGH.md)。

真实 API 账户、UAC 提权及实际系统变更仍应在可控测试环境中人工验证。风险规则采用失败收紧策略，但无法保证识别所有混淆命令；请勿将模型输出或自动审批视为绝对安全保证。
