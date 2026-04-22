import { createContext } from "react";

interface AuthState {
  isAuthenticated: boolean;
  username: string | null;
  department: string | null;
  isSupportDept: boolean;
  loading: boolean;
  error: string | null;
}

interface AuthContextValue extends AuthState {
  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  validateToken: () => Promise<boolean>;
}

export const AuthContext = createContext<AuthContextValue | null>(null);
