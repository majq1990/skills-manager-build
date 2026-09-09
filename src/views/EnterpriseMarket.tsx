import { useState, useEffect, useMemo } from "react";
import {
  Building2,
  Search,
  Tag,
  X,
  Loader2,
  Download,
  LogIn,
  LogOut,
  Shield,
  RefreshCw,
  CheckCircle2,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "../utils";
import * as api from "../lib/tauri";
import type { EnterpriseSkill } from "../lib/tauri";
import { DetailSheet } from "../components/DetailSheet";
import { useApp } from "../context/AppContext";

/** 统一把后端错误归一成字符串（替代散落的 `(err as any)?.message`）。 */
const errMsg = (err: unknown): string =>
  String(err instanceof Error ? err.message : err ?? "");

export function EnterpriseMarket() {
  const { t } = useTranslation();
  const { enterpriseUploadVisibilities, setEnterpriseUploadVisibilities } = useApp();
  const canDeleteEnterpriseSkills = enterpriseUploadVisibilities.length > 0;
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [loading, setLoading] = useState(false);
  const [skills, setSkills] = useState<EnterpriseSkill[]>([]);
  const [allTags, setAllTags] = useState<string[]>([]);
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [showLogin, setShowLogin] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [loginLoading, setLoginLoading] = useState(false);
  const [selectedSkill, setSelectedSkill] = useState<EnterpriseSkill | null>(null);
  const [installing, setInstalling] = useState<Set<string>>(new Set());
  const [deleting, setDeleting] = useState(false);
  // 本地已安装技能名（归一化小写），用于把企业市场里"本地已有"的技能标成已安装/更新
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());
  // 企业 Agent 市场（服务器端 agent 存储）
  const [marketView, setMarketView] = useState<"skills" | "agents">("skills");
  const [agents, setAgents] = useState<EnterpriseSkill[]>([]);
  const [installedAgentNames, setInstalledAgentNames] = useState<Set<string>>(new Set());
  const [agentInstalling, setAgentInstalling] = useState<Set<string>>(new Set());

  useEffect(() => {
    checkAuth();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const checkAuth = async () => {
    try {
      const authed = await api.enterpriseIsAuthenticated();
      setIsAuthenticated(authed);
      if (authed) {
        loadSkills();
        loadTags();
        loadInstalled();
        loadAgents();
        loadInstalledAgents();
      }
    } catch {
      setIsAuthenticated(false);
    }
  };

  const loadSkills = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.enterpriseListSkills();
      setSkills(data);
    } catch (err) {
      setError(String(err));
      toast.error(t("enterprise.loadError"));
    } finally {
      setLoading(false);
    }
  };

  const loadTags = async () => {
    try {
      const tags = await api.enterpriseGetTags();
      setAllTags(tags);
    } catch {
      // 标签拉取失败不阻塞主流程
    }
  };

  // 拉本地受管技能名集合（企业安装走的就是入受管库，所以这是"已安装"的权威来源）
  const loadInstalled = async () => {
    try {
      const managed = await api.getManagedSkills();
      setInstalledNames(new Set(managed.map((s) => s.name.trim().toLowerCase())));
    } catch {
      // 本地受管列表拿不到时忽略
    }
  };

  // 企业 Agent 市场：列表 / 本地已装 / 安装
  const loadAgents = async () => {
    setLoading(true);
    setError(null);
    try {
      setAgents(await api.enterpriseListAgents());
    } catch (err) {
      setError(String(err));
      toast.error(t("enterprise.loadError"));
    } finally {
      setLoading(false);
    }
  };

  const loadInstalledAgents = async () => {
    try {
      const managed = await api.getAgents();
      setInstalledAgentNames(new Set(managed.map((a) => a.name.trim().toLowerCase())));
    } catch {
      // 本地 agent 列表拿不到时忽略
    }
  };

  const handleInstallAgent = async (agent: EnterpriseSkill) => {
    if (agentInstalling.has(agent.name)) return;
    setAgentInstalling((prev) => new Set(prev).add(agent.name));
    try {
      await api.enterpriseInstallAgent(agent.name, agent.version);
      toast.success(`${agent.name} ${t("enterprise.installed")}`);
      setInstalledAgentNames((prev) => new Set(prev).add(agent.name.trim().toLowerCase()));
    } catch (err) {
      const msg = errMsg(err);
      toast.error(`${agent.name} ${t("enterprise.installFailed")}: ${msg}`);
    } finally {
      setAgentInstalling((prev) => {
        const next = new Set(prev);
        next.delete(agent.name);
        return next;
      });
    }
  };

  const handleLogin = async () => {
    if (!username || !password) {
      toast.error(t("enterprise.loginRequired"));
      return;
    }
    setLoginLoading(true);
    try {
      const result = await api.enterpriseLogin(username, password);
      if (result.success) {
        setIsAuthenticated(true);
        setShowLogin(false);
        setUsername("");
        setPassword("");
        // 把当前用户可上传的可见性级别存入全局态，供发布对话框读取
        setEnterpriseUploadVisibilities(result.uploadVisibilities ?? []);
        toast.success(t("enterprise.loginSuccess"));
        loadSkills();
        loadTags();
        loadInstalled();
        loadAgents();
        loadInstalledAgents();
      }
    } catch (err) {
      // 后端错误分类：连接失败 / 凭证错(401) / 服务器响应异常，避免一律误报为"账号密码错误"
      const msg = errMsg(err);
      if (/connect|dns|timed out|timeout|tcp|network|refused/i.test(msg)) {
        toast.error(t("enterprise.loginConnError"));
      } else if (/\(401\)|invalid username or password|authentication failed/i.test(msg)) {
        toast.error(t("enterprise.loginFailed"));
      } else if (/parse|deserialize|missing field|invalid type/i.test(msg)) {
        toast.error(t("enterprise.loginServerError"));
      } else {
        toast.error(t("enterprise.loginFailed"));
      }
    } finally {
      setLoginLoading(false);
    }
  };

  const handleInstall = async (e: React.MouseEvent, skill: EnterpriseSkill) => {
    e.stopPropagation(); // 阻止冒泡到卡片，避免点安装却打开详情
    if (installing.has(skill.name)) return;
    setInstalling((prev) => new Set(prev).add(skill.name));
    try {
      await api.enterpriseInstallSkill(skill.name, skill.version);
      toast.success(`${skill.name} ${t("enterprise.installed")}`);
      // 装好后即时标记为已安装（无需等下次刷新）
      setInstalledNames((prev) => new Set(prev).add(skill.name.trim().toLowerCase()));
    } catch (err) {
      const msg = errMsg(err);
      toast.error(`${skill.name} ${t("enterprise.installFailed")}: ${msg}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(skill.name);
        return next;
      });
    }
  };

  const handleDelete = async () => {
    if (!selectedSkill || deleting) return;
    if (!window.confirm(t("enterprise.deleteConfirm", { name: selectedSkill.name }))) return;
    setDeleting(true);
    try {
      await api.enterpriseDeleteSkill(selectedSkill.name);
      const deletedName = selectedSkill.name;
      setSkills((prev) => prev.filter((skill) => skill.name !== deletedName));
      setSelectedSkill(null);
      toast.success(t("enterprise.deleted", { name: deletedName }));
    } catch (err) {
      toast.error(`${t("enterprise.deleteFailed")}: ${errMsg(err)}`);
    } finally {
      setDeleting(false);
    }
  };

  const handleLogout = async () => {
    try {
      await api.enterpriseLogout();
      setIsAuthenticated(false);
      setSkills([]);
      setAllTags([]);
      setSelectedTags(new Set());
      setEnterpriseUploadVisibilities([]);
      toast.success(t("enterprise.logoutSuccess"));
    } catch {
      toast.error(t("enterprise.logoutFailed"));
    }
  };

  const toggleTag = (tag: string) => {
    setSelectedTags((prev) => {
      const next = new Set(prev);
      if (next.has(tag)) {
        next.delete(tag);
      } else {
        next.add(tag);
      }
      return next;
    });
  };

  const clearTags = () => {
    setSelectedTags(new Set());
  };

  const filteredSkills = useMemo(() => {
    let result = skills;

    if (searchQuery) {
      const query = searchQuery.toLowerCase();
      result = result.filter(
        (s) =>
          s.name.toLowerCase().includes(query) ||
          s.description.toLowerCase().includes(query) ||
          s.tags.some((t) => t.toLowerCase().includes(query))
      );
    }

    if (selectedTags.size > 0) {
      result = result.filter((s) =>
        Array.from(selectedTags).every((tag) => s.tags.includes(tag))
      );
    }

    return result;
  }, [skills, searchQuery, selectedTags]);

  if (!isAuthenticated) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[60vh] gap-6">
        <div className="flex flex-col items-center gap-4">
          <Building2 className="w-16 h-16 text-[var(--color-text-tertiary)]" />
          <h2 className="text-2xl font-semibold text-[var(--color-text-primary)]">
            {t("enterprise.title")}
          </h2>
          <p className="text-[var(--color-text-secondary)] text-center max-w-md">
            {t("enterprise.loginDescription")}
          </p>
        </div>

        {showLogin ? (
          <div className="w-full max-w-sm space-y-4">
            <div>
              <label className="block text-sm font-medium text-[var(--color-text-secondary)] mb-1">
                {t("enterprise.username")}
              </label>
              <input
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="w-full px-3 py-2 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-lg text-[var(--color-text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)]"
                placeholder={t("enterprise.usernamePlaceholder")}
                onKeyDown={(e) => e.key === "Enter" && handleLogin()}
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-[var(--color-text-secondary)] mb-1">
                {t("enterprise.password")}
              </label>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="w-full px-3 py-2 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-lg text-[var(--color-text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)]"
                placeholder={t("enterprise.passwordPlaceholder")}
                onKeyDown={(e) => e.key === "Enter" && handleLogin()}
              />
            </div>
            <div className="flex gap-2">
              <button
                onClick={handleLogin}
                disabled={loginLoading}
                className="flex-1 flex items-center justify-center gap-2 px-4 py-2 bg-[var(--color-accent)] text-white rounded-lg hover:opacity-90 disabled:opacity-50"
              >
                {loginLoading ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : (
                  <LogIn className="w-4 h-4" />
                )}
                {t("enterprise.login")}
              </button>
              <button
                onClick={() => setShowLogin(false)}
                className="px-4 py-2 border border-[var(--color-border)] rounded-lg text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]"
              >
                {t("common.cancel")}
              </button>
            </div>
          </div>
        ) : (
          <button
            onClick={() => setShowLogin(true)}
            className="flex items-center gap-2 px-6 py-3 bg-[var(--color-accent)] text-white rounded-lg hover:opacity-90"
          >
            <LogIn className="w-5 h-5" />
            {t("enterprise.login")}
          </button>
        )}
      </div>
    );
  }

  // Authenticated view — 企业 Agent 市场（服务器端 agent 存储，与技能市场同构）
  if (marketView === "agents") {
    return (
      <div className="flex flex-col h-full">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <div className="flex items-center gap-3">
            <Building2 className="w-6 h-6 text-[var(--color-accent)]" />
            <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">
              {t("enterprise.title")}
            </h1>
            <div className="flex items-center gap-1 rounded-lg border border-[var(--color-border)] p-0.5">
              <button
                onClick={() => setMarketView("skills")}
                className="px-3 py-1 text-sm rounded-md text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] transition-colors"
              >
                {t("enterprise.skillsTab")}
              </button>
              <button
                onClick={() => setMarketView("agents")}
                className="px-3 py-1 text-sm rounded-md bg-[var(--color-accent)] text-white transition-colors"
              >
                {t("enterprise.agentsTab")}
              </button>
            </div>
            <span className="text-sm text-[var(--color-text-tertiary)]">
              {t("enterprise.agentCount", { count: agents.length })}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                loadAgents();
                loadInstalledAgents();
              }}
              className="p-2 rounded-lg hover:bg-[var(--color-surface-hover)] text-[var(--color-text-secondary)]"
              title={t("common.refresh")}
            >
              <RefreshCw className="w-4 h-4" />
            </button>
            <button
              onClick={handleLogout}
              className="flex items-center gap-2 px-3 py-1.5 text-sm border border-[var(--color-border)] rounded-lg text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]"
            >
              <LogOut className="w-4 h-4" />
              {t("enterprise.logout")}
            </button>
          </div>
        </div>

        {/* Agent 列表 */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          {agents.length === 0 ? (
            <p className="text-sm text-[var(--color-text-tertiary)] text-center py-12">
              {t("enterprise.agentsEmpty")}
            </p>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
              {agents.map((agent) => {
                const installed = installedAgentNames.has(agent.name.trim().toLowerCase());
                const installing = agentInstalling.has(agent.name);
                return (
                  <div
                    key={agent.name}
                    className="flex flex-col rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] p-4 transition-colors hover:bg-[var(--color-surface-hover)]"
                  >
                    <div className="mb-1 flex items-center justify-between gap-2">
                      <span className="truncate font-medium text-[var(--color-text-primary)]">
                        {agent.name}
                      </span>
                      <span className="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold text-[var(--color-text-tertiary)] bg-[var(--color-surface-active)]">
                        v{agent.version}
                      </span>
                    </div>
                    <p className="mb-2 line-clamp-2 min-h-[2rem] flex-1 text-[12px] text-[var(--color-text-secondary)]">
                      {agent.description || "—"}
                    </p>
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-[11px] text-[var(--color-text-tertiary)]">
                        {agent.visibility}
                      </span>
                      <button
                        disabled={installing || installed}
                        onClick={() => void handleInstallAgent(agent)}
                        className="shrink-0 rounded-lg px-3 py-1.5 text-[12px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed bg-[var(--color-accent)] text-white hover:opacity-90"
                      >
                        {installing ? (
                          <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        ) : installed ? (
                          t("enterprise.installed")
                        ) : (
                          t("enterprise.install")
                        )}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    );
  }

  // Authenticated view — 企业技能市场
  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
        <div className="flex items-center gap-3">
          <Building2 className="w-6 h-6 text-[var(--color-accent)]" />
          <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">
            {t("enterprise.title")}
          </h1>
          <div className="flex items-center gap-1 rounded-lg border border-[var(--color-border)] p-0.5">
            <button
              onClick={() => setMarketView("skills")}
              className="px-3 py-1 text-sm rounded-md bg-[var(--color-accent)] text-white transition-colors"
            >
              {t("enterprise.skillsTab")}
            </button>
            <button
              onClick={() => setMarketView("agents")}
              className="px-3 py-1 text-sm rounded-md text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] transition-colors"
            >
              {t("enterprise.agentsTab")}
            </button>
          </div>
          <span className="text-sm text-[var(--color-text-tertiary)]">
            {t("enterprise.skillCount", { count: filteredSkills.length })}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => {
              loadSkills();
              loadTags();
              loadInstalled();
            }}
            className="p-2 rounded-lg hover:bg-[var(--color-surface-hover)] text-[var(--color-text-secondary)]"
            title={t("common.refresh")}
          >
            <RefreshCw className="w-4 h-4" />
          </button>
          <button
            onClick={handleLogout}
            className="flex items-center gap-2 px-3 py-1.5 text-sm border border-[var(--color-border)] rounded-lg text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]"
          >
            <LogOut className="w-4 h-4" />
            {t("enterprise.logout")}
          </button>
        </div>
      </div>

      {/* Search and Filters */}
      <div className="px-6 py-4 space-y-3">
        {/* Search */}
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--color-text-tertiary)]" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-10 pr-4 py-2 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-lg text-[var(--color-text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)]"
            placeholder={t("enterprise.searchPlaceholder")}
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery("")}
              className="absolute right-3 top-1/2 -translate-y-1/2"
            >
              <X className="w-4 h-4 text-[var(--color-text-tertiary)]" />
            </button>
          )}
        </div>

        {/* Tags */}
        {allTags.length > 0 && (
          <div className="flex flex-wrap gap-2 items-center">
            <Tag className="w-4 h-4 text-[var(--color-text-tertiary)]" />
            {allTags.map((tag) => (
              <button
                key={tag}
                onClick={() => toggleTag(tag)}
                className={cn(
                  "px-2.5 py-1 text-xs rounded-full border transition-colors",
                  selectedTags.has(tag)
                    ? "bg-[var(--color-accent)] text-white border-[var(--color-accent)]"
                    : "border-[var(--color-border)] text-[var(--color-text-secondary)] hover:border-[var(--color-accent)]"
                )}
              >
                {tag}
              </button>
            ))}
            {selectedTags.size > 0 && (
              <button
                onClick={clearTags}
                className="px-2 py-1 text-xs text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]"
              >
                {t("enterprise.clearFilters")}
              </button>
            )}
          </div>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto px-6 pb-6">
        {loading ? (
          <div className="flex items-center justify-center py-20">
            <Loader2 className="w-8 h-8 animate-spin text-[var(--color-accent)]" />
          </div>
        ) : error ? (
          <div className="flex flex-col items-center justify-center py-20 gap-4">
            <p className="text-red-500">{error}</p>
            <button
              onClick={loadSkills}
              className="px-4 py-2 bg-[var(--color-accent)] text-white rounded-lg"
            >
              {t("common.retry")}
            </button>
          </div>
        ) : filteredSkills.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 gap-4">
            <Shield className="w-12 h-12 text-[var(--color-text-tertiary)]" />
            <p className="text-[var(--color-text-secondary)]">
              {searchQuery || selectedTags.size > 0
                ? t("enterprise.noResults")
                : t("enterprise.noSkills")}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {filteredSkills.map((skill) => (
              <div
                key={skill.name}
                onClick={() => setSelectedSkill(skill)}
                className="p-4 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-lg hover:border-[var(--color-accent)] transition-colors cursor-pointer"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="flex-1 min-w-0">
                    <h3 className="font-medium text-[var(--color-text-primary)] truncate">
                      {skill.name}
                    </h3>
                    <p className="text-sm text-[var(--color-text-secondary)] mt-1 line-clamp-2">
                      {skill.description}
                    </p>
                  </div>
                  {skill.visibility === "support-dept" && (
                    <span className="shrink-0 px-2 py-0.5 text-xs bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400 rounded">
                      {t("enterprise.supportDept")}
                    </span>
                  )}
                </div>

                {/* Tags */}
                {skill.tags.length > 0 && (
                  <div className="flex flex-wrap gap-1 mt-2">
                    {skill.tags.map((tag) => (
                      <span
                        key={tag}
                        className="px-1.5 py-0.5 text-xs bg-[var(--color-surface-hover)] text-[var(--color-text-tertiary)] rounded"
                      >
                        {tag}
                      </span>
                    ))}
                  </div>
                )}

                {/* Footer */}
                <div className="flex items-center justify-between mt-3 pt-3 border-t border-[var(--color-border)]">
                  <div className="flex items-center gap-3 text-xs text-[var(--color-text-tertiary)]">
                    {skill.author && <span>{skill.author}</span>}
                    {skill.version && <span>v{skill.version}</span>}
                    {skill.download_count !== undefined && (
                      <span className="flex items-center gap-1">
                        <Download className="w-3 h-3" />
                        {skill.download_count}
                      </span>
                    )}
                  </div>
                  {(() => {
                    const isBusy = installing.has(skill.name);
                    const isInstalled = installedNames.has(
                      skill.name.trim().toLowerCase()
                    );
                    return (
                      <button
                        onClick={(e) => handleInstall(e, skill)}
                        disabled={isBusy}
                        title={isInstalled ? t("enterprise.updateHint") : undefined}
                        className={cn(
                          "shrink-0 flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded transition-opacity disabled:opacity-60",
                          isInstalled
                            ? "border border-[var(--color-border)] text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]"
                            : "bg-[var(--color-accent)] text-white hover:opacity-90"
                        )}
                      >
                        {isBusy ? (
                          <Loader2 className="w-3 h-3 animate-spin" />
                        ) : isInstalled ? (
                          <CheckCircle2 className="w-3 h-3 text-green-500" />
                        ) : (
                          <Download className="w-3 h-3" />
                        )}
                        {isBusy
                          ? t("enterprise.installing")
                          : isInstalled
                          ? t("enterprise.alreadyInstalled")
                          : t("enterprise.install")}
                      </button>
                    );
                  })()}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <DetailSheet
        open={!!selectedSkill}
        title={selectedSkill?.name || ""}
        description={selectedSkill?.description}
        onClose={() => setSelectedSkill(null)}
        meta={
          selectedSkill && (
            <div className="flex flex-wrap items-center gap-3 text-sm text-[var(--color-text-secondary)]">
              {selectedSkill.author && (
                <span className="flex items-center gap-1">
                  <span className="text-[var(--color-text-tertiary)]">Author:</span> {selectedSkill.author}
                </span>
              )}
              {selectedSkill.version && (
                <span className="flex items-center gap-1">
                  <span className="text-[var(--color-text-tertiary)]">Version:</span> v{selectedSkill.version}
                </span>
              )}
              <span className={cn(
                "px-2 py-0.5 text-xs rounded",
                selectedSkill.visibility === "support-dept"
                  ? "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400"
                  : "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400"
              )}>
                {selectedSkill.visibility === "support-dept" ? t("enterprise.supportDept") : "Global"}
              </span>
            </div>
          )
        }
      >
        {selectedSkill && (
          <div className="space-y-4">
            {canDeleteEnterpriseSkills && (
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={handleDelete}
                  disabled={deleting}
                  className="inline-flex items-center gap-1.5 rounded-md border border-red-500/40 px-3 py-1.5 text-sm text-red-400 hover:bg-red-500/10 disabled:opacity-60"
                >
                  {deleting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
                  {t("enterprise.delete")}
                </button>
              </div>
            )}
            {selectedSkill.tags.length > 0 && (
              <div>
                <h4 className="text-sm font-medium text-[var(--color-text-primary)] mb-2">Tags</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedSkill.tags.map((tag) => (
                    <span
                      key={tag}
                      className="px-2 py-1 text-xs bg-[var(--color-surface-hover)] text-[var(--color-text-secondary)] rounded"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              </div>
            )}

            <div>
              <h4 className="text-sm font-medium text-[var(--color-text-primary)] mb-2">Info</h4>
              <div className="grid grid-cols-2 gap-2 text-sm">
                <div className="text-[var(--color-text-tertiary)]">Name</div>
                <div className="text-[var(--color-text-primary)]">{selectedSkill.name}</div>
                {selectedSkill.version && (
                  <>
                    <div className="text-[var(--color-text-tertiary)]">Version</div>
                    <div className="text-[var(--color-text-primary)]">v{selectedSkill.version}</div>
                  </>
                )}
                {selectedSkill.author && (
                  <>
                    <div className="text-[var(--color-text-tertiary)]">Author</div>
                    <div className="text-[var(--color-text-primary)]">{selectedSkill.author}</div>
                  </>
                )}
                {selectedSkill.download_count !== undefined && (
                  <>
                    <div className="text-[var(--color-text-tertiary)]">Downloads</div>
                    <div className="text-[var(--color-text-primary)]">{selectedSkill.download_count}</div>
                  </>
                )}
              </div>
            </div>

            <div>
              <h4 className="text-sm font-medium text-[var(--color-text-primary)] mb-2">Description</h4>
              <p className="text-sm text-[var(--color-text-secondary)] leading-relaxed whitespace-pre-wrap">
                {selectedSkill.description}
              </p>
            </div>
          </div>
        )}
      </DetailSheet>
    </div>
  );
}
