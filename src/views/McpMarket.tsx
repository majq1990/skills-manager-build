import { useState, useEffect, useMemo, useCallback } from "react";
import { Search, Loader2, Globe, Server, ExternalLink, Building2, Check, Copy, Download, Package } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "../utils";
import * as api from "../lib/tauri";
import type { DomesticMcpServer, McpServer, McpProvider, NpmMcpPackage } from "../lib/tauri";

type Tab = "domestic" | "registry" | "npm";
type DomesticFilter =
  | "all"
  | "mcpso"
  | "modelscope"
  | "baidu"
  | "higress"
  | "pulsemcp"
  | "aliyun"
  | "bytedance"
  | "tencent"
  | "dingtalk"
  | "gitee";

const TOP_DEFAULT_LIMIT = 30;

export function McpMarket() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>("domestic");

  // Domestic MCP state
  const [domesticServers, setDomesticServers] = useState<DomesticMcpServer[]>([]);
  const [domesticProviders, setDomesticProviders] = useState<McpProvider[]>([]);
  const [domesticFilter, setDomesticFilter] = useState<DomesticFilter>("all");
  const [domesticSearch, setDomesticSearch] = useState("");
  const [domesticLoading, setDomesticLoading] = useState(true);

  // Registry MCP state
  const [registryServers, setRegistryServers] = useState<McpServer[]>([]);
  const [registrySearch, setRegistrySearch] = useState("");
  const [registryLoading, setRegistryLoading] = useState(false);

  // npm MCP state
  const [npmPackages, setNpmPackages] = useState<NpmMcpPackage[]>([]);
  const [npmSearch, setNpmSearch] = useState("@ntruth/dbhub");
  const [npmLoading, setNpmLoading] = useState(false);

  const [installing, setInstalling] = useState<Set<string>>(new Set());

  // Load domestic MCP servers on mount or when filter changes (not for gitee)
  useEffect(() => {
    if (domesticFilter !== "gitee") {
      loadDomesticServers();
    } else {
      // Load Gitee trending when switching to gitee filter
      handleDomesticSearch();
    }
    loadDomesticProviders();
  }, [domesticFilter]);

  const loadDomesticServers = useCallback(async () => {
    setDomesticLoading(true);
    try {
      // 默认（"all"）使用聚合 Top 接口（按安装量降序），其他分类按 provider 过滤
      const servers =
        domesticFilter === "all"
          ? await api.searchTopMcp("", TOP_DEFAULT_LIMIT)
          : await api.listDomesticMcp(domesticFilter);
      setDomesticServers(servers);
    } catch (err) {
      console.error("Failed to load domestic MCP servers:", err);
      toast.error(t("mcp.market.error"));
    } finally {
      setDomesticLoading(false);
    }
  }, [domesticFilter, t]);

  const loadDomesticProviders = useCallback(async () => {
    try {
      const providers = await api.listDomesticMcpProviders();
      setDomesticProviders(providers);
    } catch (err) {
      console.error("Failed to load MCP providers:", err);
    }
  }, []);

  // Load registry servers when tab changes
  useEffect(() => {
    if (activeTab === "registry" && registryServers.length === 0) {
      loadRegistryServers();
    }
  }, [activeTab]);

  useEffect(() => {
    if (activeTab === "npm" && npmPackages.length === 0) {
      handleNpmSearch();
    }
  }, [activeTab]);

  const loadRegistryServers = useCallback(async () => {
    setRegistryLoading(true);
    try {
      const servers = await api.listMcpRegistry(1, 50);
      setRegistryServers(servers);
    } catch (err) {
      console.error("Failed to load MCP registry:", err);
      toast.error(t("mcp.market.error"));
    } finally {
      setRegistryLoading(false);
    }
  }, [t]);

  // Search handlers
  const handleDomesticSearch = useCallback(async () => {
    if (domesticFilter === "gitee") {
      // Gitee search
      setDomesticLoading(true);
      try {
        const servers = domesticSearch.trim()
          ? await api.searchGiteeSkills(domesticSearch, 30)
          : await api.fetchGiteeTrending(30);
        setDomesticServers(servers);
      } catch (err) {
        console.error("Failed to search Gitee:", err);
        toast.error(t("mcp.market.gitee_error"));
      } finally {
        setDomesticLoading(false);
      }
      return;
    }

    // Regular domestic MCP search
    if (!domesticSearch.trim()) {
      loadDomesticServers();
      return;
    }
    setDomesticLoading(true);
    try {
      // "all" 时跨 5 大市场聚合搜索；选择具体 provider 时仍走原接口
      const servers =
        domesticFilter === "all"
          ? await api.searchTopMcp(domesticSearch, 60)
          : await api.searchDomesticMcp(domesticSearch);
      setDomesticServers(servers);
    } catch (err) {
      console.error("Failed to search domestic MCP:", err);
      toast.error(t("mcp.market.error"));
    } finally {
      setDomesticLoading(false);
    }
  }, [domesticSearch, domesticFilter, loadDomesticServers, t]);

  const handleRegistrySearch = useCallback(async () => {
    if (!registrySearch.trim()) {
      loadRegistryServers();
      return;
    }
    setRegistryLoading(true);
    try {
      const servers = await api.searchMcpRegistry(registrySearch, 20);
      setRegistryServers(servers);
    } catch (err) {
      console.error("Failed to search MCP registry:", err);
      toast.error(t("mcp.market.error"));
    } finally {
      setRegistryLoading(false);
    }
  }, [registrySearch, loadRegistryServers, t]);

  const handleNpmSearch = useCallback(async () => {
    const query = npmSearch.trim();
    if (!query) {
      setNpmPackages([]);
      return;
    }
    setNpmLoading(true);
    try {
      const packages = await api.searchNpmMcp(query, 30);
      setNpmPackages(packages);
    } catch (err) {
      console.error("Failed to search npm MCP packages:", err);
      toast.error(t("mcp.market.error"));
    } finally {
      setNpmLoading(false);
    }
  }, [npmSearch, t]);

  // Filter domestic servers
  const filteredDomestic = useMemo(() => {
    let servers = domesticServers;
    // Only filter by provider for non-gitee filters (gitee results are already filtered by API)
    if (domesticFilter !== "all" && domesticFilter !== "gitee") {
      servers = servers.filter((s) => s.provider === domesticFilter);
    }
    // For gitee filter, search is handled by API, so only client-side filter if needed
    if (domesticSearch && domesticFilter !== "gitee") {
      const needle = domesticSearch.toLowerCase();
      servers = servers.filter(
        (s) =>
          s.name.toLowerCase().includes(needle) ||
          s.description.toLowerCase().includes(needle)
      );
    }
    return servers;
  }, [domesticServers, domesticFilter, domesticSearch]);

  // Filter registry servers
  const filteredRegistry = useMemo(() => {
    if (!registrySearch) return registryServers;
    const needle = registrySearch.toLowerCase();
    return registryServers.filter(
      (s) =>
        s.name.toLowerCase().includes(needle) ||
        (s.description?.toLowerCase().includes(needle) ?? false)
    );
  }, [registryServers, registrySearch]);

  const filteredNpmPackages = useMemo(() => {
    if (!npmSearch) return npmPackages;
    const needle = npmSearch.toLowerCase();
    return npmPackages.filter(
      (pkg) =>
        pkg.name.toLowerCase().includes(needle) ||
        (pkg.description?.toLowerCase().includes(needle) ?? false) ||
        pkg.keywords.some((kw) => kw.toLowerCase().includes(needle))
    );
  }, [npmPackages, npmSearch]);

  // Install domestic MCP directly into detected agent configs
  const handleInstallDomestic = async (server: DomesticMcpServer) => {
    const id = server.id;
    setInstalling((prev) => new Set(prev).add(id));
    try {
      const result = await api.installDomesticMcpDirect(server.id);
      try {
        await navigator.clipboard.writeText(result.config_snippet);
      } catch {
        // clipboard may be denied; ignore
      }
      if (result.written_targets.length > 0) {
        toast.success(
          `${server.name} 已写入：${result.written_targets.join("、")}（配置已复制到剪贴板）`
        );
      } else {
        toast.info(
          `${server.name} 未检测到受支持的客户端配置，配置已复制到剪贴板，请粘贴到 mcp.json 的 mcpServers 节点`
        );
      }
    } catch (err) {
      console.error("Failed to install domestic MCP:", err);
      toast.error(`${server.name} ${t("mcp.market.error")}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  // Copy registry MCP URL to clipboard
  const handleCopyRegistryUrl = async (server: McpServer) => {
    const id = server.name;
    setInstalling((prev) => new Set(prev).add(id));
    try {
      const config = await api.installRegistryMcp(server.name);
      await navigator.clipboard.writeText(config);
      toast.success(`${server.name} ${t("mcp.market.copied_to_clipboard")}`);
    } catch (err) {
      console.error("Failed to copy MCP config:", err);
      toast.error(`${server.name} ${t("mcp.market.copy_error")}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const handleInstallNpm = async (pkg: NpmMcpPackage) => {
    const id = pkg.name;
    setInstalling((prev) => new Set(prev).add(id));
    try {
      const result = await api.installNpmMcpDirect(pkg.name);
      try {
        await navigator.clipboard.writeText(result.config_snippet);
      } catch {
        // clipboard may be denied; ignore
      }
      if (result.written_targets.length > 0) {
        toast.success(
          `${pkg.name} 已写入：${result.written_targets.join("、")}（配置已复制到剪贴板）`
        );
      } else {
        toast.info(
          `${pkg.name} 未检测到受支持的客户端配置，配置已复制到剪贴板，请粘贴到 mcp.json 的 mcpServers 节点`
        );
      }
    } catch (err) {
      console.error("Failed to install npm MCP:", err);
      toast.error(`${pkg.name} ${t("mcp.market.error")}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const getProviderLabel = (providerId: string) => {
    return domesticProviders.find((p) => p.id === providerId)?.name || providerId;
  };

  const getProviderIcon = (providerId: string) => {
    const icons: Record<string, string> = {
      mcpso: "🛒",
      modelscope: "🧩",
      baidu: "🐾",
      higress: "🚀",
      pulsemcp: "💓",
      aliyun: "🔷",
      bytedance: "🎵",
      tencent: "💬",
      dingtalk: "📱",
      gitee: "🐦",
    };
    return icons[providerId] || "🏢";
  };

  return (
    <div className="app-page">
      <div className="app-page-header">
        <h1 className="app-page-title">{t("mcp.market.title")}</h1>
        <p className="app-page-subtitle text-tertiary">{t("mcp.market.description")}</p>
      </div>

      {/* Tabs */}
      <div className="app-segmented mb-4">
        {([
          { key: "domestic", label: t("mcp.market.tab_domestic"), icon: Building2 },
          { key: "registry", label: t("mcp.market.tab_registry"), icon: Globe },
          { key: "npm", label: t("mcp.market.tab_npm"), icon: Package },
        ] as const).map(({ key, label, icon: Icon }) => (
          <button
            key={key}
            onClick={() => setActiveTab(key)}
            className={cn(
              "app-segmented-button flex items-center gap-1.5",
              activeTab === key && "app-segmented-button-active"
            )}
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
          </button>
        ))}
      </div>

      {/* Domestic MCP Tab */}
      {activeTab === "domestic" && (
        <>
          {/* Provider Filter */}
          <div className="app-toolbar mb-3">
            <div className="relative w-full max-w-[280px]">
              <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                type="text"
                value={domesticSearch}
                onChange={(e) => setDomesticSearch(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleDomesticSearch()}
                placeholder={domesticFilter === "gitee"
                  ? t("mcp.market.gitee_search_placeholder")
                  : t("mcp.market.search_placeholder")}
                className="app-input w-full bg-background pl-9"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </div>

            <div className="app-segmented">
              <button
                onClick={() => setDomesticFilter("all")}
                className={cn(
                  "app-segmented-button",
                  domesticFilter === "all" && "app-segmented-button-active"
                )}
              >
                {t("mcp.market.filter.all")}
              </button>
              {domesticProviders.map((provider) => (
                <button
                  key={provider.id}
                  onClick={() => setDomesticFilter(provider.id as DomesticFilter)}
                  className={cn(
                    "app-segmented-button",
                    domesticFilter === provider.id && "app-segmented-button-active"
                  )}
                  title={provider.name}
                >
                  {provider.id === "gitee" ? "🐦" : getProviderIcon(provider.id)}
                </button>
              ))}
            </div>
          </div>

          {/* Loading */}
          {domesticLoading && (
            <div className="flex flex-1 items-center justify-center py-20">
              <Loader2 className="h-6 w-6 animate-spin text-muted" />
              <span className="ml-2 text-[13px] text-muted">{t("mcp.market.loading")}</span>
            </div>
          )}

          {/* Empty */}
          {!domesticLoading && filteredDomestic.length === 0 && (
            <div className="flex flex-1 flex-col items-center justify-center py-20 text-center">
              <Search className="mb-4 h-12 w-12 text-faint" />
              <h3 className="mb-1.5 text-[14px] font-semibold text-tertiary">
                {t("mcp.market.no_results")}
              </h3>
            </div>
          )}

          {/* Grid */}
          {!domesticLoading && filteredDomestic.length > 0 && (
            <div className="grid grid-cols-2 gap-2.5 lg:grid-cols-3">
              {filteredDomestic.map((server) => (
                <DomesticMcpCard
                  key={server.id}
                  server={server}
                  providerLabel={getProviderLabel(server.provider)}
                  providerIcon={getProviderIcon(server.provider)}
                  isInstalling={installing.has(server.id)}
                  onInstall={() => handleInstallDomestic(server)}
                />
              ))}
            </div>
          )}
        </>
      )}

      {/* Registry MCP Tab */}
      {activeTab === "registry" && (
        <>
          {/* Search */}
          <div className="app-toolbar mb-3">
            <div className="relative w-full max-w-[400px]">
              <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                type="text"
                value={registrySearch}
                onChange={(e) => setRegistrySearch(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleRegistrySearch()}
                placeholder={t("mcp.market.search_registry_placeholder")}
                className="app-input w-full bg-background pl-9"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </div>
          </div>

          {/* Loading */}
          {registryLoading && (
            <div className="flex flex-1 items-center justify-center py-20">
              <Loader2 className="h-6 w-6 animate-spin text-muted" />
              <span className="ml-2 text-[13px] text-muted">{t("mcp.market.loading")}</span>
            </div>
          )}

          {/* Empty */}
          {!registryLoading && filteredRegistry.length === 0 && (
            <div className="flex flex-1 flex-col items-center justify-center py-20 text-center">
              <Search className="mb-4 h-12 w-12 text-faint" />
              <h3 className="mb-1.5 text-[14px] font-semibold text-tertiary">
                {t("mcp.market.no_results")}
              </h3>
            </div>
          )}

          {/* Grid */}
          {!registryLoading && filteredRegistry.length > 0 && (
            <div className="grid grid-cols-2 gap-2.5 lg:grid-cols-3">
              {filteredRegistry.map((server) => (
                <RegistryMcpCard
                  key={server.name}
                  server={server}
                  isCopying={installing.has(server.name)}
                  onCopyUrl={() => handleCopyRegistryUrl(server)}
                />
              ))}
            </div>
          )}
        </>
      )}

      {/* npm MCP Tab */}
      {activeTab === "npm" && (
        <>
          <div className="app-toolbar mb-3">
            <div className="relative w-full max-w-[400px]">
              <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                type="text"
                value={npmSearch}
                onChange={(e) => setNpmSearch(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleNpmSearch()}
                placeholder={t("mcp.market.search_npm_placeholder")}
                className="app-input w-full bg-background pl-9"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </div>
            <button onClick={handleNpmSearch} className="app-button-secondary">
              <Search className="h-3.5 w-3.5" />
              {t("common.search")}
            </button>
          </div>

          {npmLoading && (
            <div className="flex flex-1 items-center justify-center py-20">
              <Loader2 className="h-6 w-6 animate-spin text-muted" />
              <span className="ml-2 text-[13px] text-muted">{t("mcp.market.loading")}</span>
            </div>
          )}

          {!npmLoading && filteredNpmPackages.length === 0 && (
            <div className="flex flex-1 flex-col items-center justify-center py-20 text-center">
              <Search className="mb-4 h-12 w-12 text-faint" />
              <h3 className="mb-1.5 text-[14px] font-semibold text-tertiary">
                {t("mcp.market.no_results")}
              </h3>
            </div>
          )}

          {!npmLoading && filteredNpmPackages.length > 0 && (
            <div className="grid grid-cols-2 gap-2.5 lg:grid-cols-3">
              {filteredNpmPackages.map((pkg) => (
                <NpmMcpCard
                  key={pkg.name}
                  pkg={pkg}
                  isInstalling={installing.has(pkg.name)}
                  onInstall={() => handleInstallNpm(pkg)}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

// Domestic MCP Card Component
function DomesticMcpCard({
  server,
  providerLabel,
  providerIcon,
  isInstalling,
  onInstall,
}: {
  server: DomesticMcpServer;
  providerLabel: string;
  providerIcon: string;
  isInstalling: boolean;
  onInstall: () => void;
}) {
  const { t } = useTranslation();
  const isGitee = server.provider === "gitee";

  const handleOpenRepo = () => {
    window.open(server.url, "_blank");
  };

  return (
    <div className="app-panel flex flex-col gap-2 p-3 transition-colors hover:border-border hover:bg-surface-hover">
      {/* Name & Provider */}
      <div className="flex items-start justify-between gap-2">
        <h3 className="truncate text-[14px] font-semibold text-primary" title={server.name}>
          {server.name}
        </h3>
        <span
          className="shrink-0 rounded-full bg-surface-hover px-2 py-0.5 text-[11px] font-medium text-muted"
          title={providerLabel}
        >
          {providerIcon} {providerLabel}
        </span>
      </div>

      {/* Description */}
      <p className="line-clamp-2 text-[13px] leading-[18px] text-muted" title={server.description}>
        {server.description}
      </p>

      {/* Meta */}
      <div className="flex items-center gap-2 text-[12px] text-muted">
        <span className="rounded bg-surface-hover px-1.5 py-0.5 font-mono text-[11px] text-secondary">
          {server.provider}
        </span>
      </div>

      {/* Action */}
      <div className="mt-auto pt-1">
        {isGitee ? (
          <button
            onClick={handleOpenRepo}
            className="app-button-secondary w-full"
          >
            <ExternalLink className="h-3.5 w-3.5" />
            {t("mcp.market.open_repo")}
          </button>
        ) : (
          <button
            onClick={onInstall}
            disabled={isInstalling}
            className="app-button-primary w-full"
          >
            {isInstalling ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t("mcp.market.installing")}
              </>
            ) : (
              <>
                <Download className="h-3.5 w-3.5" />
                {t("mcp.market.one_click_install")}
              </>
            )}
          </button>
        )}
      </div>
    </div>
  );
}

function NpmMcpCard({
  pkg,
  isInstalling,
  onInstall,
}: {
  pkg: NpmMcpPackage;
  isInstalling: boolean;
  onInstall: () => void;
}) {
  const { t } = useTranslation();

  const openLink = (url: string | null) => {
    if (url) {
      window.open(url, "_blank");
    }
  };

  return (
    <div className="app-panel flex flex-col gap-2 p-3 transition-colors hover:border-border hover:bg-surface-hover">
      <div className="flex items-start justify-between gap-2">
        <h3 className="truncate font-mono text-[13px] font-semibold text-primary" title={pkg.name}>
          {pkg.name}
        </h3>
        <span className="shrink-0 rounded-full bg-surface-hover px-2 py-0.5 text-[11px] font-medium text-muted">
          npm
        </span>
      </div>

      <p className="line-clamp-2 text-[13px] leading-[18px] text-muted" title={pkg.description || "No description"}>
        {pkg.description || "No description available"}
      </p>

      <div className="flex flex-wrap items-center gap-1.5 text-[12px] text-muted">
        <span className="rounded bg-surface-hover px-1.5 py-0.5 font-mono text-[11px] text-secondary">
          v{pkg.version}
        </span>
        {typeof pkg.weekly_downloads === "number" && (
          <span className="rounded bg-surface-hover px-1.5 py-0.5 text-[11px] text-secondary">
            {pkg.weekly_downloads.toLocaleString()} / week
          </span>
        )}
      </div>

      <div className="flex gap-2 text-[12px]">
        <button
          onClick={() => openLink(pkg.npm_url)}
          className="text-muted transition-colors hover:text-primary"
        >
          npm
        </button>
        {pkg.repository_url && (
          <button
            onClick={() => openLink(pkg.repository_url)}
            className="text-muted transition-colors hover:text-primary"
          >
            repo
          </button>
        )}
      </div>

      <div className="mt-auto pt-1">
        <button
          onClick={onInstall}
          disabled={isInstalling}
          className="app-button-primary w-full"
        >
          {isInstalling ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("mcp.market.installing")}
            </>
          ) : (
            <>
              <Download className="h-3.5 w-3.5" />
              {t("mcp.market.one_click_install")}
            </>
          )}
        </button>
      </div>
    </div>
  );
}

// Registry MCP Card Component
function RegistryMcpCard({
  server,
  isCopying,
  onCopyUrl,
}: {
  server: McpServer;
  isCopying: boolean;
  onCopyUrl: () => void;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await onCopyUrl();
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="app-panel flex flex-col gap-2 p-3 transition-colors hover:border-border hover:bg-surface-hover">
      {/* Name */}
      <div className="flex items-start justify-between gap-2">
        <h3 className="truncate text-[14px] font-semibold text-primary" title={server.name}>
          {server.name}
        </h3>
        <span className="shrink-0 rounded-full bg-blue-500/12 px-2 py-0.5 text-[11px] font-medium text-blue-600 dark:text-blue-400">
          MCP
        </span>
      </div>

      {/* Description */}
      <p
        className="line-clamp-2 text-[13px] leading-[18px] text-muted"
        title={server.description || "No description"}
      >
        {server.description || "No description available"}
      </p>

      {/* URL */}
      <div className="flex items-center gap-1 text-[12px] text-muted">
        <Server className="h-3 w-3" />
        <span className="truncate" title={server.remotes[0]?.url || "No URL"}>
          {server.remotes[0]?.url || "No URL"}
        </span>
      </div>

      {/* Action */}
      <div className="mt-auto pt-1">
        <button
          onClick={handleCopy}
          disabled={isCopying}
          className={cn(
            "w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-[5px] text-[13px] font-medium transition-colors",
            copied
              ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
              : "bg-surface-hover hover:bg-surface-active text-secondary hover:text-primary"
          )}
        >
          {isCopying ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : copied ? (
            <Check className="h-3.5 w-3.5" />
          ) : (
            <Copy className="h-3.5 w-3.5" />
          )}
          {copied ? t("mcp.market.copied") : t("mcp.market.copy_config")}
        </button>
      </div>
    </div>
  );
}
