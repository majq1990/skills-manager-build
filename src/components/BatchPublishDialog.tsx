import { useCallback, useEffect, useRef, useState } from "react";
import {
  X,
  Upload,
  Loader2,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import * as api from "../lib/tauri";
import { getErrorMessage } from "../lib/error";

interface SkillItem {
  id: string;
  name: string;
}

interface Props {
  open: boolean;
  skills: SkillItem[];
  onClose: () => void;
}

type ItemStatus = "pending" | "uploading" | "success" | "error";

interface ItemState {
  status: ItemStatus;
  message?: string;
}

export function BatchPublishDialog({ open, skills, onClose }: Props) {
  const { t } = useTranslation();
  const [category, setCategory] = useState<"global" | "support-dept">("global");
  const [started, setStarted] = useState(false);
  const [items, setItems] = useState<Record<string, ItemState>>({});
  const [currentIndex, setCurrentIndex] = useState(0);
  const cancelledRef = useRef(false);

  useEffect(() => {
    if (!open) {
      setCategory("global");
      setStarted(false);
      setItems({});
      setCurrentIndex(0);
      cancelledRef.current = false;
    }
  }, [open]);

  const updateItem = useCallback(
    (id: string, state: ItemState) =>
      setItems((prev) => ({ ...prev, [id]: state })),
    []
  );

  const handleStart = useCallback(async () => {
    setStarted(true);
    cancelledRef.current = false;
    const initial: Record<string, ItemState> = {};
    for (const s of skills) initial[s.id] = { status: "pending" };
    setItems(initial);

    for (let i = 0; i < skills.length; i++) {
      if (cancelledRef.current) break;
      const skill = skills[i];
      setCurrentIndex(i);
      updateItem(skill.id, { status: "uploading" });
      try {
        const resp = await api.enterpriseUploadSkill(
          skill.id,
          undefined,
          category
        );
        updateItem(skill.id, {
          status: "success",
          message: resp.message || resp.version,
        });
      } catch (err: unknown) {
        updateItem(skill.id, {
          status: "error",
          message: getErrorMessage(err, "Upload failed"),
        });
      }
    }
    setCurrentIndex(skills.length);
  }, [skills, category, updateItem]);

  if (!open || skills.length === 0) return null;

  const doneCount = Object.values(items).filter(
    (s) => s.status === "success" || s.status === "error"
  ).length;
  const successCount = Object.values(items).filter(
    (s) => s.status === "success"
  ).length;
  const errorCount = Object.values(items).filter(
    (s) => s.status === "error"
  ).length;
  const allDone = started && doneCount === skills.length;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={allDone ? onClose : undefined}
      />
      <div className="relative w-full max-w-lg rounded-xl border border-border bg-surface p-5 shadow-2xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-[14px] font-semibold text-primary">
            <Upload className="h-4 w-4 text-accent" />
            {t("enterprise.publish.batch.title", {
              count: skills.length,
              defaultValue: `批量发布 ${skills.length} 个 Skill`,
            })}
          </h2>
          <button
            onClick={onClose}
            className="rounded p-1 text-muted transition-colors hover:text-secondary"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {!started && (
          <>
            <label className="mb-1 block text-[12px] font-medium text-muted">
              {t("enterprise.publish.dialog.category")}
            </label>
            <div className="mb-4 flex gap-2">
              <button
                type="button"
                onClick={() => setCategory("global")}
                className={cn(
                  "flex-1 rounded-lg border py-2 text-[13px] font-medium transition-colors",
                  category === "global"
                    ? "border-accent bg-accent-bg text-accent-light"
                    : "border-border text-muted hover:border-border-subtle hover:text-secondary"
                )}
              >
                {t("enterprise.publish.dialog.categoryGlobal")}
              </button>
              <button
                type="button"
                onClick={() => setCategory("support-dept")}
                className={cn(
                  "flex-1 rounded-lg border py-2 text-[13px] font-medium transition-colors",
                  category === "support-dept"
                    ? "border-accent bg-accent-bg text-accent-light"
                    : "border-border text-muted hover:border-border-subtle hover:text-secondary"
                )}
              >
                {t("enterprise.publish.dialog.categorySupportDept")}
              </button>
            </div>

            <div className="mb-4 max-h-48 overflow-y-auto rounded-lg border border-border bg-background p-2">
              {skills.map((s) => (
                <div
                  key={s.id}
                  className="flex items-center gap-2 px-2 py-1 text-[13px] text-muted"
                >
                  <Upload className="h-3 w-3 text-faint" />
                  {s.name}
                </div>
              ))}
            </div>

            <button
              onClick={handleStart}
              className="w-full rounded-lg bg-accent-dark py-2 text-[13px] font-semibold text-white transition-colors hover:bg-accent"
            >
              {t("enterprise.publish.batch.start", {
                count: skills.length,
                defaultValue: `开始上传 ${skills.length} 个 Skill`,
              })}
            </button>
          </>
        )}

        {started && (
          <>
            {!allDone && (
              <div className="mb-3 flex items-center gap-2 text-[13px] text-muted">
                <Loader2 className="h-4 w-4 animate-spin text-accent" />
                {t("enterprise.publish.batch.progress", {
                  current: currentIndex + 1,
                  total: skills.length,
                  defaultValue: `正在上传 ${currentIndex + 1}/${skills.length}...`,
                })}
              </div>
            )}

            {allDone && (
              <div className="mb-3 flex items-center gap-3 text-[13px]">
                <span className="flex items-center gap-1 text-emerald-400">
                  <CheckCircle2 className="h-4 w-4" />
                  {t("enterprise.publish.batch.successCount", {
                    count: successCount,
                    defaultValue: `${successCount} 个成功`,
                  })}
                </span>
                {errorCount > 0 && (
                  <span className="flex items-center gap-1 text-red-400">
                    <AlertCircle className="h-4 w-4" />
                    {t("enterprise.publish.batch.errorCount", {
                      count: errorCount,
                      defaultValue: `${errorCount} 个失败`,
                    })}
                  </span>
                )}
              </div>
            )}

            <div className="max-h-64 overflow-y-auto rounded-lg border border-border bg-background p-2">
              {skills.map((s) => {
                const state = items[s.id];
                return (
                  <div
                    key={s.id}
                    className="flex items-center gap-2 px-2 py-1.5 text-[13px]"
                  >
                    {state?.status === "uploading" && (
                      <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-accent" />
                    )}
                    {state?.status === "success" && (
                      <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-emerald-400" />
                    )}
                    {state?.status === "error" && (
                      <AlertCircle className="h-3.5 w-3.5 shrink-0 text-red-400" />
                    )}
                    {state?.status === "pending" && (
                      <div className="h-3.5 w-3.5 shrink-0 rounded-full border border-border" />
                    )}
                    <span
                      className={cn(
                        "flex-1 truncate",
                        state?.status === "error"
                          ? "text-red-400"
                          : state?.status === "success"
                          ? "text-emerald-400"
                          : "text-muted"
                      )}
                    >
                      {s.name}
                    </span>
                    {state?.message && (
                      <span className="max-w-[200px] truncate text-[11px] text-faint">
                        {state.message}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>

            {allDone && (
              <button
                onClick={onClose}
                className="mt-3 w-full rounded-lg border border-border py-2 text-[13px] font-medium text-secondary transition-colors hover:bg-surface-hover"
              >
                {t("enterprise.publish.dialog.close")}
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}
