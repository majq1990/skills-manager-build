import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open as dialogOpen } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  RefreshCw,
  Download,
  Upload,
  UploadCloud,
  Trash2,
  Wand2,
  X,
  ChevronDown,
  MessageSquarePlus,
  Search,
} from "lucide-react";
import { cn } from "../utils";
import * as api from "../lib/tauri";
import type {
  AgentWithTargets,
  AgentDocument,
  ScanAgentFilesResult,
  ToolInfo,
  Preset,
} from "../lib/tauri";
import { ConfirmDialog } from "./ConfirmDialog";
import { FeedbackDialog } from "./FeedbackDialog";
import { AgentPublishDialog } from "./AgentPublishDialog";
import { SkillMarkdown } from "./SkillMarkdown";

const TOOL_BADGE: Record<string, string> = {
  opencode: "oc",
  codex: "cx",
  workbuddy: "wb",
  dsh: "dsh",
};

function toolBadge(tool: string): string {
  return TOOL_BADGE[tool] ?? tool.slice(0, 2);
}

export function AgentsLibrary() {
  const { t } = useTranslation();
  const [agents, setAgents] = useState<AgentWithTargets[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [scans, setScans] = useState<ScanAgentFilesResult[]>([]);
  const [importOpen, setImportOpen] = useState(false);
  const [detail, setDetail] = useState<AgentDocument | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);
  const [feedbackAgent, setFeedbackAgent] = useState<string | null>(null);
  const [publishAgent, setPublishAgent] = useState<{ name: string; centralPath: string } | null>(
    null
  );
  const [presets, setPresets] = useState<Preset[]>([]);
  const [memberPresetIds, setMemberPresetIds] = useState<string[]>([]);
  const [search, setSearch] = useState("");

  const agentCapableTools = useMemo(
    () => tools.filter((tool) => ["opencode", "codex", "workbuddy", "dsh"].includes(tool.key)),
    [tools]
  );

  const visibleAgents = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return agents;
    return agents.filter(
      (agent) =>
        agent.name.toLowerCase().includes(q) ||
        (agent.description || "").toLowerCase().includes(q)
    );
  }, [agents, search]);

  const refresh = useCallback(async () => {
    try {
      setAgents(await api.getAgents());
    } catch (err) {
      toast.error(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
    void api
      .getToolStatus()
      .then(setTools)
      .catch(() => setTools([]));
  }, [refresh]);

  const openImport = async () => {
    setImportOpen(true);
    try {
      setScans(await api.scanAgentFiles());
    } catch (err) {
      toast.error(String(err));
    }
  };

  // Upload = import agent definition files picked from disk (mirrors the
  // skill local-import flow). .md files become canonical; .toml is treated
  // as a codex agent and reversed into the canonical AGENT.md.
  const uploadAgents = async () => {
    const selected = await dialogOpen({
      multiple: true,
      filters: [{ name: "Agent", extensions: ["md", "toml"] }],
    });
    if (!selected || selected.length === 0) return;
    setBusy(true);
    try {
      await api.importAgentFiles(selected);
      toast.success(t("agentLib.imported"));
      await refresh();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  const importFile = async (tool: string, path: string) => {
    setBusy(true);
    try {
      await api.importAgent(tool, path);
      toast.success(t("agentLib.imported"));
      await refresh();
      setScans(await api.scanAgentFiles());
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  const importAllFrom = async (tool: string, files: { path: string }[]) => {
    setBusy(true);
    try {
      for (const file of files) {
        await api.importAgent(tool, file.path);
      }
      toast.success(t("agentLib.imported"));
      await refresh();
      setScans(await api.scanAgentFiles());
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  const openDetail = async (id: string) => {
    setDetailId(id);
    try {
      const [doc, allPresets] = await Promise.all([
        api.getAgentDocument(id),
        api.getPresets(),
      ]);
      setDetail(doc);
      setPresets(allPresets);
      const member: string[] = [];
      for (const preset of allPresets) {
        const members = await api.getPresetAgents(preset.id);
        if (members.some((m) => m.id === id)) member.push(preset.id);
      }
      setMemberPresetIds(member);
    } catch (err) {
      toast.error(String(err));
    }
  };

  const togglePreset = async (agentId: string, presetId: string, current: boolean) => {
    setBusy(true);
    try {
      if (current) {
        await api.removeAgentFromPreset(agentId, presetId);
      } else {
        await api.addAgentToPreset(agentId, presetId);
      }
      setMemberPresetIds((prev) =>
        current ? prev.filter((p) => p !== presetId) : [...prev, presetId]
      );
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  const toggleTool = async (agentId: string, tool: string, current: boolean) => {
    setBusy(true);
    try {
      if (current) {
        await api.unsyncAgentFromTool(agentId, tool);
        toast.success(t("agentLib.unsynced"));
      } else {
        const outcome = await api.syncAgentToTool(agentId, tool);
        toast.success(`${t("agentLib.synced")} (${outcome.mode})`);
      }
      await refresh();
      if (detailId) setDetail(await api.getAgentDocument(detailId));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  const genVariant = async (agentId: string, tool: string) => {
    setBusy(true);
    try {
      await api.generateAgentVariant(agentId, tool);
      toast.success(t("agentLib.variantGenerated"));
      await refresh();
      if (detailId) setDetail(await api.getAgentDocument(detailId));
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  const doDelete = async () => {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      await api.deleteAgent(deleteTarget.id);
      toast.success(t("agentLib.deleted"));
      setDetail(null);
      setDetailId(null);
      await refresh();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
      setDeleteTarget(null);
    }
  };

  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col overflow-y-auto">
        <div className="mb-4 flex items-center justify-between gap-3">
          <h1 className="flex items-center gap-2 text-lg font-semibold">
            <Bot className="h-5 w-5" />
            {t("agentLib.title")}
          </h1>
          <div className="flex items-center gap-2">
            <div className="relative w-full max-w-[240px]">
              <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t("agentLib.searchPlaceholder")}
                className="app-input w-full pl-9 font-medium"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </div>
            <button
              onClick={() => void refresh()}
              className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm hover:bg-surface-hover"
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("common.refresh")}
            </button>
            <button
              onClick={() => void uploadAgents()}
              disabled={busy}
              className="flex items-center gap-1.5 rounded-md border border-accent px-3 py-1.5 text-sm font-medium text-accent-dark hover:bg-accent-bg disabled:opacity-50"
            >
              <Upload className="h-3.5 w-3.5" />
              {t("agentLib.upload")}
            </button>
            <button
              onClick={() => void openImport()}
              className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:opacity-90"
            >
              <Download className="h-3.5 w-3.5" />
              {t("agentLib.import")}
            </button>
          </div>
        </div>

        {agents.length === 0 ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted">
            <Bot className="h-10 w-10" />
            <p className="text-sm">{t("agentLib.empty")}</p>
          </div>
        ) : visibleAgents.length === 0 ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted">
            {t("agentLib.noSearchResults")}
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
            {visibleAgents.map((agent) => (
              <div
                key={agent.id}
                onClick={() => void openDetail(agent.id)}
                className={cn(
                  "cursor-pointer rounded-lg border border-border bg-surface p-3 transition-colors hover:bg-surface-hover",
                  detailId === agent.id && "border-accent"
                )}
              >
                <div className="mb-1 flex items-center justify-between gap-2">
                  <span className="truncate font-medium">{agent.name}</span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setDeleteTarget({ id: agent.id, name: agent.name });
                    }}
                    className="text-faint transition-colors hover:text-red-500"
                    title={t("common.delete")}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
                <p className="mb-2 line-clamp-2 min-h-[2rem] text-[12px] text-muted">
                  {agent.description ?? "—"}
                </p>
                <div className="flex items-center gap-1">
                  {agentCapableTools.map((tool) => {
                    const deployed = agent.targets.some((tgt) => tgt.tool === tool.key);
                    return (
                      <span
                        key={tool.key}
                        title={`${tool.display_name}: ${deployed ? "✓" : "—"}`}
                        className={cn(
                          "rounded px-1.5 py-0.5 text-[10px] font-semibold",
                          deployed
                            ? "bg-emerald-500/15 text-emerald-600"
                            : "bg-surface-active text-faint"
                        )}
                      >
                        {toolBadge(tool.key)}
                      </span>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Detail drawer */}
      {detail && detailId && (
        <div className="ml-3 flex w-[380px] shrink-0 flex-col overflow-hidden rounded-lg border border-border bg-surface">
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <span className="truncate text-sm font-semibold">
              {agents.find((a) => a.id === detailId)?.name}
            </span>
            <span className="flex shrink-0 items-center gap-1">
              <button
                onClick={() => {
                  const agent = agents.find((a) => a.id === detailId);
                  if (agent?.central_path) {
                    setPublishAgent({ name: agent.name, centralPath: agent.central_path });
                  }
                }}
                className="text-faint transition-colors hover:text-secondary"
                title={t("agentLib.publishTitle")}
              >
                <UploadCloud className="h-4 w-4" />
              </button>
              <button
                onClick={() =>
                  setFeedbackAgent(
                    agents.find((a) => a.id === detailId)?.name ?? null
                  )
                }
                className="text-faint transition-colors hover:text-secondary"
                title={t("feedback.title")}
              >
                <MessageSquarePlus className="h-4 w-4" />
              </button>
              <button
                onClick={() => {
                  setDetail(null);
                  setDetailId(null);
                }}
                className="text-faint hover:text-secondary"
              >
                <X className="h-4 w-4" />
              </button>
            </span>
          </div>
          <div className="border-b border-border px-3 py-2">
            <p className="mb-2 text-[12px] font-medium text-muted">{t("agentLib.deploySection")}</p>
            <div className="space-y-1.5">
              {detail.resolution.map((res) => {
                const deployed = agents
                  .find((a) => a.id === detailId)
                  ?.targets.some((tgt) => tgt.tool === res.tool);
                return (
                  <div key={res.tool} className="flex items-center justify-between gap-2">
                    <span className="flex min-w-0 items-center gap-1.5 text-[12px]">
                      <span className="rounded bg-surface-active px-1.5 py-0.5 text-[10px] font-semibold">
                        {toolBadge(res.tool)}
                      </span>
                      <span className="truncate text-tertiary">
                        {res.resolved_source ?? t("agentLib.missingVariant")}
                      </span>
                    </span>
                    <span className="flex shrink-0 items-center gap-1">
                      {!res.deployable && !deployed && (
                        <button
                          disabled={busy}
                          onClick={() => void genVariant(detailId, res.tool)}
                          className="flex items-center gap-1 rounded border border-border px-2 py-0.5 text-[11px] hover:bg-surface-hover"
                          title={t("agentLib.genVariant")}
                        >
                          <Wand2 className="h-3 w-3" />
                        </button>
                      )}
                      <button
                        disabled={busy || (!res.deployable && !deployed)}
                        onClick={() => void toggleTool(detailId, res.tool, !!deployed)}
                        className={cn(
                          "relative h-5 w-9 rounded-full transition-colors",
                          deployed ? "bg-emerald-500" : "bg-border",
                          !res.deployable && !deployed && "opacity-40"
                        )}
                      >
                        <span
                          className={cn(
                            "absolute top-0.5 h-4 w-4 rounded-full bg-white transition-transform",
                            deployed && "translate-x-4"
                          )}
                        />
                      </button>
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
          <div className="border-b border-border px-3 py-2">
            <p className="mb-2 text-[12px] font-medium text-muted">{t("agentLib.presetSection")}</p>
            {presets.length === 0 ? (
              <p className="text-[12px] text-faint">{t("agentLib.noPresets")}</p>
            ) : (
              <div className="space-y-1.5">
                {presets.map((preset) => {
                  const member = memberPresetIds.includes(preset.id);
                  return (
                    <div key={preset.id} className="flex items-center justify-between gap-2">
                      <span className="min-w-0 flex-1 truncate text-[12px] text-tertiary">
                        {preset.name}
                      </span>
                      <button
                        disabled={busy}
                        onClick={() => void togglePreset(detailId, preset.id, member)}
                        className={cn(
                          "relative h-5 w-9 shrink-0 rounded-full transition-colors",
                          member ? "bg-emerald-500" : "bg-border"
                        )}
                      >
                        <span
                          className={cn(
                            "absolute top-0.5 h-4 w-4 rounded-full bg-white transition-transform",
                            member && "translate-x-4"
                          )}
                        />
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          <div className="min-h-0 flex-1 overflow-auto bg-surface px-3 py-2">
            <SkillMarkdown content={detail.content} />
          </div>
        </div>
      )}

      {/* Import sheet */}
      {importOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          onClick={() => setImportOpen(false)}
        >
          <div
            className="max-h-[70vh] w-[560px] overflow-hidden rounded-lg border border-border bg-surface shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between border-b border-border px-4 py-3">
              <span className="font-medium">{t("agentLib.importTitle")}</span>
              <button onClick={() => setImportOpen(false)} className="text-faint hover:text-secondary">
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="max-h-[calc(70vh-49px)] overflow-y-auto p-4">
              {scans.length === 0 && (
                <p className="text-sm text-muted">{t("agentLib.noScanResults")}</p>
              )}
              {scans.map((scan) => (
                <div key={scan.tool} className="mb-4">
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="flex items-center gap-1.5 text-sm font-medium">
                      <ChevronDown className="h-3.5 w-3.5" />
                      {scan.display_name}
                      <span className="text-[12px] text-faint">({scan.files.length})</span>
                    </span>
                    {scan.files.length > 0 && (
                      <button
                        disabled={busy}
                        onClick={() => void importAllFrom(scan.tool, scan.files)}
                        className="rounded border border-border px-2 py-0.5 text-[11px] hover:bg-surface-hover"
                      >
                        {t("agentLib.importAll")}
                      </button>
                    )}
                  </div>
                  {scan.files.length === 0 ? (
                    <p className="px-2 text-[12px] text-faint">{t("agentLib.noScanResults")}</p>
                  ) : (
                    <div className="space-y-1">
                      {scan.files.map((file) => (
                        <div
                          key={file.path}
                          className="flex items-center justify-between rounded px-2 py-1 hover:bg-surface-hover"
                        >
                          <span className="min-w-0 flex-1 truncate text-[12px]">
                            {file.name_guess}
                            {file.imported_agent_id && (
                              <span className="ml-1.5 text-[10px] text-emerald-600">✓</span>
                            )}
                          </span>
                          <button
                            disabled={busy || !!file.imported_agent_id}
                            onClick={() => void importFile(scan.tool, file.path)}
                            className="shrink-0 rounded border border-border px-2 py-0.5 text-[11px] hover:bg-surface-hover disabled:opacity-40"
                          >
                            {file.imported_agent_id
                              ? t("agentLib.imported")
                              : t("agentLib.import")}
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        message={t("agentLib.deleteConfirm", { name: deleteTarget?.name ?? "" })}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => doDelete()}
      />

      <FeedbackDialog
        open={!!feedbackAgent}
        agent={feedbackAgent ?? undefined}
        onClose={() => setFeedbackAgent(null)}
      />

      <AgentPublishDialog
        open={!!publishAgent}
        agentName={publishAgent?.name}
        centralPath={publishAgent?.centralPath}
        onClose={() => setPublishAgent(null)}
      />
    </div>
  );
}
