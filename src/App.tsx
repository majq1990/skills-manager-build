import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Toaster } from "sonner";
import { AppProvider } from "./context/AppContext";
import { ThemeProvider, useThemeContext } from "./context/ThemeContext";
import { AuthProvider } from "./context/AuthContext";
import { SkillMarket } from "./views/SkillMarket";
import { McpMarket } from "./views/McpMarket";
import { HelpDialog } from "./components/HelpDialog";
import { CloseActionGuard } from "./components/CloseActionGuard";
import { Layout } from "./components/Layout";
import { Dashboard } from "./views/Dashboard";
import { MySkills } from "./views/MySkills";
import { InstallSkills } from "./views/InstallSkills";
import { Settings } from "./views/Settings";
import { ProjectDetail } from "./views/ProjectDetail";

function ThemedToaster() {
  const { resolvedTheme } = useThemeContext();
  return (
    <Toaster
      theme={resolvedTheme}
      position="bottom-right"
      toastOptions={{
        style: {
          background: "var(--color-surface)",
          border: "1px solid var(--color-border)",
          color: "var(--color-text-primary)",
        },
      }}
    />
  );
}

function App() {
  return (
    <ThemeProvider>
      <AppProvider>
        <AuthProvider>
          <BrowserRouter>
            <Routes>
              <Route element={<Layout />}>
                <Route path="/" element={<Dashboard />} />
                <Route path="/my-skills" element={<MySkills />} />
                <Route path="/install" element={<InstallSkills />} />
                <Route path="/project/:id" element={<ProjectDetail />} />
                <Route path="/settings" element={<Settings />} />
                <Route path="/market" element={<SkillMarket />} />
                <Route path="/mcp-market" element={<McpMarket />} />
              </Route>
            </Routes>
            <HelpDialog />
            <CloseActionGuard />
          </BrowserRouter>
        </AuthProvider>
        <ThemedToaster />
      </AppProvider>
    </ThemeProvider>
  );
}

export default App;
