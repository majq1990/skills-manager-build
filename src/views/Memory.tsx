import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Bot,
  Brain,
  CheckCircle2,
  Database,
  Loader2,
  Plus,
  RefreshCw,
  Search,
} from "lucide-react";
import { toast } from "sonner";
import * as api from "../lib/tauri";
import type { MemorySyncReport } from "../lib/tauri";

export function Memory() {
  const [report, setReport] = useState<MemorySyncReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [content, setContent] = useState("");
  const [memoryType, setMemoryType] = useState("reference");

  const load = async () => {
    try {
      setReport(await api.memoryGetStatus());
    } catch (error) {
      toast.error(`读取统一记忆状态失败：${String(error)}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void load(); }, []);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return report?.memories ?? [];
    return (report?.memories ?? []).filter((item) =>
      `${item.slug} ${item.description} ${item.memory_type}`.toLowerCase().includes(needle)
    );
  }, [query, report]);

  const runSync = async () => {
    setSyncing(true);
    try {
      const next = await api.memorySync();
      setReport(next);
      const r = next.reconcile;
      toast.success(`统一完成：${r.imported + r.replaced_newer} 条汇入，已分发到 ${next.tools_installed} 个 Agent`);
    } catch (error) {
      toast.error(`统一记忆失败：${String(error)}`);
    } finally {
      setSyncing(false);
    }
  };

  const saveMemory = async () => {
    if (!title.trim() || !content.trim()) {
      toast.error("标题和内容不能为空");
      return;
    }
    setSyncing(true);
    try {
      await api.memoryRemember(title, description, content, memoryType, "skills-manager-gui");
      setTitle("");
      setDescription("");
      setContent("");
      setShowCreate(false);
      await load();
      toast.success("记忆已写入统一仓并同步到各 Agent");
    } catch (error) {
      toast.error(`保存失败：${String(error)}`);
    } finally {
      setSyncing(false);
    }
  };

  if (loading) {
    return <div className="h-full flex items-center justify-center text-muted"><Loader2 className="w-5 h-5 animate-spin" /></div>;
  }

  const reconciliation = report?.reconcile;
  const sourceFiles = reconciliation?.sources.reduce((sum, source) => sum + source.files, 0) ?? 0;

  return (
    <div className="h-full overflow-auto bg-base">
      <div className="max-w-6xl mx-auto px-8 py-7 space-y-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2.5">
              <Brain className="w-6 h-6 text-accent" />
              <h1 className="text-xl font-semibold text-primary">统一记忆</h1>
            </div>
            <p className="mt-1.5 text-sm text-tertiary">
              各 Agent 写入统一仓或原生 memory 目录，Skills Manager 负责汇总、去重、备份并跨 Agent 分发。
            </p>
            <p className="mt-1 text-xs font-mono text-muted break-all">{report?.shared_root}</p>
          </div>
          <div className="flex gap-2">
            <button onClick={() => setShowCreate((v) => !v)} className="inline-flex items-center gap-2 px-3 py-2 rounded-md border border-border text-sm text-secondary hover:bg-surface-hover">
              <Plus className="w-4 h-4" />新增记忆
            </button>
            <button onClick={runSync} disabled={syncing} className="inline-flex items-center gap-2 px-3 py-2 rounded-md bg-accent text-white text-sm hover:opacity-90 disabled:opacity-60">
              {syncing ? <Loader2 className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
              汇总并同步
            </button>
          </div>
        </div>

        {showCreate && (
          <section className="rounded-lg border border-border bg-surface p-5 space-y-3">
            <h2 className="text-sm font-semibold text-primary">写入统一记忆仓</h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="标题" className="px-3 py-2 rounded-md border border-border bg-base text-sm text-primary outline-none focus:border-accent" />
              <select value={memoryType} onChange={(e) => setMemoryType(e.target.value)} className="px-3 py-2 rounded-md border border-border bg-base text-sm text-primary outline-none focus:border-accent">
                <option value="reference">参考资料</option><option value="project">项目</option><option value="feedback">反馈/偏好</option><option value="user">用户信息</option>
              </select>
            </div>
            <input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="一句话说明（用于跨 Agent 语义召回）" className="w-full px-3 py-2 rounded-md border border-border bg-base text-sm text-primary outline-none focus:border-accent" />
            <textarea value={content} onChange={(e) => setContent(e.target.value)} placeholder="记忆内容（Markdown）" rows={6} className="w-full px-3 py-2 rounded-md border border-border bg-base text-sm text-primary outline-none focus:border-accent resize-y" />
            <div className="flex justify-end gap-2"><button onClick={() => setShowCreate(false)} className="px-3 py-2 text-sm text-tertiary">取消</button><button onClick={saveMemory} disabled={syncing} className="px-3 py-2 rounded-md bg-accent text-white text-sm disabled:opacity-60">保存并同步</button></div>
          </section>
        )}

        <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
          {[
            [Database, "统一记忆", report?.memory_count ?? 0],
            [Bot, "已安装 Agent", report?.tools_installed ?? 0],
            [RefreshCw, "原生目录文件", sourceFiles],
            [CheckCircle2, "待汇入/替换", (reconciliation?.imported ?? 0) + (reconciliation?.replaced_newer ?? 0)],
          ].map(([Icon, label, value]) => {
            const C = Icon as typeof Database;
            return <div key={String(label)} className="rounded-lg border border-border bg-surface p-4"><C className="w-4 h-4 text-accent mb-3" /><div className="text-2xl font-semibold text-primary">{String(value)}</div><div className="text-xs text-muted mt-1">{String(label)}</div></div>;
          })}
        </div>

        {(reconciliation?.errors.length ?? 0) > 0 && (
          <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-4 text-sm text-amber-700 dark:text-amber-300">
            <div className="flex items-center gap-2 font-medium"><AlertTriangle className="w-4 h-4" />存在 {reconciliation?.errors.length} 个采集错误</div>
            <ul className="mt-2 list-disc pl-5 text-xs space-y-1">{reconciliation?.errors.map((e) => <li key={e}>{e}</li>)}</ul>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <section className="rounded-lg border border-border bg-surface overflow-hidden">
            <div className="px-4 py-3 border-b border-border text-sm font-semibold text-primary">原生记忆来源</div>
            <div className="divide-y divide-border-subtle">
              {(reconciliation?.sources ?? []).length === 0 ? <div className="p-4 text-sm text-muted">未发现独立的 Agent memory 目录；各 Agent 可直接写统一仓。</div> : reconciliation?.sources.map((source) => (
                <div key={`${source.agent}-${source.path}`} className="px-4 py-3 flex items-center justify-between gap-3"><div className="min-w-0"><div className="text-sm text-secondary capitalize">{source.agent}</div><div className="text-xs font-mono text-muted truncate">{source.path}</div></div><span className="text-xs text-muted shrink-0">{source.files} 条</span></div>
              ))}
            </div>
          </section>
          <section className="rounded-lg border border-border bg-surface overflow-hidden">
            <div className="px-4 py-3 border-b border-border text-sm font-semibold text-primary">跨 Agent 分发</div>
            <div className="divide-y divide-border-subtle max-h-72 overflow-auto">
              {(report?.tools ?? []).map((tool) => (
                <div key={tool.tool_key} className="px-4 py-3 flex items-center justify-between gap-3"><div><div className="text-sm text-secondary">{tool.display_name}</div><div className="text-xs text-muted">{tool.adapter}</div></div><div className="text-right"><div className={tool.error ? "text-xs text-red-500" : "text-xs text-emerald-500"}>{tool.error ? "异常" : "就绪"}</div><div className="text-xs text-muted">{tool.deployed} 条</div></div></div>
              ))}
            </div>
          </section>
        </div>

        <section className="rounded-lg border border-border bg-surface overflow-hidden">
          <div className="px-4 py-3 border-b border-border flex items-center justify-between gap-3"><div className="text-sm font-semibold text-primary">统一记忆清单</div><div className="relative w-72 max-w-full"><Search className="absolute left-2.5 top-2.5 w-3.5 h-3.5 text-muted" /><input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索标题、说明或类型" className="w-full pl-8 pr-3 py-2 rounded-md border border-border bg-base text-xs text-primary outline-none focus:border-accent" /></div></div>
          <div className="divide-y divide-border-subtle max-h-[420px] overflow-auto">
            {filtered.map((item) => <div key={item.source_path} className="px-4 py-3"><div className="flex items-center gap-2"><span className="text-sm font-medium text-secondary">{item.slug}</span><span className="px-1.5 py-0.5 rounded bg-surface-hover text-[10px] text-muted">{item.memory_type}</span></div><div className="mt-1 text-xs text-tertiary">{item.description.replace(/^\[memory\]\s*/, "")}</div></div>)}
          </div>
        </section>
      </div>
    </div>
  );
}
