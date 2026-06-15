import { useState, useEffect } from "react";
import { X, MessageSquarePlus, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import * as api from "../lib/tauri";

// value 必须与服务端 VALID_TYPES 一致（中文），label 走 i18n
const TYPE_OPTIONS = [
  { value: "技能问题", key: "feedback.typeSkillIssue" },
  { value: "工具问题", key: "feedback.typeToolIssue" },
  { value: "功能建议", key: "feedback.typeSuggestion" },
];

interface Props {
  open: boolean;
  /** 默认反馈类型（中文 value，需在 TYPE_OPTIONS 内） */
  defaultType?: string;
  /** 关联技能（如 "技能名 v1.2.0"）；传了则显示只读 chip，工具反馈时留空 */
  skill?: string;
  onClose: () => void;
}

export function FeedbackDialog({ open, defaultType, skill, onClose }: Props) {
  const { t } = useTranslation();
  const [type, setType] = useState(defaultType || "功能建议");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [submitting, setSubmitting] = useState(false);

  // 每次打开时重置表单，并应用传入的默认类型
  useEffect(() => {
    if (open) {
      setType(defaultType || "功能建议");
      setTitle("");
      setDescription("");
    }
  }, [open, defaultType]);

  if (!open) return null;

  const handleSubmit = async () => {
    if (!title.trim() || !description.trim()) {
      toast.error(t("feedback.required"));
      return;
    }
    setSubmitting(true);
    try {
      // 反馈需登录态（提交人由服务端从 JWT 自动带）
      const authed = await api.enterpriseIsAuthenticated();
      if (!authed) {
        toast.error(t("feedback.needLogin"));
        return;
      }
      await api.enterpriseSubmitFeedback(type, skill || "", title.trim(), description.trim());
      toast.success(t("feedback.success"));
      onClose();
    } catch (err) {
      const msg = String((err as any)?.message ?? err ?? "");
      if (/not authenticated|\(401\)/i.test(msg)) {
        toast.error(t("feedback.needLogin"));
      } else {
        toast.error(`${t("feedback.failed")}: ${msg}`);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" onClick={onClose} />
      <div className="relative bg-surface border border-border rounded-xl w-full max-w-md p-5 shadow-2xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-[13px] font-semibold text-primary flex items-center gap-2">
            <MessageSquarePlus className="w-4 h-4 text-accent-light" />
            {t("feedback.title")}
          </h2>
          <button
            onClick={onClose}
            className="text-muted hover:text-secondary p-1 rounded transition-colors outline-none"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-4">
          {/* 反馈类型 */}
          <div>
            <label className="block text-[12px] font-medium text-secondary mb-1">
              {t("feedback.typeLabel")}
            </label>
            <select
              value={type}
              onChange={(e) => setType(e.target.value)}
              className="w-full px-3 py-2 bg-bg-secondary border border-border rounded-[4px] text-[13px] text-primary focus:outline-none focus:ring-2 focus:ring-accent"
            >
              {TYPE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(opt.key)}
                </option>
              ))}
            </select>
          </div>

          {/* 关联技能（只读，技能反馈时显示） */}
          {skill ? (
            <div>
              <label className="block text-[12px] font-medium text-secondary mb-1">
                {t("feedback.relatedSkill")}
              </label>
              <div className="px-3 py-2 bg-bg-secondary border border-border-subtle rounded-[4px] text-[13px] text-tertiary truncate">
                {skill}
              </div>
            </div>
          ) : null}

          {/* 标题 */}
          <div>
            <label className="block text-[12px] font-medium text-secondary mb-1">
              {t("feedback.titleLabel")}
            </label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              maxLength={300}
              placeholder={t("feedback.titlePlaceholder")}
              className="w-full px-3 py-2 bg-bg-secondary border border-border rounded-[4px] text-[13px] text-primary focus:outline-none focus:ring-2 focus:ring-accent"
            />
          </div>

          {/* 详细描述 */}
          <div>
            <label className="block text-[12px] font-medium text-secondary mb-1">
              {t("feedback.descLabel")}
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              maxLength={3000}
              rows={5}
              placeholder={t("feedback.descPlaceholder")}
              className="w-full px-3 py-2 bg-bg-secondary border border-border rounded-[4px] text-[13px] text-primary resize-none focus:outline-none focus:ring-2 focus:ring-accent"
            />
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
            onClick={handleSubmit}
            disabled={submitting}
            className="px-3 py-1.5 rounded-[4px] bg-accent-dark hover:bg-accent text-white text-[13px] font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed border border-accent-border outline-none flex items-center gap-1.5"
          >
            {submitting ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {t("feedback.submitting")}
              </>
            ) : (
              t("feedback.submit")
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
