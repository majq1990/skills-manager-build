import type { LucideIcon } from "lucide-react";
import {
  Blocks,
  BookOpen,
  Briefcase,
  Code2,
  FolderGit2,
  NotebookPen,
  Palette,
  Plane,
  Rocket,
  Wrench,
} from "lucide-react";
import type { Scenario } from "./tauri";

export interface ScenarioIconOption {
  key: string;
  label: string;
  icon: LucideIcon;
  colorClass: string;
  activeClass: string;
}

export const SCENARIO_ICON_OPTIONS: ScenarioIconOption[] = [
  {
    key: "briefcase",
    label: "Work",
    icon: Briefcase,
    colorClass: "text-amber-300",
    activeClass: "border-amber-500/30 bg-amber-500/12",
  },
  {
    key: "book-open",
    label: "Study",
    icon: BookOpen,
    colorClass: "text-emerald-300",
    activeClass: "border-emerald-500/30 bg-emerald-500/12",
  },
  {
    key: "folder-git-2",
    label: "Open Source",
    icon: FolderGit2,
    colorClass: "text-rose-300",
    activeClass: "border-rose-500/30 bg-rose-500/12",
  },
  {
    key: "plane",
    label: "Travel",
    icon: Plane,
    colorClass: "text-yellow-300",
    activeClass: "border-yellow-500/30 bg-yellow-500/12",
  },
  {
    key: "code-2",
    label: "Build",
    icon: Code2,
    colorClass: "text-cyan-300",
    activeClass: "border-cyan-500/30 bg-cyan-500/12",
  },
  {
    key: "rocket",
    label: "Launch",
    icon: Rocket,
    colorClass: "text-orange-300",
    activeClass: "border-orange-500/30 bg-orange-500/12",
  },
  {
    key: "notebook-pen",
    label: "Notes",
    icon: NotebookPen,
    colorClass: "text-orange-300",
    activeClass: "border-orange-500/30 bg-orange-500/12",
  },
  {
    key: "blocks",
    label: "Systems",
    icon: Blocks,
    colorClass: "text-teal-300",
    activeClass: "border-teal-500/30 bg-teal-500/12",
  },
  {
    key: "palette",
    label: "Design",
    icon: Palette,
    colorClass: "text-pink-300",
    activeClass: "border-pink-500/30 bg-pink-500/12",
  },
  {
    key: "wrench",
    label: "Ops",
    icon: Wrench,
    colorClass: "text-zinc-300",
    activeClass: "border-zinc-500/30 bg-zinc-500/10",
  },
];

const SCENARIO_ICON_MAP = new Map(
  SCENARIO_ICON_OPTIONS.map((option) => [option.key, option] as const)
);

const SCENARIO_KEYWORD_RULES: Array<{ key: string; keywords: string[] }> = [
  { key: "briefcase", keywords: ["工作", "work", "office", "client"] },
  { key: "book-open", keywords: ["学习", "study", "learn", "course", "research"] },
  { key: "folder-git-2", keywords: ["开源", "opensource", "open source", "github"] },
  { key: "plane", keywords: ["旅行", "travel", "trip", "holiday"] },
  { key: "code-2", keywords: ["开发", "code", "build", "app"] },
  { key: "notebook-pen", keywords: ["笔记", "note", "write", "journal"] },
  { key: "palette", keywords: ["设计", "design", "brand", "ui"] },
];

export function inferScenarioIconKey(scenario?: Pick<Scenario, "name" | "description" | "icon"> | null) {
  if (scenario?.icon && SCENARIO_ICON_MAP.has(scenario.icon)) {
    return scenario.icon;
  }

  const haystack = `${scenario?.name || ""} ${scenario?.description || ""}`.toLowerCase();
  const matched = SCENARIO_KEYWORD_RULES.find((rule) =>
    rule.keywords.some((keyword) => haystack.includes(keyword))
  );

  return matched?.key || "briefcase";
}

export function getScenarioIconOption(
  scenario?: Pick<Scenario, "name" | "description" | "icon"> | string | null
) {
  const key =
    typeof scenario === "string"
      ? scenario
      : inferScenarioIconKey(scenario);
  return SCENARIO_ICON_MAP.get(key) || SCENARIO_ICON_OPTIONS[0];
}
