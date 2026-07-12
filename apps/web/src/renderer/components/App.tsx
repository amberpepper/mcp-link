import React, { useState, useEffect } from "react";
import {
  Routes,
  Route,
  Navigate,
  useLocation,
  useParams,
} from "react-router-dom";
import { Sonner } from "@mcp_link/ui";
import Home from "./Home";
import { useTranslation } from "react-i18next";
import SidebarComponent from "./Sidebar";
import { SidebarProvider } from "@mcp_link/ui";
import LogViewer from "@/renderer/components/mcp/log/LogViewer";
import Settings from "./setting/Settings";
import KeyManager from "./keys/KeyManager";
import { useServerStore, initializeStores } from "../stores";
import { usePlatformAPI } from "@/renderer/platform-api";
import { IconProgress } from "@tabler/icons-react";
import SkillsPage from "./skills/SkillsPage";
import SkillMarket from "./skills/SkillMarket";
import HttpLogin from "./auth/HttpLogin";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import { HTTP_ACCESS_TOKEN_KEY } from "@/renderer/platform-api/http-platform-api";
import AddServerPage from "@/renderer/components/mcp/server/AddServerPage";
import ServerDetailPage from "@/renderer/components/mcp/server/ServerDetailPage";
import HooksPage from "@/renderer/components/hooks/HooksPage";
import HookEditPage from "@/renderer/components/hooks/HookEditPage";
import MarketSourcesPage from "@/renderer/components/markets/MarketSourcesPage";

// Main App component
const App: React.FC = () => {
  const { t, i18n } = useTranslation();
  const location = useLocation();
  const platformAPI = usePlatformAPI();

  // Zustand stores
  const { refreshServers } = useServerStore();

  // Local state for loading and temporary UI states
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const hasPlatformAccess =
    isTauriRuntime() ||
    Boolean(window.localStorage.getItem(HTTP_ACCESS_TOKEN_KEY));
  const isHttpAuthRoute =
    !isTauriRuntime() &&
    !hasPlatformAccess &&
    (location.pathname === "/login" || location.pathname === "/setup");

  // Initialize stores
  useEffect(() => {
    const initializeApp = async () => {
      if (!hasPlatformAccess) {
        setIsLoading(false);
        return;
      }

      try {
        // Initialize all stores
        await initializeStores();

        const settings = await platformAPI.settings.get();
        if (settings.language) {
          await i18n.changeLanguage(settings.language);
        }
      } catch (error) {
        console.error("Failed to initialize app:", error);
      } finally {
        setIsLoading(false);
      }
    };

    initializeApp();
  }, [platformAPI, i18n, hasPlatformAccess]);

  // Refresh servers on initial load only
  useEffect(() => {
    if (!hasPlatformAccess) return;
    refreshServers();
  }, [refreshServers, hasPlatformAccess]);

  // Simple polling: refresh server list every 3 seconds
  useEffect(() => {
    if (!hasPlatformAccess) return;
    const id = setInterval(() => {
      // Ignore errors to keep polling resilient
      refreshServers().catch(() => {});
    }, 3000);
    return () => clearInterval(id);
  }, [refreshServers, hasPlatformAccess]);

  // Loading indicator component to reuse
  const LoadingIndicator = () => (
    <div className="flex h-full items-center justify-center bg-content-light">
      <div className="text-center">
        <IconProgress className="h-10 w-10 mx-auto animate-spin text-primary" />
        <p className="mt-4 text-muted-foreground">{t("common.loading")}</p>
      </div>
    </div>
  );

  // If still loading, show loading indicator
  if (isLoading) {
    return <LoadingIndicator />;
  }

  const requireHttpAuth = (element: React.ReactElement) => {
    if (isTauriRuntime()) return element;
    if (window.localStorage.getItem(HTTP_ACCESS_TOKEN_KEY)) return element;
    return <Navigate to="/login" replace />;
  };

  return (
    <SidebarProvider defaultOpen={true} className="h-full">
      <Sonner />

      {!isHttpAuthRoute && <SidebarComponent />}
      <main className="flex flex-col flex-1 w-full min-w-0 overflow-auto">
        <div className="flex flex-col flex-1">
          <Routes>
            <Route path="/login" element={<HttpLogin />} />
            <Route path="/setup" element={<HttpLogin />} />
            <Route path="/" element={<Navigate to="/servers" replace />} />
            <Route path="/servers" element={requireHttpAuth(<Home />)} />
            <Route
              path="/servers/add"
              element={requireHttpAuth(<AddServerPage />)}
            />
            <Route
              path="/servers/:id"
              element={requireHttpAuth(<ServerDetailPage />)}
            />
            <Route path="/logs" element={requireHttpAuth(<LogViewer />)} />
            <Route path="/keys" element={requireHttpAuth(<KeyManager />)} />
            <Route path="/hooks" element={requireHttpAuth(<HooksPage />)} />
            <Route
              path="/hooks/new"
              element={requireHttpAuth(<HookEditPage mode="new" />)}
            />
            <Route
              path="/hooks/:id"
              element={requireHttpAuth(<HookEditPage />)}
            />
            <Route
              path="/workflows"
              element={<Navigate to="/hooks" replace />}
            />
            <Route
              path="/workflows/:workflowId"
              element={<WorkflowRedirect />}
            />
            <Route path="/settings" element={requireHttpAuth(<Settings />)} />
            <Route
              path="/market-sources"
              element={requireHttpAuth(<MarketSourcesPage />)}
            />
            <Route path="/skills" element={requireHttpAuth(<SkillsPage />)} />
            <Route
              path="/skills/market"
              element={requireHttpAuth(<SkillMarket />)}
            />
            <Route
              path="/skills/agents"
              element={<Navigate to="/skills" replace />}
            />

            <Route path="*" element={<Navigate to="/servers" />} />
          </Routes>
        </div>
      </main>
    </SidebarProvider>
  );
};

const WorkflowRedirect: React.FC = () => {
  const { workflowId } = useParams<{ workflowId?: string }>();
  return (
    <Navigate to={workflowId ? `/hooks/${workflowId}` : "/hooks"} replace />
  );
};

export default App;
