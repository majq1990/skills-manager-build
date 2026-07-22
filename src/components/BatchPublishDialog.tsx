import { useCallback, useEffect, useRef, useState } from "react";
import { X, UploadCloud, Loader2, CheckCircle2, AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
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

export interface BatchPublishSkill {
  id: string;
  name: string;
  central_path: string;
}

interface Props {
  open: boolean;
  skills: BatchPublishSkill[];
  onClose: () => void;
}

type ItemStatus = "pending" | "uploading" | "success" | "error";

interface ItemState {
  status: ItemStatus;
  message?: string;
}

export function BatchPublishDialog({ open, skills, onClose }: Props) {
  const { t } = useTranslation();
  const { enterpriseUploadVisibilities } = useApp();
  const [started, setStarted] = useState(false);
  const [items, setItems] = useState<Record<string, ItemState>>({});
  const [currentIndex, setCurrentIndex] = useState(0);
  const [visibility, setVisibility] = useState("");
  const cancelledRef = useRef(false);

  const noUploadPermission = enterpriseUploadVisibilities.length === 0;

  useEffect(() => {
    if (!open) {
      setStarted(false);
      setItems({});
      setCurrentIndex(0);
      cancelledRef.current = false;
      return;
    }
    // 打开时确定统一可见性默认值：含 public 用 public，否则用第一个可选项。
    const allowed = enterpriseUploadVisibilities;
    if (allowed.length === 0) {
      setVisibility("");
    } else if (allowed.includes("public")) {
      setVisibility("public");
    } else {
      setVisibility(allowed[0]);
    }
  }, [open, enterpriseUploadVisibilities]);

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
          skill.name,
          skill.central_path,
          undefined,
          visibility || undefined
        );
        updateItem(skill.id, {
          status: "success",
          message: resp.version ? `v${resp.version}` : resp.message,
        });
      } catch (err: unknown) {
        updateItem(skill.id, {
          status: "error",
          message: getErrorMessage(err, t("publish.failed")),
        });
      }
    }
    setCurrentIndex(skills.length);
  }, [skills, updateItem, t, visibility]);

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
        onClick={allDone || !started ? onClose : undefined}
      />
      <div className="relative bg-surface border border-border rounded-xl w-full max-w-lg p-5 shadow-2xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-[13px] font-semibold text-primary flex items-center gap-2">
            <UploadCloud className="w-4 h-4 text-accent-light" />
            {t("publish.batch.title", { count: skills.length })}
          </h2>
          <button
            onClick={onClose}
            className="text-muted hover:text-secondary p-1 rounded transition-colors outline-none"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {!started && (
          <>
            <div className="mb-4 max-h-48 overflow-y-auto rounded-[4px] border border-border bg-bg-secondary p-2">
              {skills.map((s) => (
                <div
                  key={s.id}
                  className="flex items-center gap-2 px-2 py-1 text-[13px] text-muted"
                >
                  <UploadCloud className="w-3 h-3 text-faint" />
                  <span className="truncate">{s.name}</span>
                </div>
              ))}
            </div>

            {/* 统一可见范围（应用到本批全部技能） */}
            <div className="mb-4">
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

            <button
              onClick={handleStart}
              disabled={noUploadPermission}
              className="w-full rounded-[4px] bg-accent-dark hover:bg-accent py-2 text-[13px] font-semibold text-white transition-colors border border-accent-border outline-none disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {t("publish.batch.start", { count: skills.length })}
            </button>
          </>
        )}

        {started && (
          <>
            {!allDone && (
              <div className="mb-3 flex items-center gap-2 text-[13px] text-muted">
                <Loader2 className="w-4 h-4 animate-spin text-accent-light" />
                {t("publish.batch.progress", {
                  current: Math.min(currentIndex + 1, skills.length),
                  total: skills.length,
                })}
              </div>
            )}

            {allDone && (
              <div className="mb-3 flex items-center gap-3 text-[13px]">
                <span className="flex items-center gap-1 text-emerald-400">
                  <CheckCircle2 className="w-4 h-4" />
                  {t("publish.batch.successCount", { count: successCount })}
                </span>
                {errorCount > 0 && (
                  <span className="flex items-center gap-1 text-red-400">
                    <AlertCircle className="w-4 h-4" />
                    {t("publish.batch.errorCount", { count: errorCount })}
                  </span>
                )}
              </div>
            )}

            <div className="max-h-64 overflow-y-auto rounded-[4px] border border-border bg-bg-secondary p-2 space-y-1">
              {skills.map((s) => {
                const st = items[s.id]?.status ?? "pending";
                const msg = items[s.id]?.message;
                return (
                  <div
                    key={s.id}
                    className="flex items-center gap-2 px-2 py-1 text-[13px]"
                  >
                    {st === "uploading" && (
                      <Loader2 className="w-3.5 h-3.5 animate-spin text-accent-light shrink-0" />
                    )}
                    {st === "success" && (
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                    )}
                    {st === "error" && (
                      <AlertCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />
                    )}
                    {st === "pending" && (
                      <span className="w-3.5 h-3.5 shrink-0 rounded-full border border-faint" />
                    )}
                    <span
                      className={cn(
                        "truncate",
                        st === "error" ? "text-red-300" : "text-secondary"
                      )}
                    >
                      {s.name}
                    </span>
                    {msg && (
                      <span
                        className={cn(
                          "ml-auto truncate text-[12px]",
                          st === "error" ? "text-red-400" : "text-faint"
                        )}
                        title={msg}
                      >
                        {msg}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>

            {allDone && (
              <div className="flex justify-end mt-4">
                <button
                  onClick={onClose}
                  className="px-3 py-1.5 rounded-[4px] bg-accent-dark hover:bg-accent text-white text-[13px] font-medium transition-colors border border-accent-border outline-none"
                >
                  {t("common.done", { defaultValue: "完成" })}
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
