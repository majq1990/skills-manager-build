import { useState, useEffect, useCallback, type ReactNode } from "react";
import { getAuthStatus, enterpriseLogin, enterpriseLogout, validateToken } from "../lib/tauri";
import { toast } from "sonner";
import { AuthContext } from "./auth";

interface AuthState {
  isAuthenticated: boolean;
  username: string | null;
  department: string | null;
  isSupportDept: boolean;
  loading: boolean;
  error: string | null;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [authState, setAuthState] = useState<AuthState>({
    isAuthenticated: false,
    username: null,
    department: null,
    isSupportDept: false,
    loading: true,
    error: null,
  });

  const loadAuthStatus = useCallback(async () => {
    try {
      const status = await getAuthStatus();
      setAuthState({
        isAuthenticated: status.authenticated,
        username: status.username,
        department: status.department,
        isSupportDept: status.is_support_dept,
        loading: false,
        error: null,
      });
    } catch (err) {
      setAuthState((prev) => ({
        ...prev,
        loading: false,
        error: err instanceof Error ? err.message : "Failed to load auth status",
      }));
    }
  }, []);

  const login = useCallback(async (username: string, password: string) => {
    setAuthState((prev) => ({ ...prev, loading: true, error: null }));
    try {
      const result = await enterpriseLogin(username, password);
      setAuthState({
        isAuthenticated: true,
        username: result.username,
        department: result.department,
        isSupportDept: result.is_support_dept,
        loading: false,
        error: null,
      });
      toast.success("登录成功");
    } catch (err) {
      setAuthState((prev) => ({
        ...prev,
        loading: false,
        error: err instanceof Error ? err.message : "登录失败",
      }));
      toast.error(err instanceof Error ? err.message : "登录失败");
    }
  }, []);

  const logout = useCallback(async () => {
    try {
      await enterpriseLogout();
      setAuthState({
        isAuthenticated: false,
        username: null,
        department: null,
        isSupportDept: false,
        loading: false,
        error: null,
      });
      toast.success("已退出登录");
    } catch (err) {
      setAuthState((prev) => ({
        ...prev,
        error: err instanceof Error ? err.message : "退出登录失败",
      }));
      toast.error(err instanceof Error ? err.message : "退出登录失败");
    }
  }, []);

  const validateTokenFn = useCallback(async (): Promise<boolean> => {
    try {
      const isValid = await validateToken();
      if (!isValid) {
        await logout();
        toast.error("登录已过期，请重新登录");
        return false;
      }
      return true;
    } catch (err) {
      await logout();
      toast.error(err instanceof Error ? err.message : "登录验证失败");
      return false;
    }
  }, [logout]);

  useEffect(() => {
    loadAuthStatus();
  }, [loadAuthStatus]);

  return (
    <AuthContext.Provider
      value={{
        ...authState,
        login,
        logout,
        validateToken: validateTokenFn,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}
