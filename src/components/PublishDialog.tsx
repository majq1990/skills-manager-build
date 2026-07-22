import { useState, useEffect } from "react";
import { X, UploadCloud, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import * as api from "../lib/tauri";
import { getErrorMessage } from "../lib/error";
import { useApp } from "../context/AppContext";

/** 把可见性取值（public/tech-manager/support）映射到 i18n label。未知值原样返回。 */
function visibilityLabel(v: string, t: (k: string) => string): string {
  switch (v) {
    case "public":
      return t("publish.visibilityPublic");
    case "tech-manager":
      return t("publish.visibilityTechManager");
    case "support":
      return t("publish.visibilitySupport");
    default:
      return v;
  }
}

interface Props {
  open: boolean;
  /** 技能名（= URL {name}，须与 SKILL.md 内 name 一致） */
  skillName?: string;
  /** 中央仓技能目录（打包其内容到 zip 根） */
  centralPath?: string;
  /** 当前本地版本（仅作占位提示） */
  currentVersion?: string;
  onClose: () => void;
}

/** 把后端错误信息归一为友好文案（安全扫描失败 / 无上传权限 403）。 */
function describeError(err: unknown, t: (k: string) => string): string {
  const msg = getErrorMessage(err, t("publish.failed"));
  if (/security scan failed|unsafe/i.test(msg)) {
    return `${t("publish.scanFailed")}: ${msg}`;
  }
  if (/\(403\)|forbidden|permission/i.test(msg)) {
    return t("publish.noPermission");
  }
  if (/not authenticated|\(401\)/i.test(msg)) {
    return t("publish.needLogin");
  }
  return `${t("publish.failed")}: ${msg}`;
}

export function PublishDialog({
  open,
  skillName,
  centralPath,
  currentVersion,
  onClose,
}: Props) {
  const { t } = useTranslation();
  const { enterpriseUploadVisibilities } = useApp();
  const [version, setVersion] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [visibility, setVisibility] = useState("");

  const noUploadPermission = enterpriseUploadVisibilities.length === 0;

  // 打开时确定可见性默认值：取该技能现有可见性（若在可选范围内），否则用第一个可选项。
  useEffect(() => {
    if (!open) return;
    setVersion("");
    setSubmitting(false);

    const allowed = enterpriseUploadVisibilities;
    if (allowed.length === 0) {
      setVisibility("");
      return;
    }
    // 先用第一个可选项兜底，再异步用该技能现有可见性覆盖（找得到且在可选范围内才覆盖）。
    setVisibility(allowed[0]);
    if (!skillName) return;
    let cancelled = false;
    (async () => {
      try {
        const list = await api.enterpriseListSkills();
        if (cancelled) return;
        const current = list.find((s) => s.name === skillName)?.visibility;
        if (current && allowed.includes(current)) {
          setVisibility(current);
        }
      } catch {
        // 拿不到列表（如未登录/网络问题）就保持兜底默认值。
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, skillName]);

  if (!open || !skillName || !centralPath) return null;

  const handlePublish = async () => {
    setSubmitting(true);
    try {
      const authed = await api.enterpriseIsAuthenticated();
      if (!authed) {
        toast.error(t("publish.needLogin"));
        return;
      }
      const resp = await api.enterpriseUploadSkill(
        skillName,
        centralPath,
        version.trim() || undefined,
        visibility || undefined
      );
      toast.success(
        t("publish.success", {
          name: skillName,
          version: resp.version || version.trim() || "",
        })
      );
      onClose();
    } catch (err) {
      toast.error(describeError(err, t));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative bg-surface border border-border rounded-xl w-full max-w-md p-5 shadow-2xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-[13px] font-semibold text-primary flex items-center gap-2">
            <UploadCloud className="w-4 h-4 text-accent-light" />
            {t("publish.title")}
          </h2>
          <button
            onClick={onClose}
            className="text-muted hover:text-secondary p-1 rounded transition-colors outline-none"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-4">
          <p className="text-[13px] text-muted">
            {t("publish.subtitle", { name: skillName })}
          </p>

          {/* 关联技能（只读） */}
          <div>
            <label className="block text-[12px] font-medium text-secondary mb-1">
              {t("publish.skillLabel")}
            </label>
            <div className="px-3 py-2 bg-bg-secondary border border-border-subtle rounded-[4px] text-[13px] text-tertiary truncate">
              {skillName}
              {currentVersion ? ` (v${currentVersion})` : ""}
            </div>
          </div>

          {/* 目标版本（可选，留空自动递增） */}
          <div>
            <label className="block text-[12px] font-medium text-secondary mb-1">
              {t("publish.versionLabel")}
            </label>
            <input
              type="text"
              value={version}
              onChange={(e) => setVersion(e.target.value)}
              maxLength={50}
              placeholder={t("publish.versionPlaceholder")}
              className="w-full px-3 py-2 bg-bg-secondary border border-border rounded-[4px] text-[13px] text-primary focus:outline-none focus:ring-2 focus:ring-accent"
            />
            <p className="mt-1 text-[12px] text-faint">{t("publish.versionHint")}</p>
          </div>

          {/* 可见范围（按角色过滤好的可选级别） */}
          <div>
            <label className="block text-[12px] font-medium text-secondary mb-1">
              {t("publish.visibilityLabel")}
            </label>
            {noUploadPermission ? (
              <p className="text-[12px] text-red-400">
                {t("publish.noUploadPermission")}
              </p>
            ) : (
              <select
                value={visibility}
                onChange={(e) => setVisibility(e.target.value)}
                className="w-full px-3 py-2 bg-bg-secondary border border-border rounded-[4px] text-[13px] text-primary focus:outline-none focus:ring-2 focus:ring-accent"
              >
                {enterpriseUploadVisibilities.map((v) => (
                  <option key={v} value={v}>
                    {visibilityLabel(v, t)}
                  </option>
                ))}
              </select>
            )}
          </div>
        </div>

        <div className="flex justify-end gap-2 mt-5">
          <button
            onClick={onClose}
            className="px-3 py-1.5 rounded-[4px] text-[13px] font-medium text-tertiary hover:text-secondary hover:bg-surface-hover transition-colors outline-none"
          >
            {t("common.cancel")}
          </button>
          <button
            onClick={handlePublish}
            disabled={submitting || noUploadPermission}
            className="px-3 py-1.5 rounded-[4px] bg-accent-dark hover:bg-accent text-white text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed border border-accent-border outline-none flex items-center gap-1.5"
          >
            {submitting ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {t("publish.publishing")}
              </>
            ) : (
              t("publish.publish")
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
