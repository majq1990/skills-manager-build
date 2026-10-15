//! 上传发布时的标签推荐与受控词表。
//!
//! 为什么需要它：企业端的标签搜索是「精确匹配单个标签」，容不下自由词；
//! 而手动一个个填标签的成本高到没人会填，结果就是传上去的一批 skill 全都没标签。
//! 这里的策略是把成本压到接近零——上传瞬间从 frontmatter、分类/目录名、
//! 名称与描述关键词推一版标签预填进弹窗，用户可删可改可留空。
//!
//! 预填只是起点，最终以用户在弹窗里确认的为准；确认后的标签由
//! [`crate::core::enterprise_api::EnterpriseApi::upload_skill`] 随包上传，
//! 服务端再把它写进发布产物的 frontmatter（否则重启重建 catalog 时会丢）。

/// 受控词表：候选标签收敛到这一小批，避免自由词把标签空间打散后
/// 精确匹配搜索再也搜不到东西。用户仍可少量自定义（见 [`normalize_tag`]）。
pub const CONTROLLED_VOCABULARY: &[&str] = &[
    "需求分析",
    "需求处理",
    "测试",
    "用例",
    "缺陷",
    "报表",
    "文档",
    "翻译",
    "代码审查",
    "重构",
    "部署",
    "运维",
    "安全",
    "数据",
    "可视化",
    "大屏",
    "会议",
    "周报",
    "审批",
    "数据库",
    "接口",
    "性能",
    "规范",
];

/// 分类/目录名到标签的映射。键按小写包含匹配，值按优先级排列。
const CATEGORY_HINTS: &[(&str, &[&str])] = &[
    ("requirement", &["需求分析", "需求处理"]),
    ("demand", &["需求分析", "需求处理"]),
    ("test", &["测试", "用例"]),
    ("qa", &["测试", "用例"]),
    ("bug", &["缺陷", "测试"]),
    ("report", &["报表", "数据"]),
    ("doc", &["文档"]),
    ("translate", &["翻译", "文档"]),
    ("review", &["代码审查"]),
    ("refactor", &["重构", "代码审查"]),
    ("deploy", &["部署", "运维"]),
    ("ops", &["运维", "部署"]),
    ("security", &["安全"]),
    ("data", &["数据", "报表"]),
    ("visual", &["可视化", "大屏"]),
    ("screen", &["大屏", "可视化"]),
    ("meeting", &["会议"]),
    ("weekly", &["周报", "文档"]),
    ("approval", &["审批"]),
    ("database", &["数据库", "数据"]),
    ("api", &["接口"]),
    ("performance", &["性能"]),
    ("standard", &["规范", "文档"]),
];

/// 名称/描述关键词到标签的映射（小写包含匹配）。
const KEYWORD_HINTS: &[(&str, &str)] = &[
    ("需求", "需求处理"),
    ("工单", "需求处理"),
    ("测试", "测试"),
    ("用例", "用例"),
    ("缺陷", "缺陷"),
    ("bug", "缺陷"),
    ("报表", "报表"),
    ("报告", "报表"),
    ("文档", "文档"),
    ("翻译", "翻译"),
    ("审查", "代码审查"),
    ("review", "代码审查"),
    ("重构", "重构"),
    ("部署", "部署"),
    ("发布", "部署"),
    ("运维", "运维"),
    ("安全", "安全"),
    ("漏洞", "安全"),
    ("数据", "数据"),
    ("可视化", "可视化"),
    ("大屏", "大屏"),
    ("会议", "会议"),
    ("周报", "周报"),
    ("审批", "审批"),
    ("数据库", "数据库"),
    ("sql", "数据库"),
    ("接口", "接口"),
    ("api", "接口"),
    ("性能", "性能"),
    ("规范", "规范"),
];

/// 单个标签的长度上限（服务端与企业端 UI 都是短标签场景）。
const MAX_TAG_LEN: usize = 20;

/// 归一化用户输入的一个标签：去空白、限长，并剔除服务端手写 YAML 解析器
/// 处理不了的字符（`, [ ] " '` 会让注入 frontmatter 的标签被静默写坏，
/// 见服务端 `normalizeTags`）。替换成空格而不是直接删除，否则
/// `报表,周报` 会拼成 `报表周报` 改变原意。返回 `None` 表示这个标签应当丢弃。
pub fn normalize_tag(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if matches!(c, ',' | '[' | ']' | '"' | '\'' | '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let truncated: String = trimmed.chars().take(MAX_TAG_LEN).collect();
    Some(truncated)
}

/// 归一化一组标签：逐个清洗、去重、保序。
pub fn normalize_tags<'a, I: IntoIterator<Item = &'a str>>(raw: I) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in raw {
        if let Some(tag) = normalize_tag(item) {
            if seen.insert(tag.clone()) {
                out.push(tag);
            }
        }
    }
    out
}

/// 推荐标签的输入。
pub struct TagSuggestionInput<'a> {
    /// 技能/agent 身份名（如 `product-manager-agent`）。
    pub name: &'a str,
    pub description: &'a str,
    /// 分类或来源目录名（如 `requirement`、`global`）。
    pub category: Option<&'a str>,
    /// 发布物 frontmatter 里已有的 tags。
    pub frontmatter_tags: &'a [String],
}

/// 推荐结果条数上限：标签是筛选用途，铺一排长串反而没法点。
const MAX_SUGGESTIONS: usize = 5;

/// 推一版标签。优先级：frontmatter 里已在受控词表中的标签 > 分类映射 >
/// 名称/描述关键词。都不命中时返回空列表（用户可自己填）。
pub fn suggest_tags(input: &TagSuggestionInput<'_>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |tag: &str| {
        if !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
    };

    // 1. frontmatter 已有的标签：作者自己标过的，可信度最高。
    //    只保留受控词表内的，自由词不进推荐（避免把噪声带进搜索空间）。
    for tag in input.frontmatter_tags {
        if is_controlled(tag) {
            push(tag);
        }
    }

    // 2. 分类/目录名映射
    if let Some(category) = input.category {
        let lower = category.to_lowercase();
        for (key, tags) in CATEGORY_HINTS {
            if lower.contains(key) {
                for tag in *tags {
                    push(tag);
                }
            }
        }
    }

    // 3. 名称 + 描述关键词
    let haystack = format!("{} {}", input.name, input.description).to_lowercase();
    for (key, tag) in KEYWORD_HINTS {
        if haystack.contains(key) {
            push(tag);
        }
    }

    out.truncate(MAX_SUGGESTIONS);
    out
}

/// 该标签是否在受控词表内（大小写不敏感）。
pub fn is_controlled(tag: &str) -> bool {
    let lower = tag.trim().to_lowercase();
    CONTROLLED_VOCABULARY
        .iter()
        .any(|c| c.to_lowercase() == lower)
}

/// 从发布物目录里读 frontmatter 的 tags（SKILL.md 或 AGENT.md）。
/// 文件缺失 / 无 frontmatter / tags 不是数组时返回空列表。
pub fn read_frontmatter_tags(dir: &std::path::Path) -> Vec<String> {
    for candidate in ["SKILL.md", "AGENT.md"] {
        let path = dir.join(candidate);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some((yaml, _)) = split_frontmatter(&content) else {
            continue;
        };
        let Some(tags) = yaml.get("tags") else {
            continue;
        };
        let collected: Vec<String> = match tags {
            serde_yaml::Value::Sequence(seq) => seq
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            serde_yaml::Value::String(s) => s
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
            _ => Vec::new(),
        };
        if !collected.is_empty() {
            return collected;
        }
    }
    Vec::new()
}

/// 与 [`crate::core::agent_variant::split_frontmatter`] 相同的切分规则：
/// 文件头一个 `---` 围栏块。这里不复用那个私有函数，避免为核心模块
/// 增加不必要的公开面。
fn split_frontmatter(content: &str) -> Option<(serde_yaml::Value, String)> {
    let trimmed = content.trim_start();
    let rest = trimmed.strip_prefix("---")?;
    let end = rest.find("---")?;
    let yaml_str = &rest[..end];
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(yaml_str).ok()?;
    let body = rest[end + 3..].trim_start_matches('\n').to_string();
    Some((yaml, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        name: &'a str,
        description: &'a str,
        category: Option<&'a str>,
        frontmatter_tags: &'a [String],
    ) -> TagSuggestionInput<'a> {
        TagSuggestionInput {
            name,
            description,
            category,
            frontmatter_tags,
        }
    }

    // ── normalize ──

    #[test]
    fn normalize_tag_strips_parser_hostile_characters() {
        // 这些字符会让服务端手写 YAML 解析器把标签写坏，必须在客户端就挡掉。
        // 规则与服务端 normalizeTags 保持一致：替换成空格后折叠，不是直接删除。
        assert_eq!(normalize_tag("报表,周报").as_deref(), Some("报表 周报"));
        assert_eq!(normalize_tag("[括号]").as_deref(), Some("括号"));
        assert_eq!(normalize_tag("含\"引号\"").as_deref(), Some("含 引号"));
        assert_eq!(normalize_tag("  需求处理  ").as_deref(), Some("需求处理"));
        assert_eq!(normalize_tag("报,表,,周,报").as_deref(), Some("报 表 周 报"));
    }

    #[test]
    fn normalize_tag_matches_server_sanitization() {
        // 客户端展示的标签必须与服务端最终存储的一致，否则用户会以为没生效
        for raw in ["报表,周报", "[括号]", "含\"引号\"", "  需求处理  ", "测试  用例"] {
            let client = normalize_tag(raw).unwrap_or_default();
            let server = raw
                .replace(|c| matches!(c, ',' | '[' | ']' | '"' | '\'' | '\n' | '\r' | '\t'), " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(client, server, "与服务端清洗结果不一致: {raw}");
        }
    }

    #[test]
    fn normalize_tag_rejects_empty_and_overlong() {
        assert_eq!(normalize_tag(""), None);
        assert_eq!(normalize_tag("   "), None);
        assert_eq!(normalize_tag(",,"), None);
        let long = "超".repeat(50);
        let out = normalize_tag(&long).expect("应截断而非丢弃");
        assert_eq!(out.chars().count(), MAX_TAG_LEN);
    }

    #[test]
    fn normalize_tags_dedups_and_keeps_order() {
        let out = normalize_tags(["测试", "测试", "用例", "  ", "报表"]);
        assert_eq!(out, vec!["测试", "用例", "报表"]);
    }

    // ── suggestion sources ──

    #[test]
    fn frontmatter_tags_in_vocabulary_win() {
        let tags = vec!["测试".to_string(), "自由词".to_string()];
        let out = suggest_tags(&input("whatever", "无关描述", None, &tags));
        assert_eq!(out, vec!["测试"], "自由词不应进推荐: {out:?}");
    }

    #[test]
    fn category_maps_to_tags() {
        let out = suggest_tags(&input("some-skill", "", Some("requirement"), &[]));
        assert!(out.contains(&"需求处理".to_string()), "{out:?}");
        assert!(out.contains(&"需求分析".to_string()), "{out:?}");
    }

    #[test]
    fn category_match_is_case_insensitive_and_substring() {
        let out = suggest_tags(&input("x", "", Some("QA-Tools"), &[]));
        assert!(out.contains(&"测试".to_string()), "{out:?}");
    }

    #[test]
    fn keywords_from_name_and_description() {
        let out = suggest_tags(&input("weekly-report", "自动生成本周工作周报", None, &[]));
        assert!(out.contains(&"周报".to_string()), "{out:?}");
    }

    #[test]
    fn suggestions_are_deduped_and_capped() {
        // 名称和描述同时命中一堆关键词，结果不能无限长
        let out = suggest_tags(
            &input(
                "test-bug-report-api-sql-security",
                "测试 缺陷 报表 接口 数据库 安全 部署 运维",
                Some("test"),
                &[],
            ),
        );
        assert!(out.len() <= MAX_SUGGESTIONS, "超过上限: {out:?}");
        let mut sorted = out.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), out.len(), "有重复: {out:?}");
    }

    #[test]
    fn no_signal_yields_empty() {
        let out = suggest_tags(&input("zzz-unknown", "nothing matching here", Some("global"), &[]));
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn every_suggested_tag_is_controlled() {
        let out = suggest_tags(
            &input("requirement-test-skill", "需求 测试 报表 安全", Some("requirement"), &[]),
        );
        for tag in &out {
            assert!(is_controlled(tag), "推荐出受控词表外的标签: {tag}");
        }
    }

    #[test]
    fn vocabulary_has_no_duplicates() {
        let mut sorted: Vec<&str> = CONTROLLED_VOCABULARY.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "受控词表有重复项");
    }

    #[test]
    fn vocabulary_tags_survive_normalization() {
        for tag in CONTROLLED_VOCABULARY {
            assert_eq!(
                normalize_tag(tag).as_deref(),
                Some(*tag),
                "受控词表标签被归一化改坏了: {tag}"
            );
        }
    }

    // ── frontmatter reading ──

    #[test]
    fn read_frontmatter_tags_sequence_and_string_forms() {
        let dir = std::env::temp_dir().join(format!("tag-suggest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("SKILL.md");

        std::fs::write(&md, "---\nname: a\ntags: [测试, 用例]\n---\nbody\n").unwrap();
        assert_eq!(read_frontmatter_tags(&dir), vec!["测试", "用例"]);

        std::fs::write(&md, "---\nname: a\ntags: 测试, 用例\n---\nbody\n").unwrap();
        assert_eq!(read_frontmatter_tags(&dir), vec!["测试", "用例"]);

        std::fs::write(&md, "---\nname: a\n---\nbody\n").unwrap();
        assert!(read_frontmatter_tags(&dir).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_frontmatter_tags_falls_back_to_agent_md() {
        let dir = std::env::temp_dir().join(format!("tag-suggest-agent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("AGENT.md"),
            "---\nname: a\ntags:\n  - 需求处理\n  - 数据分析\n---\nbody\n",
        )
        .unwrap();
        assert_eq!(read_frontmatter_tags(&dir), vec!["需求处理", "数据分析"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_frontmatter_tags_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join("tag-suggest-does-not-exist-xyz");
        assert!(read_frontmatter_tags(&dir).is_empty());
    }
}
