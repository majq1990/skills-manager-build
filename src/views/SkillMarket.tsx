import { useState, useEffect, useMemo, useCallback } from "react";
import { Search, Loader2, Shield, Download, Check, BarChart3, Tag, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "../utils";
import { useAuth } from "../hooks/useAuth";
import { useApp } from "../context/AppContext";
import * as api from "../lib/tauri";
import type { SkillInfo } from "../lib/tauri";

export function SkillMarket() {
  const { t } = useTranslation();
  const { isAuthenticated } = useAuth();
  const { refreshManagedSkills } = useApp();
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"all" | "global" | "support-dept">("all");
  const [installing, setInstalling] = useState<Set<string>>(new Set());
  const [allTags, setAllTags] = useState<string[]>([]);
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set());

  const loadSkills = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.enterpriseListSkills();
      setSkills(data);
    } catch (err: any) {
      console.error("Failed to load skills:", err);
      // Handle Tauri errors properly
      let errorMsg = t("enterprise.market.error");
      if (err?.message) {
        errorMsg = err.message;
      } else if (typeof err === 'string') {
        errorMsg = err;
      } else if (err && typeof err === 'object') {
        // Try to extract error message from Tauri error object
        errorMsg = err.message || err.error || JSON.stringify(err);
      }
      setError(`${t("enterprise.market.error")}: ${errorMsg}`);
    } finally {
      setLoading(false);
    }
  }, [t]);

  const loadTags = useCallback(async () => {
    try {
      const tags = await api.enterpriseGetTags();
      setAllTags(tags);
    } catch (err) {
      console.error("Failed to load tags:", err);
    }
  }, []);

  useEffect(() => {
    if (isAuthenticated) {
      loadSkills();
      loadTags();
    }
  }, [isAuthenticated, loadSkills, loadTags]);

  const handleInstall = async (name: string) => {
    setInstalling((prev) => new Set(prev).add(name));
    try {
      await api.enterpriseInstallSkill(name);
      toast.success(`${name} ${t("enterprise.market.install")} ${t("enterprise.market.installed")}`);
      await loadSkills();
      // Refresh My Skills list so the newly installed skill appears
      await refreshManagedSkills();
    } catch (err: any) {
      console.error("Failed to install skill:", err);
      const errorMsg = err?.message || String(err) || t("enterprise.market.error");
      toast.error(`${name} 安装失败: ${errorMsg}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
    }
  };

  const toggleTag = useCallback((tag: string) => {
    setSelectedTags((prev) => {
      const next = new Set(prev);
      if (next.has(tag)) {
        next.delete(tag);
      } else {
        next.add(tag);
      }
      return next;
    });
  }, []);

  const filtered = useMemo(() => {
    const needle = search.toLowerCase();
    return skills.filter((skill) => {
      if (filter !== "all" && skill.visibility !== filter) return false;
      if (selectedTags.size > 0) {
        const skillTags = skill.tags || [];
        const hasAllTags = Array.from(selectedTags).every((tag) => skillTags.includes(tag));
        if (!hasAllTags) return false;
      }
      if (needle) {
        const matchesName = skill.name.toLowerCase().includes(needle);
        const matchesDesc = (skill.description || "").toLowerCase().includes(needle);
        const matchesTags = (skill.tags || []).some((tag) => tag.toLowerCase().includes(needle));
        if (!matchesName && !matchesDesc && !matchesTags) return false;
      }
      return true;
    });
  }, [skills, search, filter, selectedTags]);

  if (!isAuthenticated) {
    return (
      <div className="app-page">
        <div className="app-page-header">
          <h1 className="app-page-title">{t("enterprise.market.title")}</h1>
          <p className="app-page-subtitle text-tertiary">{t("enterprise.market.description")}</p>
        </div>
        <div className="flex flex-1 flex-col items-center justify-center pb-20 text-center">
          <Shield className="mb-4 h-12 w-12 text-faint" />
          <h3 className="mb-1.5 text-[14px] font-semibold text-tertiary">
            {t("enterprise.market.loginRequired")}
          </h3>
          <p className="text-[13px] text-muted">
            {t("enterprise.market.loginDescription")}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="app-page">
      <div className="app-page-header">
        <h1 className="app-page-title">{t("enterprise.market.title")}</h1>
        <p className="app-page-subtitle text-tertiary">{t("enterprise.market.description")}</p>
      </div>

      {/* Search & Filters */}
      <div className="app-toolbar">
        <div className="relative w-full max-w-[280px]">
          <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("enterprise.market.searchPlaceholder")}
            className="app-input w-full bg-background pl-9"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
        </div>

        <div className="app-segmented">
          {([
            { key: "all", label: t("enterprise.market.filter.all") },
            { key: "global", label: t("enterprise.market.filter.global") },
            { key: "support-dept", label: t("enterprise.market.filter.supportDept") },
          ] as const).map(({ key, label }) => (
            <button
              key={key}
              onClick={() => setFilter(key)}
              className={cn(
                "app-segmented-button",
                filter === key && "app-segmented-button-active"
              )}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* Tag Filter */}
      {allTags.length > 0 && (
        <div className="flex flex-wrap gap-1.5 px-1">
          <Tag className="h-3.5 w-3.5 shrink-0 text-muted mt-0.5" />
          {allTags.map((tag) => (
            <button
              key={tag}
              onClick={() => toggleTag(tag)}
              className={cn(
                "inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-[11px] font-medium transition-colors",
                selectedTags.has(tag)
                  ? "bg-blue-500/15 text-blue-600 dark:text-blue-400 border border-blue-500/30"
                  : "bg-surface-hover text-secondary border border-transparent hover:border-border"
              )}
            >
              {tag}
              {selectedTags.has(tag) && <X className="h-2.5 w-2.5" />}
            </button>
          ))}
          {selectedTags.size > 0 && (
            <button
              onClick={() => setSelectedTags(new Set())}
              className="text-[11px] text-muted hover:text-secondary underline"
            >
              {t("enterprise.market.clearTags", "清除")}
            </button>
          )}
        </div>
      )}

      {/* Loading */}
      {loading && (
        <div className="flex flex-1 items-center justify-center py-20">
          <Loader2 className="h-6 w-6 animate-spin text-muted" />
          <span className="ml-2 text-[13px] text-muted">{t("enterprise.market.loading")}</span>
        </div>
      )}

      {/* Error */}
      {!loading && error && (
        <div className="flex flex-1 flex-col items-center justify-center py-20 text-center">
          <p className="mb-3 text-[14px] font-medium text-red-500">{error}</p>
          <button
            onClick={loadSkills}
            className="app-button-secondary"
          >
            {t("enterprise.market.retry")}
          </button>
        </div>
      )}

      {/* Empty */}
      {!loading && !error && filtered.length === 0 && (
        <div className="flex flex-1 flex-col items-center justify-center py-20 text-center">
          <Search className="mb-4 h-12 w-12 text-faint" />
          <h3 className="mb-1.5 text-[14px] font-semibold text-tertiary">
            {t("enterprise.market.noResults")}
          </h3>
        </div>
      )}

      {/* Skill Grid */}
      {!loading && !error && filtered.length > 0 && (
        <div className="grid grid-cols-2 gap-2.5 lg:grid-cols-3">
          {filtered.map((skill) => {
            const isInstalling = installing.has(skill.name);
            const isInstalled = skill.installed;

            return (
              <div
                key={skill.name}
                className="app-panel flex flex-col gap-2 p-3 transition-colors hover:border-border hover:bg-surface-hover"
              >
                {/* Name & Visibility */}
                <div className="flex items-start justify-between gap-2">
                  <h3
                    className="truncate text-[14px] font-semibold text-primary"
                    title={skill.name}
                  >
                    {skill.name}
                  </h3>
                  {skill.visibility === "support-dept" && (
                    <span className="shrink-0 rounded-full bg-amber-500/12 px-2 py-0.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
                      {t("enterprise.market.supportDeptBadge")}
                    </span>
                  )}
                </div>

                {/* Description */}
                <p
                  className="line-clamp-2 text-[13px] leading-[18px] text-muted"
                  title={skill.description || ""}
                >
                  {skill.description || "—"}
                </p>

                {/* Tags */}
                {skill.tags && skill.tags.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {skill.tags.map((tag) => (
                      <span
                        key={tag}
                        className="rounded-full bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400"
                      >
                        {tag}
                      </span>
                    ))}
                  </div>
                )}

                {/* Meta */}
                <div className="flex items-center gap-2 text-[12px] text-muted">
                  <span className="rounded bg-surface-hover px-1.5 py-0.5 font-mono text-[11px] text-secondary">
                    v{skill.version}
                  </span>
                  <span className="truncate" title={skill.author}>
                    {skill.author}
                  </span>
                  {skill.download_count !== undefined && skill.download_count > 0 && (
                    <span className="flex items-center gap-0.5 text-[11px] text-muted">
                      <BarChart3 className="h-3 w-3" />
                      {skill.download_count}
                    </span>
                  )}
                </div>

                {/* Action */}
                <div className="mt-auto pt-1">
                  {isInstalled ? (
                    <button
                      disabled
                      className="app-button-secondary w-full cursor-default opacity-60"
                    >
                      <Check className="h-3.5 w-3.5 text-emerald-500" />
                      {t("enterprise.market.installed")}
                    </button>
                  ) : (
                    <button
                      onClick={() => handleInstall(skill.name)}
                      disabled={isInstalling}
                      className="app-button-primary w-full"
                    >
                      {isInstalling ? (
                        <>
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          {t("enterprise.market.installing")}
                        </>
                      ) : (
                        <>
                          <Download className="h-3.5 w-3.5" />
                          {t("enterprise.market.install")}
                        </>
                      )}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
