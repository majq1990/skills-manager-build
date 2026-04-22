import { Trash2, CheckCircle2, Circle, Upload } from "lucide-react";
import { cn } from "../utils";

interface MultiSelectToolbarLabels {
  hint: string;
  selected: string;
  delete: string;
  enable: string;
  disable: string;
  selectAll: string;
  deselectAll: string;
  cancel: string;
  publish?: string;
}

interface MultiSelectToolbarProps {
  selectedCount: number;
  isAllSelected: boolean;
  anyDisabled: boolean;
  showToggle: boolean;
  canPublish?: boolean;
  labels: MultiSelectToolbarLabels;
  onDelete: () => void;
  onToggle: () => void;
  onSelectAll: () => void;
  onCancel: () => void;
  onPublish?: () => void;
}

export function MultiSelectToolbar({
  selectedCount,
  isAllSelected,
  anyDisabled,
  showToggle,
  canPublish,
  labels,
  onDelete,
  onToggle,
  onSelectAll,
  onCancel,
  onPublish,
}: MultiSelectToolbarProps) {
  return (
    <div className="flex items-center gap-2 px-1 py-1.5">
      <span className="text-[13px] text-muted">
        {selectedCount > 0 ? labels.selected : labels.hint}
      </span>
      {selectedCount > 0 && (
        <>
          <button
            onClick={onDelete}
            className="inline-flex items-center gap-1.5 rounded-md bg-red-600/90 px-2.5 py-1 text-[13px] font-medium text-white hover:bg-red-500 transition-colors"
          >
            <Trash2 className="h-3.5 w-3.5" />
            {labels.delete}
          </button>
          {showToggle && (
            <button
              onClick={onToggle}
              className={cn(
                "inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[13px] font-medium text-white transition-colors",
                anyDisabled
                  ? "bg-emerald-600/90 hover:bg-emerald-500"
                  : "bg-amber-600/90 hover:bg-amber-500"
              )}
            >
              {anyDisabled
                ? <CheckCircle2 className="h-3.5 w-3.5" />
                : <Circle className="h-3.5 w-3.5" />}
              {anyDisabled ? labels.enable : labels.disable}
            </button>
          )}
          {canPublish && onPublish && labels.publish && (
            <button
              onClick={onPublish}
              className="inline-flex items-center gap-1.5 rounded-md bg-accent-dark px-2.5 py-1 text-[13px] font-medium text-white hover:bg-accent transition-colors"
            >
              <Upload className="h-3.5 w-3.5" />
              {labels.publish}
            </button>
          )}
        </>
      )}
      <button
        onClick={onSelectAll}
        className="rounded-md px-2.5 py-1 text-[13px] font-medium text-muted hover:text-secondary hover:bg-surface-hover transition-colors"
      >
        {isAllSelected ? labels.deselectAll : labels.selectAll}
      </button>
      <button
        onClick={onCancel}
        className="rounded-md px-2.5 py-1 text-[13px] font-medium text-muted hover:text-secondary hover:bg-surface-hover transition-colors"
      >
        {labels.cancel}
      </button>
    </div>
  );
}
