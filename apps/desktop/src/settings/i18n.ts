import type { Language } from "./types";

const en: Record<string, string> = {
  "settings.title": "Settings",
  "settings.search": "Search settings...",
  "settings.general": "General",
  "settings.appearance": "Appearance",
  "settings.model_provider": "Model Providers",
  "settings.agent_runtime": "Agent Runtime",
  "settings.governance": "Governance",
  "settings.risk_gate": "Risk Gate",
  "settings.cognitive_customs": "Cognitive Customs",
  "settings.policy_engine": "Policy Engine",
  "settings.privacy": "Privacy & Data",
  "settings.developer": "Developer",
  "settings.saved": "Saved",
  "settings.save": "Save Changes",
  "settings.reset": "Reset to Defaults",
  "settings.unsaved": "Unsaved changes",
  "settings.default_home": "Default Home",
  "settings.default_home_desc": "Default page shown on startup",
  "settings.startup_behavior": "Startup Behavior",
  "settings.startup_behavior_desc": "What happens when you open the console",
  "settings.default_mission_mode": "Default Mission Mode",
  "settings.autosave_drafts": "Autosave Drafts",
  "settings.time_format": "Time Format",
  "settings.region": "Region",
  "settings.language": "Language",
  "settings.theme": "Theme",
  "settings.font_size": "Font Size",
  "settings.density": "Density",
  "settings.sidebar_mode": "Sidebar Mode",
  "settings.reduce_motion": "Reduce Motion",
  "settings.high_contrast": "High Contrast",
  "settings.provider": "Provider",
  "settings.base_url": "Base URL",
  "settings.api_key": "API Key",
  "settings.default_model": "Default Model",
  "settings.fast_model": "Fast Model",
  "settings.reasoning_model": "Reasoning Model",
  "settings.max_tokens": "Max Tokens",
  "settings.temperature": "Temperature",
  "settings.request_timeout_ms": "Request Timeout (ms)",
  "settings.test_connection": "Save & Test Connection",
  "settings.test_success": "Connection successful",
  "settings.test_failed": "Connection failed",
  "settings.api_key_warning": "Your key is stored locally on this device.",
  "settings.open_last_task": "Open Last Task",
  "settings.open_new_task": "Open New Task",
  "settings.auto": "Auto Detect",
  "settings.readonly": "Read-Only Analysis",
  "settings.collaborative": "Collaborative Approval",
  "settings.high_risk": "High-Risk Request",
};

const zh: Record<string, string> = {
  "settings.title": "设置",
  "settings.search": "搜索设置...",
  "settings.general": "通用",
  "settings.appearance": "外观",
  "settings.model_provider": "模型供应商",
  "settings.agent_runtime": "智能体运行时",
  "settings.governance": "治理",
  "settings.risk_gate": "风险闸门",
  "settings.cognitive_customs": "认知海关",
  "settings.policy_engine": "制度层",
  "settings.privacy": "隐私与数据",
  "settings.developer": "开发者",
  "settings.saved": "已保存",
  "settings.save": "保存更改",
  "settings.reset": "重置为默认值",
  "settings.unsaved": "有未保存的更改",
  "settings.default_home": "默认首页",
  "settings.default_home_desc": "启动时显示的默认页面",
  "settings.startup_behavior": "启动行为",
  "settings.startup_behavior_desc": "打开控制台时的默认行为",
  "settings.default_mission_mode": "默认任务模式",
  "settings.autosave_drafts": "自动保存草稿",
  "settings.time_format": "时间格式",
  "settings.region": "区域",
  "settings.language": "语言",
  "settings.theme": "主题",
  "settings.font_size": "字号",
  "settings.density": "界面密度",
  "settings.sidebar_mode": "侧边栏模式",
  "settings.reduce_motion": "减少动画",
  "settings.high_contrast": "高对比度",
  "settings.provider": "供应商",
  "settings.base_url": "Base URL",
  "settings.api_key": "API Key",
  "settings.default_model": "默认模型",
  "settings.fast_model": "快速模型",
  "settings.reasoning_model": "推理模型",
  "settings.max_tokens": "最大 Token 数",
  "settings.temperature": "Temperature",
  "settings.request_timeout_ms": "请求超时 (ms)",
  "settings.test_connection": "保存并测试连接",
  "settings.test_success": "连接成功",
  "settings.test_failed": "连接失败",
  "settings.api_key_warning": "密钥将保存在本机设备上。",
  "settings.open_last_task": "打开上次任务",
  "settings.open_new_task": "打开新任务",
  "settings.auto": "自动判断",
  "settings.readonly": "只读分析",
  "settings.collaborative": "协作审批",
  "settings.high_risk": "高风险申请",
};

const dicts: Record<Language, Record<string, string>> = { zh, en };

let currentLang: Language = (localStorage.getItem("coevo-lang") as Language) || "en";

export function setLanguage(lang: Language) {
  currentLang = lang;
  localStorage.setItem("coevo-lang", lang);
}

export function getLanguage(): Language {
  return currentLang;
}

export function t(key: string): string {
  return dicts[currentLang]?.[key] || en[key] || key;
}
