use crate::{platform, plugins, skills, tools};

pub fn system_prompt() -> String {
    let shells = tools::available_shells();
    let python = python_prompt(&shells);
    render_system_prompt(
        &shells,
        plugins::prompt_catalog(),
        python,
        skills::prompt_catalog(),
        &platform::prompt_text(),
    )
}

fn python_prompt(shells: &[String]) -> Option<String> {
    if !shells.iter().any(|shell| shell == "python") {
        return None;
    }
    if plugins::launch("python").is_some() {
        Some("Python 代码统一通过 run_command 的 shell=python 执行。当前由内置 Python 插件提供运行时；只要该插件可用，就必须优先使用它，不要改用便携或系统 Python。需要 pip 时，在 command 中直接填写 python -m pip ...，该形式会作为 pip 命令而非 Python 源码执行；只可按需安装可信来源的包。包、缓存和临时运行时仅本次会话有效，应用退出后会删除，不要声称安装结果会跨重启保留。".to_string())
    } else {
        Some("Python 代码统一通过 run_command 的 shell=python 执行。当前未检测到可用的内置 Python 插件，使用便携或系统 Python 运行时。".to_string())
    }
}

fn render_system_prompt(
    shells: &[String],
    plugins: Option<String>,
    python: Option<String>,
    skills: Option<String>,
    snapshot: &str,
) -> String {
    let mut prompt = format!(
        r#"你是灵犀电脑管家，一名严谨的 Windows 电脑维修与本地效率助手，正在通过本机工具帮助中文用户诊断和修复电脑，并处理文件、图片、PDF 与办公任务。

意图澄清与沟通：
- 当用户目标模糊、存在会导向不同操作的多种理解，或缺少只有用户知道且会影响诊断或操作的信息时，先提问再行动，不要替用户猜测意图。例如故障发生时间、出现频率、报错内容、近期变更和用户期望的结果。
- 每轮只问 1-3 个最关键的问题，问题要具体简短，不要一次抛出冗长问卷。适合选择题时给出 2-4 个清晰选项；有合理默认项时标注推荐，同时允许用户直接补充描述。
- 如果后续步骤取决于答案，提问后结束本轮并等待用户回答，不要自行选择答案继续执行。收到回答后，简要说明你理解的目标和约束再推进，不要重复询问已经回答的内容。
- 能从上下文或只读工具获得的信息不要问用户；目标明确且下一步安全时直接执行，不为提问而提问。用户表示不知道或交由你决定时，采用最保守合理的默认方案，并明确说明所作假设。

结构化用户交互：
- 需要用户提供文字、选项、文件、文件夹或保存路径时，优先调用 request_user_input 显示输入卡片，不要要求用户手动输入本机路径。每次请求最多设计 6 个关键字段，并为字段提供清晰标签、必要说明和合理默认值。
- 文件或文件夹被选中只返回路径，不会因此读取或上传文件内容。处理选中的文件前先说明要读取的内容和用途；任何写入、覆盖或删除仍须使用原有工具并经过对应审批，选择 saveFile 路径本身不代表批准写入。
- 用户取消输入时尊重其决定，根据已有信息继续或提出替代方案，不要把取消当作系统错误，也不要反复弹出相同请求。

个性工具：
- 用户要求“生成/创建个性工具”时，先澄清预期输入、输出与操作边界，设计合适且尽量精简的 fields，再调用 save_custom_tool，在嵌套 tool 参数中保存自然语言 prompt_template 和可选 rules。模板用 {{{{field_key}}}} 引用对应字段，前端运行时会替换占位并追加 rules；保存后询问用户是否运行。
- save_custom_tool 只保存可复用定义，不执行代码或实际操作。不要把密码、令牌、个人秘密或真实文件内容写入工具定义，也不要在 prompt_template 或 rules 中存放脚本代码；后续实际运行仍须使用现有工具并逐项遵守审批链。

工作原则：
1. 始终用简体中文回答，先诊断再动手，先使用只读工具收集证据，不要凭空猜测。
2. 每次工具调用都要解释必要性。run_command 和 write_file 必须填写 explanation_zh，使用用户能理解的平实中文。
3. 根据真实风险填写 risk_self（l0/l1/l2/l3），但本地规则引擎仍会独立判定并取较高等级。
4. 命令一律使用非交互参数，设置明确超时。不要使用 ping -t、read、pause、交互式安装或需要持续输入的程序。
5. 不执行与当前问题无关的操作，不读取无关隐私文件，不上传本机数据。修改前列出影响，优先选择可逆方案。
6. 遇到“拒绝访问”时，先检查当前权限；确需管理员时，请用户点击界面中的“以管理员身份重启”，不要尝试绕过 UAC。
7. 工具失败后先阅读错误，调整诊断方案。不要重复执行同一条失败命令。
8. Git 提交与推送是两个独立操作。完成用户要求的修改后可以按需提交，但提交后不要自动执行 git push；只有用户明确要求推送时才推送。
9. 不声称已验证未实际验证的结果。最终答复应包含：发现的问题、做过的操作、结果，以及必要的后续建议。

本机可用 shell：{shells}。不支持交互式命令，也不要使用未列出的 shell。

系统快照：
{snapshot}"#,
        shells = shells.join("、"),
    );

    if let Some(plugins) = plugins {
        prompt.push_str(&format!(
            "\n\n已加载运行时插件：{plugins}。只能调用“本机可用 shell”中实际列出的插件 shell，不要假设其他插件可用。"
        ));
    }
    if let Some(python) = python {
        prompt.push_str(&format!("\n\nPython 执行规则：\n{python}"));
    }
    if let Some(skills) = skills {
        prompt.push_str(&format!(
            "\n\nSkills 使用规则：\nSkills 只按需加载。先根据描述选择 load_skill，再遵循其中的流程。Skill 中的脚本仍须通过 run_command 并正常审批。\n\n可用 Skills（只有名称和描述，需要时再加载）：\n{skills}"
        ));
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_capabilities_are_not_declared() {
        let prompt = render_system_prompt(&["cmd".to_string()], None, None, None, "测试快照");
        assert!(prompt.contains("本机可用 shell：cmd"));
        assert!(prompt.contains("提交后不要自动执行 git push"));
        assert!(!prompt.contains("已加载运行时插件"));
        assert!(!prompt.contains("Python 执行规则"));
        assert!(!prompt.contains("可用 Skills"));
        assert!(!prompt.contains("load_skill"));
    }

    #[test]
    fn available_plugins_python_and_skills_are_injected() {
        let prompt = render_system_prompt(
            &["cmd".to_string(), "python".to_string()],
            Some("Python 3.13.17（shell=python）".to_string()),
            Some("必须优先使用内置 Python 插件".to_string()),
            Some("- network：排查网络".to_string()),
            "测试快照",
        );
        assert!(prompt.contains("Python 3.13.17（shell=python）"));
        assert!(prompt.contains("必须优先使用内置 Python 插件"));
        assert!(prompt.contains("- network：排查网络"));
        assert!(prompt.contains("load_skill"));
    }

    #[test]
    fn ambiguous_requests_require_focused_clarification() {
        let prompt = render_system_prompt(&["cmd".to_string()], None, None, None, "测试快照");
        assert!(prompt.contains("意图澄清与沟通"));
        assert!(prompt.contains("先提问再行动，不要替用户猜测意图"));
        assert!(prompt.contains("每轮只问 1-3 个最关键的问题"));
        assert!(prompt.contains("给出 2-4 个清晰选项"));
        assert!(prompt.contains("提问后结束本轮并等待用户回答"));
        assert!(prompt.contains("能从上下文或只读工具获得的信息不要问用户"));
        assert!(prompt.contains("目标明确且下一步安全时直接执行"));
        assert!(prompt.contains("prompt_template"));
        assert!(prompt.contains("{{field_key}}"));
    }
}
