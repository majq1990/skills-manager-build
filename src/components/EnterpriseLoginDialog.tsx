import { useState } from "react";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

interface Props {
  open: boolean;
  onClose: () => void;
  onLogin: (username: string, password: string) => Promise<void>;
}

export function EnterpriseLoginDialog({ open, onClose, onLogin }: Props) {
  const { t } = useTranslation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await onLogin(username, password);
      setUsername("");
      setPassword("");
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" onClick={onClose} />
      <div className="relative w-[400px] app-panel p-6 shadow-2xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-[13px] font-semibold text-primary">{t("enterprise.login.title")}</h2>
          <button onClick={onClose} className="text-muted hover:text-secondary p-1 rounded transition-colors outline-none">
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-[13px] font-medium text-tertiary mb-1">{t("enterprise.login.username")}</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="app-input w-full"
              required
              autoComplete="username"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-[13px] font-medium text-tertiary mb-1">{t("enterprise.login.password")}</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="app-input w-full"
              required
              autoComplete="current-password"
            />
          </div>
          {error && (
            <div className="text-[13px] text-danger">{error}</div>
          )}
          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="app-button-secondary"
              disabled={loading}
            >
              {t("common.cancel")}
            </button>
            <button
              type="submit"
              disabled={loading || !username || !password}
              className="app-button-primary"
            >
              {loading ? t("enterprise.login.loggingIn") : t("enterprise.login.login")}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
