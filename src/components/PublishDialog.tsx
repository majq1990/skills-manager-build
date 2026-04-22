import { useCallback, useEffect, useRef, useState } from "react";
import {
  X,
  Upload,
  Loader2,
  CheckCircle2,
  AlertCircle,
  Clock,
  RefreshCw,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import * as api from "../lib/tauri";
import type { VersionInfo } from "../lib/tauri";
import { getErrorMessage } from "../lib/error";

interface Props {
  open: boolean;
  skillId: string;
  skillName: string;
  onClose: () => void;
}

type Phase = "idle" | "uploading" | "scanning" | "done" | "error";

export function PublishDialog({ open, skillId, skillName, onClose }: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("idle");
  const [version, setVersion] = useState("");
  const [category, setCategory] = useState<"global" | "support-dept">("global");
  const [uploadedVersion, setUploadedVersion] = useState("");
  const [scanStatus, setScanStatus] = useState("");
  const [message, setMessage] = useState("");
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<VersionInfo[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [rescanning, setRescanning] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const cleanup = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!open) {
      setPhase("idle");
      setVersion("");
      setCategory("global");
      setUploadedVersion("");
      setScanStatus("");
      setMessage("");
      setHistoryOpen(false);
      setHistory([]);
      cleanup();
    }
  }, [open, cleanup]);

  useEffect(() => () => cleanup(), [cleanup]);

  const startScanPoll = useCallback(
    (name: string, ver: string) => {
      cleanup();
      setPhase("scanning");
      let attempts = 0;
      pollRef.current = setInterval(async () => {
        attempts++;
        try {
          const resp = await api.enterpriseCheckScan(name, ver);
          const status = resp.data?.status ?? "unknown";
          setScanStatus(status);
          if (status !== "pending" || attempts >= 12) {
            cleanup();
            setPhase("done");
          }
        } catch {
          cleanup();
          setPhase("done");
        }
      }, 5000);
    },
    [cleanup]
  );

  const handleUpload = async () => {
    setPhase("uploading");
    setMessage("");
    try {
      const resp = await api.enterpriseUploadSkill(
        skillId,
        version || undefined,
        category
      );
      setUploadedVersion(resp.version);
      setScanStatus(resp.scan_status);
      setMessage(resp.message);

      if (resp.scan_status === "pending") {
        startScanPoll(skillName, resp.version);
      } else {
        setPhase("done");
      }
    } catch (err: unknown) {
      setPhase("error");
      setMessage(getErrorMessage(err, t("enterprise.publish.dialog.failed")));
    }
  };

  const handleRescan = async () => {
    if (!uploadedVersion) return;
    setRescanning(true);
    try {
      const resp = await api.enterpriseTriggerScan(skillName, uploadedVersion);
      setScanStatus(resp.status);
      if (resp.status === "pending") {
        startScanPoll(skillName, uploadedVersion);
      }
    } catch {
      // ignore
    } finally {
      setRescanning(false);
    }
  };

  const loadHistory = async () => {
    if (history.length > 0) return;
    setHistoryLoading(true);
    try {
      const data = await api.enterpriseUploadHistory(skillName);
      setHistory(data);
    } catch {
      // ignore
    } finally {
      setHistoryLoading(false);
    }
  };

  const toggleHistory = () => {
    const next = !historyOpen;
    setHistoryOpen(next);
    if (next) loadHistory();
  };

  if (!open) return null;

  const scanBadge = () => {
    if (!scanStatus) return null;
    const map: Record<string, { icon: typeof CheckCircle2; color: string }> = {
      passed: { icon: CheckCircle2, color: "text-emerald-400" },
      pending: { icon: Clock, color: "text-amber-400" },
      failed: { icon: AlertCircle, color: "text-red-400" },
    };
    const entry = map[scanStatus] ?? map.pending!;
    const Icon = entry.icon;
    return (
      <span className={cn("inline-flex items-center gap-1", entry.color)}>
        <Icon className="h-3.5 w-3.5" />
        {t(`enterprise.publish.dialog.${scanStatus === "passed" ? "passed" : scanStatus === "failed" ? "failedScan" : "pending"}`)}
      </span>
    );
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative w-full max-w-md rounded-xl border border-border bg-surface p-5 shadow-2xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-[14px] font-semibold text-primary">
            <Upload className="h-4 w-4 text-accent" />
            {t("enterprise.publish.dialog.title")}
          </h2>
          <button
            onClick={onClose}
            className="rounded p-1 text-muted transition-colors hover:text-secondary"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <p className="mb-3 text-[13px] text-muted">
          {t("enterprise.publish.dialog.subtitle", { name: skillName })}
        </p>

        {phase === "idle" && (
          <>
            <label className="mb-1 block text-[12px] font-medium text-muted">
              {t("enterprise.publish.dialog.version")}
            </label>
            <input
              type="text"
              value={version}
              onChange={(e) => setVersion(e.target.value)}
              placeholder={t("enterprise.publish.dialog.versionPlaceholder")}
              className="mb-3 w-full rounded-lg border border-border bg-background px-3 py-2 text-[13px] text-primary outline-none placeholder:text-faint focus:border-accent"
              spellCheck={false}
            />
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
            <button
              onClick={handleUpload}
              className="w-full rounded-lg bg-accent-dark py-2 text-[13px] font-semibold text-white transition-colors hover:bg-accent"
            >
              {t("enterprise.publish.dialog.upload")}
            </button>
          </>
        )}

        {phase === "uploading" && (
          <div className="flex items-center gap-2 py-4 text-[13px] text-muted">
            <Loader2 className="h-4 w-4 animate-spin text-accent" />
            {t("enterprise.publish.dialog.uploading")}
          </div>
        )}

        {phase === "scanning" && (
          <div className="space-y-2 py-4">
            <div className="flex items-center gap-2 text-[13px] text-muted">
              <Loader2 className="h-4 w-4 animate-spin text-amber-400" />
              {t("enterprise.publish.dialog.scanning")}
            </div>
            {scanBadge()}
          </div>
        )}

        {(phase === "done" || phase === "error") && (
          <div className="space-y-3 py-2">
            {phase === "error" ? (
              <div className="flex items-center gap-2 text-[13px] text-red-400">
                <AlertCircle className="h-4 w-4" />
                {message || t("enterprise.publish.dialog.failed")}
              </div>
            ) : (
              <>
                <div className="flex items-center gap-2 text-[13px] text-emerald-400">
                  <CheckCircle2 className="h-4 w-4" />
                  {t("enterprise.publish.dialog.success")}
                </div>
                {uploadedVersion && (
                  <p className="text-[13px] text-muted">
                    {t("enterprise.publish.dialog.version")}: {uploadedVersion}
                  </p>
                )}
                <div className="flex items-center gap-3 text-[13px]">
                  <span className="text-muted">
                    {t("enterprise.publish.dialog.scanStatus")}:
                  </span>
                  {scanBadge()}
                  {uploadedVersion && (
                    <button
                      onClick={handleRescan}
                      disabled={rescanning}
                      className="inline-flex items-center gap-1 text-[12px] text-accent-light hover:underline disabled:opacity-50"
                    >
                      <RefreshCw
                        className={cn(
                          "h-3 w-3",
                          rescanning && "animate-spin"
                        )}
                      />
                      {t("enterprise.publish.dialog.rescan")}
                    </button>
                  )}
                </div>
                {message && (
                  <p className="text-[12px] text-faint">{message}</p>
                )}
              </>
            )}

            <button
              onClick={toggleHistory}
              className="mt-2 inline-flex items-center gap-1 text-[12px] text-muted hover:text-secondary"
            >
              {historyOpen ? (
                <ChevronUp className="h-3 w-3" />
              ) : (
                <ChevronDown className="h-3 w-3" />
              )}
              {t("enterprise.publish.dialog.history")}
            </button>

            {historyOpen && (
              <div className="max-h-40 overflow-y-auto rounded-lg border border-border bg-background p-2">
                {historyLoading ? (
                  <Loader2 className="mx-auto h-4 w-4 animate-spin text-muted" />
                ) : history.length === 0 ? (
                  <p className="text-center text-[12px] text-faint">
                    {t("enterprise.publish.dialog.noHistory")}
                  </p>
                ) : (
                  <table className="w-full text-[12px]">
                    <thead>
                      <tr className="text-left text-faint">
                        <th className="pb-1 font-medium">
                          {t("enterprise.publish.dialog.version")}
                        </th>
                        <th className="pb-1 font-medium">
                          {t("enterprise.publish.dialog.scanStatus")}
                        </th>
                        <th className="pb-1 font-medium">
                          {t("enterprise.publish.dialog.uploadedAt")}
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {history.map((v) => (
                        <tr key={v.version} className="text-muted">
                          <td className="py-0.5">{v.version}</td>
                          <td className="py-0.5">{v.status}</td>
                          <td className="py-0.5">{v.uploaded_at}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            )}

            <button
              onClick={onClose}
              className="mt-2 w-full rounded-lg border border-border py-2 text-[13px] font-medium text-secondary transition-colors hover:bg-surface-hover"
            >
              {t("enterprise.publish.dialog.close")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
