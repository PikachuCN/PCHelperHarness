import type { CustomTool, InteractionField, InteractionValue } from "./types";

export type ToolboxCategory = "文件管理" | "图片处理" | "PDF 与文档" | "打印与办公" | "系统维修";
export type ToolboxIcon = "rename" | "search" | "organize" | "duplicates" | "invoice" | "watermark" | "compress" | "merge" | "convert" | "print" | "performance" | "network" | "disk" | "health" | "custom";

export interface ToolPreset {
  id: string;
  name: string;
  description: string;
  category: ToolboxCategory | "我的工具";
  icon: ToolboxIcon;
  fields: InteractionField[];
  instructions: string;
  promptTemplate?: string;
  rules?: string;
  actionLabel: string;
  requiresPython?: boolean;
  custom?: boolean;
}

const imageFilters = [{ name: "图片文件", extensions: ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"] }];
const pdfFilters = [{ name: "PDF 文件", extensions: ["pdf"] }];
const invoiceFilters = [{ name: "发票文件", extensions: ["pdf", "jpg", "jpeg", "png", "ofd"] }];

const dryRunField: InteractionField = {
  key: "dryRun",
  label: "先预览，不修改文件",
  description: "建议先核对 AI 给出的处理清单，再决定是否正式执行。",
  kind: "toggle",
  defaultValue: true,
};

export const toolPresets: ToolPreset[] = [
  {
    id: "batch-rename",
    name: "批量重命名",
    description: "替换文本、添加前缀或按顺序编号，先预览再执行。",
    category: "文件管理",
    icon: "rename",
    actionLabel: "生成重命名方案",
    fields: [
      { key: "folder", label: "文件所在文件夹", kind: "folder", required: true },
      { key: "mode", label: "重命名方式", kind: "select", required: true, defaultValue: "replace", options: [
        { value: "replace", label: "替换文件名中的文本" },
        { value: "prefix", label: "添加前缀或后缀" },
        { value: "sequence", label: "按规则批量编号" },
      ] },
      { key: "match", label: "要替换的文本", kind: "text", placeholder: "替换模式必填，其他模式可留空" },
      { key: "replacement", label: "替换为 / 前后缀 / 编号规则", kind: "text", placeholder: "例如：2026 年报销，或 产品图_{序号}" },
      { key: "extensions", label: "限定扩展名", kind: "text", placeholder: "例如 jpg,png,pdf；留空处理全部" },
      { key: "includeSubfolders", label: "包含子文件夹", kind: "toggle", defaultValue: false },
      dryRunField,
    ],
    instructions: "根据用户选择的模式处理，忽略不适用的文本参数。保留原扩展名，检查重名、非法字符和覆盖风险。预览模式只列出旧路径与新路径映射，不修改任何文件；非预览模式也要先给出影响数量并遵循审批，不得静默覆盖。",
  },
  {
    id: "smart-search",
    name: "智能文件搜索",
    description: "按文件名、正文、类型和目录深度组合查找。",
    category: "文件管理",
    icon: "search",
    actionLabel: "开始查找",
    fields: [
      { key: "path", label: "搜索范围", kind: "folder", required: true, description: "只扫描你选择的目录，不会默认扫描整块磁盘。" },
      { key: "query", label: "查找内容", kind: "text", required: true, placeholder: "文件名、关键词或一句话描述" },
      { key: "searchIn", label: "匹配位置", kind: "select", defaultValue: "both", options: [
        { value: "name", label: "仅文件名" },
        { value: "content", label: "仅文件正文" },
        { value: "both", label: "文件名和正文" },
      ] },
      { key: "extensions", label: "文件类型", kind: "text", placeholder: "例如 docx,xlsx,pdf；留空不限" },
      { key: "maxDepth", label: "最大目录深度", kind: "number", defaultValue: 8 },
      { key: "includeHidden", label: "包含隐藏文件", kind: "toggle", defaultValue: false },
    ],
    instructions: "优先调用原生 search_files，将 extensions 解析为不带点的扩展名数组。只读搜索，不移动或修改文件。结果按匹配位置和相关性整理，展示完整路径、大小、修改时间与摘要；自然语言描述无法直接匹配时，先提取最关键的关键词，不要编造结果。",
  },
  {
    id: "organize-files",
    name: "文件智能整理",
    description: "按类型或日期规划归档，保留可追踪的整理清单。",
    category: "文件管理",
    icon: "organize",
    actionLabel: "生成整理方案",
    fields: [
      { key: "source", label: "要整理的文件夹", kind: "folder", required: true },
      { key: "mode", label: "整理方式", kind: "select", defaultValue: "type", options: [
        { value: "type", label: "按文件类型分类" },
        { value: "date", label: "按年月日期归档" },
        { value: "typeDate", label: "先按类型，再按日期" },
      ] },
      { key: "includeSubfolders", label: "包含子文件夹", kind: "toggle", defaultValue: false },
      dryRunField,
    ],
    instructions: "先读取目录并生成源路径到目标路径的清单。预览模式不得移动文件。正式执行时只在用户选定目录内操作，不处理系统目录，不覆盖已有文件，遇到冲突应保留双方并生成明确的新名称。",
  },
  {
    id: "duplicate-files",
    name: "重复文件排查",
    description: "用大小与哈希交叉确认，只生成报告，不自动删除。",
    category: "文件管理",
    icon: "duplicates",
    actionLabel: "扫描重复文件",
    fields: [
      { key: "folder", label: "排查范围", kind: "folder", required: true },
      { key: "extensions", label: "限定扩展名", kind: "text", placeholder: "留空检查全部类型" },
      { key: "includeSubfolders", label: "包含子文件夹", kind: "toggle", defaultValue: true },
    ],
    instructions: "只读检查。先按文件大小分组，再计算候选文件哈希以确认完全重复，避免仅凭文件名判断。输出分组、可释放空间和完整路径；不要删除、移动或修改任何文件，后续清理由用户逐项确认。",
  },
  {
    id: "invoice-rename",
    name: "发票批量改名",
    description: "识别 PDF 或图片发票信息，按日期、号码和金额命名。",
    category: "文件管理",
    icon: "invoice",
    actionLabel: "识别并生成方案",
    requiresPython: true,
    fields: [
      { key: "files", label: "选择发票", kind: "files", required: true, filters: invoiceFilters },
      { key: "rule", label: "命名规则", kind: "select", defaultValue: "dateNumberAmount", options: [
        { value: "dateNumberAmount", label: "开票日期_发票号码_金额" },
        { value: "sellerDateAmount", label: "销售方_开票日期_金额" },
        { value: "numberOnly", label: "发票号码" },
      ] },
      { key: "outputFolder", label: "整理后保存到", kind: "folder", required: true, description: "采用复制方式，原始发票保持不变。" },
      dryRunField,
    ],
    instructions: "优先使用可用的 Python 运行时。文本 PDF 可提取文字后识别；扫描件只有在本地 OCR 能力确实可用时才识别，OFD 或无法识别的文件必须单列，不能猜测发票号码、日期或金额。始终以复制方式输出，原文件不变。预览仅给识别结果和目标文件名；正式执行前检查敏感信息不要外发，并处理重名。",
  },
  {
    id: "image-watermark",
    name: "图片批量水印",
    description: "自定义文字、位置和透明度，批量生成带水印副本。",
    category: "图片处理",
    icon: "watermark",
    actionLabel: "生成水印图片",
    requiresPython: true,
    fields: [
      { key: "files", label: "选择图片", kind: "files", required: true, filters: imageFilters },
      { key: "text", label: "水印文字", kind: "text", required: true, placeholder: "例如：仅供报销使用" },
      { key: "outputFolder", label: "输出文件夹", kind: "folder", required: true },
      { key: "position", label: "水印位置", kind: "select", defaultValue: "bottomRight", options: [
        { value: "bottomRight", label: "右下角" },
        { value: "bottomLeft", label: "左下角" },
        { value: "center", label: "居中" },
        { value: "tiled", label: "平铺全图" },
      ] },
      { key: "opacity", label: "不透明度（%）", kind: "number", defaultValue: 35 },
      { key: "fontSize", label: "字号（px）", kind: "number", defaultValue: 36 },
    ],
    instructions: "使用 Python Pillow 等本地图像库生成新文件，保留原图和 EXIF 可保留部分；选择支持中文的字体，透明度限制在 1 到 100，字号根据图片尺寸合理缩放。输出到指定文件夹，不覆盖原图。完成后报告成功与失败数量，并抽查输出文件确实可读取。",
  },
  {
    id: "image-compress",
    name: "图片批量压缩",
    description: "调整尺寸、质量和格式，控制输出文件体积。",
    category: "图片处理",
    icon: "compress",
    actionLabel: "开始压缩",
    requiresPython: true,
    fields: [
      { key: "files", label: "选择图片", kind: "files", required: true, filters: imageFilters },
      { key: "outputFolder", label: "输出文件夹", kind: "folder", required: true },
      { key: "maxWidth", label: "最大宽度（px）", kind: "number", defaultValue: 1920 },
      { key: "quality", label: "图片质量（1-100）", kind: "number", defaultValue: 78 },
      { key: "format", label: "输出格式", kind: "select", defaultValue: "keep", options: [
        { value: "keep", label: "保持原格式" },
        { value: "jpeg", label: "JPEG" },
        { value: "webp", label: "WebP" },
      ] },
    ],
    instructions: "使用本地 Python 图像库等比例缩小，不放大原图，质量限制在 1 到 100。RGBA 转 JPEG 时使用合适的背景色。只写入输出目录，保留原文件，并对比处理前后的文件大小。",
  },
  {
    id: "pdf-merge",
    name: "PDF 合并",
    description: "将多个 PDF 按所选顺序合并为一个文件。",
    category: "PDF 与文档",
    icon: "merge",
    actionLabel: "合并 PDF",
    requiresPython: true,
    fields: [
      { key: "files", label: "选择要合并的 PDF", kind: "files", required: true, filters: pdfFilters, description: "至少选择两个文件；如顺序重要，AI 会再次向你确认。" },
      { key: "output", label: "合并后保存为", kind: "saveFile", required: true, filters: pdfFilters, defaultValue: "合并文档.pdf" },
    ],
    instructions: "使用 Python pypdf 等可信本地库。先核对文件数量、顺序、加密和损坏情况，至少两个有效 PDF 才合并。若原生选择器未保留用户期望顺序，使用 request_user_input 让用户确认。不得覆盖已有文件，完成后验证页数等于各输入有效页数之和。",
  },
  {
    id: "pdf-convert",
    name: "PDF 格式转换",
    description: "批量转为图片、可检索文本或 Word 文档。",
    category: "PDF 与文档",
    icon: "convert",
    actionLabel: "开始转换",
    requiresPython: true,
    fields: [
      { key: "files", label: "选择 PDF", kind: "files", required: true, filters: pdfFilters },
      { key: "target", label: "转换为", kind: "select", defaultValue: "png", options: [
        { value: "png", label: "PNG 图片" },
        { value: "text", label: "TXT 文本" },
        { value: "docx", label: "Word 文档" },
      ] },
      { key: "outputFolder", label: "输出文件夹", kind: "folder", required: true },
      { key: "dpi", label: "图片清晰度（DPI）", kind: "number", defaultValue: 150, description: "仅转换为图片时使用。" },
    ],
    instructions: "根据目标格式选择可信的本地 Python 库，例如 PyMuPDF、pypdf 或 pdf2docx；缺少依赖时说明并通过既有审批安装，不要谎称转换成功。扫描 PDF 转文本或 Word 时，没有 OCR 就不能保证文字可编辑。文件写入指定目录，不覆盖原文件，并验证输出可打开且页数合理。",
  },
  {
    id: "batch-print",
    name: "文档批量打印",
    description: "收集打印参数、选择真实打印机，确认后统一发送。",
    category: "打印与办公",
    icon: "print",
    actionLabel: "配置打印任务",
    requiresPython: false,
    fields: [
      { key: "files", label: "要打印的文件", kind: "files", required: true, filters: [{ name: "可打印文档", extensions: ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "jpg", "jpeg", "png"] }] },
      { key: "copies", label: "每份打印份数", kind: "number", defaultValue: 1 },
      { key: "color", label: "色彩", kind: "select", defaultValue: "color", options: [
        { value: "color", label: "彩色" },
        { value: "mono", label: "黑白" },
      ] },
      { key: "duplex", label: "双面打印", kind: "select", defaultValue: "none", options: [
        { value: "none", label: "单面" },
        { value: "long", label: "双面 - 长边翻转" },
        { value: "short", label: "双面 - 短边翻转" },
      ] },
    ],
    instructions: "先只读查询真实打印机及能力，再用 request_user_input 的 select 让用户选择打印机和确认最终任务。打印会消耗纸张，发送前必须得到明确确认。根据已安装软件选择可靠的非交互打印方式，不要用会卡住的交互命令；不支持的格式逐项列出，不要静默跳过。",
  },
  {
    id: "repair-performance",
    name: "电脑变慢",
    description: "检查 CPU、内存、磁盘和启动项，定位性能瓶颈。",
    category: "系统维修",
    icon: "performance",
    actionLabel: "开始诊断",
    fields: [],
    instructions: "我的电脑最近明显变慢，请先进行只读诊断，检查 CPU、内存、磁盘、进程与启动项，找出最可能的原因；任何优化修改都先说明并等待批准。",
  },
  {
    id: "repair-network",
    name: "网络连接修复",
    description: "分层排查适配器、网关、DNS、代理与外网连接。",
    category: "系统维修",
    icon: "network",
    actionLabel: "开始排查",
    fields: [],
    instructions: "我的电脑上不了网，请按适配器、网关、DNS、代理和外网连接逐层排查。先只读检查，根据证据推进，不要一开始就重置全部网络配置。",
  },
  {
    id: "repair-disk",
    name: "磁盘空间清理",
    description: "分析空间占用，区分安全临时文件与个人数据。",
    category: "系统维修",
    icon: "disk",
    actionLabel: "分析磁盘",
    fields: [],
    instructions: "我的 C 盘空间不足，请先只读分析空间占用，区分系统临时文件、应用缓存和个人文件。不要默认删除个人目录内容，任何清理都必须列出具体路径、大小与风险并等待确认。",
  },
  {
    id: "repair-health",
    name: "系统全面体检",
    description: "检查系统、硬件、存储、网络与安全关键状态。",
    category: "系统维修",
    icon: "health",
    actionLabel: "开始体检",
    fields: [],
    instructions: "请给我的电脑做一次全面体检，先进行只读检查，覆盖系统、硬件、存储、网络与安全状态。按严重程度汇总问题，不要未经批准修改系统。",
  },
];

export const toolboxCategories: Array<"全部" | ToolboxCategory | "我的工具"> = ["全部", "文件管理", "图片处理", "PDF 与文档", "打印与办公", "系统维修", "我的工具"];

export function customToolAsPreset(tool: CustomTool): ToolPreset {
  return {
    id: tool.id,
    name: tool.name,
    description: tool.description,
    category: "我的工具",
    icon: "custom",
    fields: tool.fields,
    instructions: tool.rules ?? "",
    promptTemplate: tool.promptTemplate,
    rules: tool.rules,
    actionLabel: "使用我的工具",
    custom: true,
  };
}

export function createFieldValues(fields: InteractionField[]): Record<string, InteractionValue> {
  return Object.fromEntries(fields.map((field) => {
    if (field.defaultValue !== undefined && field.defaultValue !== null) return [field.key, field.defaultValue];
    if (field.kind === "multiSelect" || field.kind === "files") return [field.key, []];
    if (field.kind === "toggle") return [field.key, false];
    return [field.key, ""];
  }));
}

export function validateFieldValues(fields: InteractionField[], values: Record<string, InteractionValue>): string | null {
  for (const field of fields) {
    if (!field.required) continue;
    const value = values[field.key];
    if (value === null || value === undefined || (typeof value === "string" && !value.trim()) || (Array.isArray(value) && value.length === 0)) {
      return `请填写或选择“${field.label}”`;
    }
  }
  return null;
}

export function buildToolPrompt(tool: ToolPreset, values: Record<string, InteractionValue>): string {
  const parameters = Object.fromEntries(Object.entries(values).filter(([, value]) => {
    if (value === "" || value === null) return false;
    if (Array.isArray(value) && value.length === 0) return false;
    return true;
  }));
  const workflow = tool.promptTemplate
    ? `${tool.promptTemplate.replace(/\{\{\s*([\w-]+)\s*\}\}/g, (placeholder, key: string) => {
      const value = parameters[key];
      if (value === undefined) return placeholder;
      return typeof value === "string" ? value : JSON.stringify(value);
    })}${tool.rules ? `\n\n固定规则：\n${tool.rules}` : ""}`
    : tool.instructions;
  return `请使用「${tool.name}」完成我的任务。\n\n我已确认的任务参数：\n\`\`\`json\n${JSON.stringify(parameters, null, 2)}\n\`\`\`\n\n执行要求：\n${workflow}\n\n如果还缺少只有我才知道的信息，请调用 request_user_input，以合适的文本、选项、文件或文件夹控件询问，不要要求我手动粘贴长路径。`;
}

export function buildCustomToolRequest(requirement: string): string {
  return `我想创建一个可重复使用的个性工具。\n\n我的需求：${requirement.trim()}\n\n请先理解输入、输出、处理边界和可能风险；信息不足时用 request_user_input 询问。设计完成后调用 save_custom_tool，将自然语言执行流程和可复用的 fields 保存到“我的工具”。prompt_template 中用 {{field_key}} 引用用户以后填写的参数，rules 只保留可复用的安全规则；不要把真实文件内容、密码或隐私信息写进定义。保存后告诉我工具的用法，并询问是否立即运行，不要未经我确认直接处理文件。`;
}

export const customToolExamples = [
  "把客户发来的图片统一压缩，并在右下角加上项目名",
  "找出下载目录里 30 天前的安装包，只生成清单",
  "把每周报表按月份归档，文件名统一成日期_报表名",
];
