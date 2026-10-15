import { useMemo, useRef, useState } from "react";
import { X, Plus, Tag as TagIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import { MAX_TAG_LEN, MAX_TAG_COUNT, sanitizeTag } from "../lib/tags";

interface Props {
  value: string[];
  onChange: (tags: string[]) => void;
  /** 受控词表（来自 enterprise_suggest_tags 的 vocabulary），用于候选下拉 */
  vocabulary?: string[];
  disabled?: boolean;
}

/** 发布弹窗的标签输入：chips + 回车确认 + 受控词表候选下拉 + 少量自定义。 */
export function TagChipsInput({ value, onChange, vocabulary = [], disabled }: Props) {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const atMax = value.length >= MAX_TAG_COUNT;
  const selected = useMemo(() => new Set(value), [value]);

  const candidates = useMemo(() => {
    const needle = input.trim().toLowerCase();
    const unique = Array.from(new Set(vocabulary));
    const matched: Array<{ tag: string; custom: boolean }> = unique
      .filter((tag) => !selected.has(tag))
      .filter((tag) => (needle ? tag.toLowerCase().includes(needle) : true))
      .slice(0, 6)
      .map((tag) => ({ tag, custom: false }));
    const typed = sanitizeTag(input);
    const showCustom =
      typed.length > 0 && !unique.some((tag) => tag.toLowerCase() === typed.toLowerCase());
    return showCustom ? [{ tag: typed, custom: true }, ...matched] : matched;
  }, [vocabulary, input, selected]);

  const commit = (raw: string) => {
    const tag = sanitizeTag(raw);
    if (!tag || selected.has(tag) || atMax) {
      setInput("");
      return;
    }
    onChange([...value, tag]);
    setInput("");
    inputRef.current?.focus();
  };

  const remove = (tag: string) => {
    onChange(value.filter((item) => item !== tag));
    inputRef.current?.focus();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      // 回车必须阻止默认行为，否则会顺手提交整个发布表单
      e.preventDefault();
      commit(input);
    } else if (e.key === "," || e.key === "，") {
      e.preventDefault();
      commit(input);
    } else if (e.key === "Backspace" && input === "" && value.length > 0) {
      e.preventDefault();
      remove(value[value.length - 1]);
    } else if (e.key === "Escape") {
      setInput("");
    }
  };

  return (
    <div>
      <div className="flex flex-wrap items-center gap-1.5">
        {value.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1 rounded-full bg-accent-bg px-2 py-0.5 text-[12px] font-medium text-accent-light"
          >
            {tag}
            <button
              type="button"
              onClick={() => remove(tag)}
              disabled={disabled}
              className="text-accent-light/70 hover:text-red-500 disabled:opacity-50"
              title={t("publish.tagsRemove", { tag })}
            >
              <X className="h-2.5 w-2.5" />
            </button>
          </span>
        ))}
        {!atMax && (
          <div className="relative">
            <input
              ref={inputRef}
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={() => {
                // 失焦时把已输入内容收下，避免"打了字没回车就发布"丢标签
                if (input.trim()) commit(input);
              }}
              disabled={disabled}
              maxLength={MAX_TAG_LEN}
              placeholder={value.length === 0 ? t("publish.tagsPlaceholder") : ""}
              className={cn(
                "h-6 rounded-full border border-border-subtle bg-transparent px-2 text-[12px] text-secondary outline-none focus:border-accent",
                value.length === 0 ? "w-40" : "w-20"
              )}
            />
            {candidates.length > 0 && input.trim() && (
              <div className="absolute left-0 top-7 z-10 min-w-[140px] max-w-[220px] rounded-md border border-border-subtle bg-surface p-1 shadow-lg">
                {candidates.map(({ tag, custom }) => (
                  <button
                    key={custom ? `custom:${tag}` : tag}
                    type="button"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => commit(tag)}
                    className="w-full truncate rounded px-1.5 py-1 text-left text-[12px] text-secondary hover:bg-surface-hover"
                  >
                    {custom ? t("publish.tagsAddCustom", { tag }) : tag}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
        {!atMax && input.trim() && (
          <button
            type="button"
            onClick={() => commit(input)}
            disabled={disabled}
            className="inline-flex items-center rounded-full border border-border-subtle p-0.5 text-muted transition-colors hover:border-accent hover:text-accent-light disabled:opacity-50"
            title={t("publish.tagsAdd")}
          >
            <Plus className="h-3 w-3" />
          </button>
        )}
      </div>
      {atMax ? (
        <p className="mt-1 flex items-center gap-1 text-[12px] text-faint">
          <TagIcon className="h-3 w-3" />
          {t("publish.tagsMaxReached", { max: MAX_TAG_COUNT })}
        </p>
      ) : (
        <p className="mt-1 text-[12px] text-faint">
          {t("publish.tagsHint", { max: MAX_TAG_COUNT })}
        </p>
      )}
    </div>
  );
}
