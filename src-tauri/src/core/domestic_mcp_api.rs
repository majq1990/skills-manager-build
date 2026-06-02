use serde::{Deserialize, Serialize};

/// Domestic MCP Server info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomesticMcpServer {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider: McpProvider,
    pub url: String,
    pub category: String,
    pub installs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpProvider {
    #[serde(rename = "aliyun")]
    Aliyun,
    #[serde(rename = "bytedance")]
    Bytedance,
    #[serde(rename = "tencent")]
    Tencent,
    #[serde(rename = "dingtalk")]
    Dingtalk,
    #[serde(rename = "gitee")]
    Gitee,
    #[serde(rename = "mcpso")]
    McpSo,
    #[serde(rename = "modelscope")]
    ModelScope,
    #[serde(rename = "baidu")]
    BaiduMcp,
    #[serde(rename = "higress")]
    Higress,
    #[serde(rename = "pulsemcp")]
    PulseMcp,
}

impl McpProvider {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            McpProvider::Aliyun => "阿里云",
            McpProvider::Bytedance => "字节跳动",
            McpProvider::Tencent => "腾讯云",
            McpProvider::Dingtalk => "钉钉",
            McpProvider::Gitee => "Gitee",
            McpProvider::McpSo => "MCP.so",
            McpProvider::ModelScope => "魔搭社区",
            McpProvider::BaiduMcp => "百度MCP广场",
            McpProvider::Higress => "Higress MCP",
            McpProvider::PulseMcp => "PulseMCP",
        }
    }

    pub fn icon_url(&self) -> &'static str {
        match self {
            McpProvider::Aliyun => "https://img.alicdn.com/imgextra/i4/O1CN01Z5paLz1O0zuCC7osS_!!6000000001644-55-tps-83-82.svg",
            McpProvider::Bytedance => "https://p3-tt.byteimg.com/origin/pgc-image/4d1f9b6c3d6e4c4e9b7f6a5e4d3c2b1a",
            McpProvider::Tencent => "https://cloud.tencent.com/favicon.ico",
            McpProvider::Dingtalk => "https://www.dingtalk.com/favicon.ico",
            McpProvider::Gitee => "https://gitee.com/favicon.ico",
            McpProvider::McpSo => "https://mcp.so/favicon.ico",
            McpProvider::ModelScope => "https://modelscope.cn/favicon.ico",
            McpProvider::BaiduMcp => "https://www.baidu.com/favicon.ico",
            McpProvider::Higress => "https://higress.cn/favicon.ico",
            McpProvider::PulseMcp => "https://www.pulsemcp.com/favicon.ico",
        }
    }

}

/// Domestic MCP Market API
/// Aggregates MCP servers from domestic providers
pub struct DomesticMcpMarket;

impl DomesticMcpMarket {
    /// Get all domestic MCP servers (sorted by installs DESC for "Top" defaults)
    pub fn list_all() -> Vec<DomesticMcpServer> {
        let mut servers = Vec::new();
        // 5 大国内 MCP 市场（按访问量排序：MCP.so > 魔搭 > 百度 > Higress > PulseMCP）
        servers.extend(Self::mcpso_servers());
        servers.extend(Self::modelscope_servers());
        servers.extend(Self::baidu_servers());
        servers.extend(Self::higress_servers());
        servers.extend(Self::pulsemcp_servers());
        // 厂商云
        servers.extend(Self::aliyun_servers());
        servers.extend(Self::bytedance_servers());
        servers.extend(Self::tencent_servers());
        servers.extend(Self::dingtalk_servers());
        servers.sort_by(|a, b| b.installs.cmp(&a.installs));
        servers
    }

    /// Top N servers across all sources (default landing list)
    #[allow(dead_code)]
    pub fn list_top(n: usize) -> Vec<DomesticMcpServer> {
        let mut all = Self::list_all();
        all.truncate(n);
        all
    }

    /// Search domestic MCP servers
    pub fn search(query: &str) -> Vec<DomesticMcpServer> {
        let all = Self::list_all();
        let query_lower = query.to_lowercase();

        all.into_iter()
            .filter(|server| {
                server.name.to_lowercase().contains(&query_lower)
                    || server.description.to_lowercase().contains(&query_lower)
                    || server.category.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Filter by provider
    pub fn filter_by_provider(provider: McpProvider) -> Vec<DomesticMcpServer> {
        Self::list_all()
            .into_iter()
            .filter(|s| s.provider == provider)
            .collect()
    }

    // 阿里云百炼 MCP 服务列表
    fn aliyun_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "aliyun-bailian-chat".to_string(),
                name: "阿里云百炼对话".to_string(),
                description: "调用阿里云百炼大模型服务进行对话".to_string(),
                provider: McpProvider::Aliyun,
                url: "https://bailian.aliyuncs.com/mcp".to_string(),
                category: "AI/LLM".to_string(),
                installs: 15000,
            },
            DomesticMcpServer {
                id: "aliyun-oss".to_string(),
                name: "阿里云OSS存储".to_string(),
                description: "操作阿里云对象存储服务(OSS)的文件".to_string(),
                provider: McpProvider::Aliyun,
                url: "https://mcp-oss.aliyuncs.com".to_string(),
                category: "Storage".to_string(),
                installs: 8900,
            },
            DomesticMcpServer {
                id: "aliyun-rds".to_string(),
                name: "阿里云RDS数据库".to_string(),
                description: "查询和管理阿里云RDS数据库实例".to_string(),
                provider: McpProvider::Aliyun,
                url: "https://mcp-rds.aliyuncs.com".to_string(),
                category: "Database".to_string(),
                installs: 6200,
            },
            DomesticMcpServer {
                id: "aliyun-dns".to_string(),
                name: "阿里云DNS解析".to_string(),
                description: "管理阿里云云解析DNS记录".to_string(),
                provider: McpProvider::Aliyun,
                url: "https://mcp-dns.aliyuncs.com".to_string(),
                category: "Network".to_string(),
                installs: 4300,
            },
            DomesticMcpServer {
                id: "aliyun-sls".to_string(),
                name: "阿里云日志服务".to_string(),
                description: "查询和分析阿里云SLS日志服务".to_string(),
                provider: McpProvider::Aliyun,
                url: "https://mcp-sls.aliyuncs.com".to_string(),
                category: "Logging".to_string(),
                installs: 3800,
            },
        ]
    }

    // 字节跳动 MCP 服务列表
    fn bytedance_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "bytedance-doubao".to_string(),
                name: "豆包大模型".to_string(),
                description: "调用字节跳动豆包大模型API进行对话".to_string(),
                provider: McpProvider::Bytedance,
                url: "https://mcp.volcengine.com/doubao".to_string(),
                category: "AI/LLM".to_string(),
                installs: 22000,
            },
            DomesticMcpServer {
                id: "bytedance-coding".to_string(),
                name: "字节编程助手".to_string(),
                description: "基于字节大模型的代码生成和分析".to_string(),
                provider: McpProvider::Bytedance,
                url: "https://mcp.volcengine.com/coding".to_string(),
                category: "Development".to_string(),
                installs: 12000,
            },
            DomesticMcpServer {
                id: "bytedance-ark".to_string(),
                name: "火山方舟".to_string(),
                description: "访问火山方舟平台上的大模型服务".to_string(),
                provider: McpProvider::Bytedance,
                url: "https://ark.mcp.volcengine.com".to_string(),
                category: "AI/LLM".to_string(),
                installs: 9500,
            },
            DomesticMcpServer {
                id: "bytedance-tos".to_string(),
                name: "火山TOS存储".to_string(),
                description: "操作火山引擎对象存储服务".to_string(),
                provider: McpProvider::Bytedance,
                url: "https://tos.mcp.volcengine.com".to_string(),
                category: "Storage".to_string(),
                installs: 5600,
            },
        ]
    }

    // 腾讯云 MCP 服务列表
    fn tencent_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "tencent-hunyuan".to_string(),
                name: "腾讯混元大模型".to_string(),
                description: "调用腾讯混元大模型API".to_string(),
                provider: McpProvider::Tencent,
                url: "https://mcp.cloud.tencent.com/hunyuan".to_string(),
                category: "AI/LLM".to_string(),
                installs: 18000,
            },
            DomesticMcpServer {
                id: "tencent-cos".to_string(),
                name: "腾讯云COS".to_string(),
                description: "操作腾讯云对象存储(COS)".to_string(),
                provider: McpProvider::Tencent,
                url: "https://mcp-cos.tencentcloudapi.com".to_string(),
                category: "Storage".to_string(),
                installs: 11000,
            },
            DomesticMcpServer {
                id: "tencent-cdb".to_string(),
                name: "腾讯云数据库".to_string(),
                description: "查询腾讯云云数据库MySQL实例".to_string(),
                provider: McpProvider::Tencent,
                url: "https://mcp-cdb.tencentcloudapi.com".to_string(),
                category: "Database".to_string(),
                installs: 7200,
            },
            DomesticMcpServer {
                id: "tencent-scf".to_string(),
                name: "云函数SCF".to_string(),
                description: "管理和调用腾讯云云函数".to_string(),
                provider: McpProvider::Tencent,
                url: "https://mcp-scf.tencentcloudapi.com".to_string(),
                category: "Serverless".to_string(),
                installs: 5800,
            },
            DomesticMcpServer {
                id: "tencent-ocr".to_string(),
                name: "腾讯云OCR".to_string(),
                description: "调用腾讯云文字识别OCR服务".to_string(),
                provider: McpProvider::Tencent,
                url: "https://mcp-ocr.tencentcloudapi.com".to_string(),
                category: "AI/Vision".to_string(),
                installs: 9200,
            },
        ]
    }

    // 钉钉 MCP 服务列表
    fn dingtalk_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "dingtalk-calendar".to_string(),
                name: "钉钉日历".to_string(),
                description: "查询和管理钉钉日历日程".to_string(),
                provider: McpProvider::Dingtalk,
                url: "https://mcp-gw.dingtalk.com/calendar".to_string(),
                category: "Productivity".to_string(),
                installs: 8500,
            },
            DomesticMcpServer {
                id: "dingtalk-todo".to_string(),
                name: "钉钉待办".to_string(),
                description: "创建和管理钉钉待办任务".to_string(),
                provider: McpProvider::Dingtalk,
                url: "https://mcp-gw.dingtalk.com/todo".to_string(),
                category: "Productivity".to_string(),
                installs: 7800,
            },
            DomesticMcpServer {
                id: "dingtalk-approval".to_string(),
                name: "钉钉审批".to_string(),
                description: "提交和查询钉钉审批流程".to_string(),
                provider: McpProvider::Dingtalk,
                url: "https://mcp-gw.dingtalk.com/approval".to_string(),
                category: "Workflow".to_string(),
                installs: 6500,
            },
            DomesticMcpServer {
                id: "dingtalk-sheet".to_string(),
                name: "钉钉表格".to_string(),
                description: "操作钉钉AI表格数据".to_string(),
                provider: McpProvider::Dingtalk,
                url: "https://mcp-gw.dingtalk.com/sheet".to_string(),
                category: "Productivity".to_string(),
                installs: 5200,
            },
        ]
    }

    // ──────────────────────────────────────────────────────────
    // MCP.so —— 国内访问量最大的 MCP 聚合站（综合榜 Top）
    // ──────────────────────────────────────────────────────────
    fn mcpso_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "mcpso-filesystem".into(), name: "Filesystem".into(),
                description: "本地文件系统读写、目录扫描的官方 MCP".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/filesystem".into(),
                category: "Filesystem".into(), installs: 185000,
            },
            DomesticMcpServer {
                id: "mcpso-github".into(), name: "GitHub".into(),
                description: "管理 Issue / PR / 仓库内容的 GitHub MCP".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/github".into(),
                category: "Development".into(), installs: 162000,
            },
            DomesticMcpServer {
                id: "mcpso-puppeteer".into(), name: "Puppeteer".into(),
                description: "无头浏览器自动化、截图、抓取".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/puppeteer".into(),
                category: "Browser".into(), installs: 134000,
            },
            DomesticMcpServer {
                id: "mcpso-postgres".into(), name: "PostgreSQL".into(),
                description: "查询 / 写入 PostgreSQL 数据库的 MCP".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/postgres".into(),
                category: "Database".into(), installs: 98000,
            },
            DomesticMcpServer {
                id: "mcpso-bravesearch".into(), name: "Brave Search".into(),
                description: "Brave Web Search API 集成".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/brave-search".into(),
                category: "Search".into(), installs: 87000,
            },
            DomesticMcpServer {
                id: "mcpso-fetch".into(), name: "Fetch".into(),
                description: "抓取任意 URL 内容并转换为 Markdown".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/fetch".into(),
                category: "Web".into(), installs: 76000,
            },
            DomesticMcpServer {
                id: "mcpso-memory".into(), name: "Memory".into(),
                description: "对话记忆持久化（知识图谱 KG）".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/memory".into(),
                category: "Memory".into(), installs: 64000,
            },
            DomesticMcpServer {
                id: "mcpso-time".into(), name: "Time".into(),
                description: "时区/日期处理工具".into(),
                provider: McpProvider::McpSo, url: "https://mcp.so/server/time".into(),
                category: "Utility".into(), installs: 51000,
            },
        ]
    }

    // ──────────────────────────────────────────────────────────
    // 魔搭 ModelScope MCP（阿里官方开源社区）
    // ──────────────────────────────────────────────────────────
    fn modelscope_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "modelscope-search".into(), name: "ModelScope 搜索".into(),
                description: "搜索魔搭社区 5000+ 模型 / 数据集 / 创空间".into(),
                provider: McpProvider::ModelScope,
                url: "https://modelscope.cn/mcp/servers/modelscope-search".into(),
                category: "Search".into(), installs: 92000,
            },
            DomesticMcpServer {
                id: "modelscope-image".into(), name: "通义万相文生图".into(),
                description: "调用魔搭通义万相生图能力".into(),
                provider: McpProvider::ModelScope,
                url: "https://modelscope.cn/mcp/servers/wanxiang".into(),
                category: "AI/Image".into(), installs: 71000,
            },
            DomesticMcpServer {
                id: "modelscope-qwen".into(), name: "Qwen 推理".into(),
                description: "在魔搭上推理 Qwen2.5 / Qwen3 系列模型".into(),
                provider: McpProvider::ModelScope,
                url: "https://modelscope.cn/mcp/servers/qwen".into(),
                category: "AI/LLM".into(), installs: 88000,
            },
            DomesticMcpServer {
                id: "modelscope-paraformer".into(), name: "Paraformer 语音识别".into(),
                description: "中文语音识别（达摩院开源）".into(),
                provider: McpProvider::ModelScope,
                url: "https://modelscope.cn/mcp/servers/paraformer".into(),
                category: "AI/Audio".into(), installs: 42000,
            },
            DomesticMcpServer {
                id: "modelscope-cosyvoice".into(), name: "CosyVoice 语音合成".into(),
                description: "阿里达摩院开源 TTS 模型 MCP".into(),
                provider: McpProvider::ModelScope,
                url: "https://modelscope.cn/mcp/servers/cosyvoice".into(),
                category: "AI/Audio".into(), installs: 38000,
            },
        ]
    }

    // ──────────────────────────────────────────────────────────
    // 百度 MCP 广场（百度智能云千帆）
    // ──────────────────────────────────────────────────────────
    fn baidu_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "baidu-wenxin".into(), name: "文心一言".into(),
                description: "调用文心一言 ERNIE 4.5/X1 大模型".into(),
                provider: McpProvider::BaiduMcp,
                url: "https://mcp.bce.baidu.com/server/wenxin".into(),
                category: "AI/LLM".into(), installs: 78000,
            },
            DomesticMcpServer {
                id: "baidu-search".into(), name: "百度搜索".into(),
                description: "调用百度搜索接口（含 AI 摘要）".into(),
                provider: McpProvider::BaiduMcp,
                url: "https://mcp.bce.baidu.com/server/search".into(),
                category: "Search".into(), installs: 96000,
            },
            DomesticMcpServer {
                id: "baidu-map".into(), name: "百度地图".into(),
                description: "POI 检索 / 路径规划 / 逆地理编码".into(),
                provider: McpProvider::BaiduMcp,
                url: "https://mcp.bce.baidu.com/server/map".into(),
                category: "Map".into(), installs: 56000,
            },
            DomesticMcpServer {
                id: "baidu-ocr".into(), name: "百度 OCR".into(),
                description: "通用文字识别、表格识别、卡证识别".into(),
                provider: McpProvider::BaiduMcp,
                url: "https://mcp.bce.baidu.com/server/ocr".into(),
                category: "AI/Vision".into(), installs: 41000,
            },
            DomesticMcpServer {
                id: "baidu-bos".into(), name: "百度 BOS 存储".into(),
                description: "百度对象存储读写".into(),
                provider: McpProvider::BaiduMcp,
                url: "https://mcp.bce.baidu.com/server/bos".into(),
                category: "Storage".into(), installs: 23000,
            },
        ]
    }

    // ──────────────────────────────────────────────────────────
    // Higress MCP Marketplace（阿里 Higress 网关托管）
    // ──────────────────────────────────────────────────────────
    fn higress_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "higress-amap".into(), name: "高德地图".into(),
                description: "Higress 托管的高德地图 MCP（POI/路线/天气）".into(),
                provider: McpProvider::Higress,
                url: "https://mcp.higress.ai/mcp/amap".into(),
                category: "Map".into(), installs: 67000,
            },
            DomesticMcpServer {
                id: "higress-arxiv".into(), name: "arXiv 论文搜索".into(),
                description: "搜索 arXiv 论文摘要 / 全文 PDF".into(),
                provider: McpProvider::Higress,
                url: "https://mcp.higress.ai/mcp/arxiv".into(),
                category: "Research".into(), installs: 35000,
            },
            DomesticMcpServer {
                id: "higress-ddg".into(), name: "DuckDuckGo 搜索".into(),
                description: "免代理的 DuckDuckGo 搜索代理".into(),
                provider: McpProvider::Higress,
                url: "https://mcp.higress.ai/mcp/duckduckgo".into(),
                category: "Search".into(), installs: 29000,
            },
            DomesticMcpServer {
                id: "higress-weather".into(), name: "全球天气".into(),
                description: "实时天气查询（含未来 7 天预报）".into(),
                provider: McpProvider::Higress,
                url: "https://mcp.higress.ai/mcp/weather".into(),
                category: "Utility".into(), installs: 24000,
            },
            DomesticMcpServer {
                id: "higress-12306".into(), name: "12306 火车票".into(),
                description: "查询 12306 余票 / 时刻表".into(),
                provider: McpProvider::Higress,
                url: "https://mcp.higress.ai/mcp/12306".into(),
                category: "Travel".into(), installs: 31000,
            },
        ]
    }

    // ──────────────────────────────────────────────────────────
    // PulseMCP（全球聚合站，国内可访问，含中文 MCP）
    // ──────────────────────────────────────────────────────────
    fn pulsemcp_servers() -> Vec<DomesticMcpServer> {
        vec![
            DomesticMcpServer {
                id: "pulse-notion".into(), name: "Notion".into(),
                description: "Notion 数据库 / 页面读写".into(),
                provider: McpProvider::PulseMcp,
                url: "https://www.pulsemcp.com/servers/notion".into(),
                category: "Productivity".into(), installs: 58000,
            },
            DomesticMcpServer {
                id: "pulse-slack".into(), name: "Slack".into(),
                description: "Slack 频道消息 / 用户管理".into(),
                provider: McpProvider::PulseMcp,
                url: "https://www.pulsemcp.com/servers/slack".into(),
                category: "Communication".into(), installs: 49000,
            },
            DomesticMcpServer {
                id: "pulse-sentry".into(), name: "Sentry".into(),
                description: "查询 Sentry 错误 / 性能事件".into(),
                provider: McpProvider::PulseMcp,
                url: "https://www.pulsemcp.com/servers/sentry".into(),
                category: "Observability".into(), installs: 27000,
            },
            DomesticMcpServer {
                id: "pulse-gdrive".into(), name: "Google Drive".into(),
                description: "读取 Google Drive 文件".into(),
                provider: McpProvider::PulseMcp,
                url: "https://www.pulsemcp.com/servers/gdrive".into(),
                category: "Storage".into(), installs: 33000,
            },
            DomesticMcpServer {
                id: "pulse-spotify".into(), name: "Spotify".into(),
                description: "Spotify 播放控制 / 歌单管理".into(),
                provider: McpProvider::PulseMcp,
                url: "https://www.pulsemcp.com/servers/spotify".into(),
                category: "Media".into(), installs: 19000,
            },
        ]
    }
}
