import React, { lazy, Suspense, useEffect, useState } from "react";
import {
  Routes,
  Route,
  Navigate,
  useLocation,
  useParams,
} from "react-router-dom";
import { Sonner } from "@mcp_link/ui";
import { useTranslation } from "react-i18next";
import SidebarComponent from "./Sidebar";
import { SidebarProvider } from "@mcp_link/ui";
import { useServerStore, initializeStores } from "../stores";
import { usePlatformAPI } from "@/renderer/platform-api";
import { IconProgress } from "@tabler/icons-react";
import HttpLogin from "./auth/HttpLogin";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import {
  HTTP_AUTH_CHANGED_EVENT,
  verifyHttpSession,
} from "@/renderer/platform-api/http-platform-api";
import AgentPluginDropInstaller from "@/renderer/components/agents/AgentPluginDropInstaller";

const Home = lazy(() => import("./Home"));
const LogViewer = lazy(() => import("@/renderer/components/mcp/log/LogViewer"));
const Settings = lazy(() => import("./setting/Settings"));
const KeyManager = lazy(() => import("./keys/KeyManager"));
const SkillsPage = lazy(() => import("./skills/SkillsPage"));
const SkillMarket = lazy(() => import("./skills/SkillMarket"));
const AddServerPage = lazy(
  () => import("@/renderer/components/mcp/server/AddServerPage"),
);
const ServerDetailPage = lazy(
  () => import("@/renderer/components/mcp/server/ServerDetailPage"),
);
const HooksPage = lazy(() => import("@/renderer/components/hooks/HooksPage"));
const HookEditPage = lazy(
  () => import("@/renderer/components/hooks/HookEditPage"),
);
const MarketSourcesPage = lazy(
  () => import("@/renderer/components/markets/MarketSourcesPage"),
);
const SessionsPage = lazy(
  () => import("@/renderer/components/sessions/SessionsPage"),
);
const AgentsPage = lazy(
  () => import("@/renderer/components/agents/AgentsPage"),
);
const AgentManagementPage = lazy(
  () => import("@/renderer/components/agents/AgentManagementPage"),
);
const GatewayPage = lazy(
  () => import("@/renderer/components/gateway/GatewayPage"),
);

// Main App component
const App: React.FC = () => {
  const { t, i18n } = useTranslation();
  const location = useLocation();
  const platformAPI = usePlatformAPI();
  const isDesktopRuntime = isTauriRuntime();

  // Zustand stores
  const { refreshServers } = useServerStore();

  // Local state for loading and temporary UI states
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [httpAuthState, setHttpAuthState] = useState<
    "checking" | "authenticated" | "unauthenticated"
  >(isDesktopRuntime ? "authenticated" : "checking");
  const hasPlatformAccess =
    isDesktopRuntime || httpAuthState === "authenticated";
  const isHttpAuthRoute =
    !isDesktopRuntime &&
    !hasPlatformAccess &&
    (location.pathname === "/login" || location.pathname === "/setup");

  useEffect(() => {
    if (isDesktopRuntime) return;
    let active = true;
    const handleAuthChanged = (event: Event) => {
      const authenticated = (event as CustomEvent<{ authenticated: boolean }>)
        .detail?.authenticated;
      setHttpAuthState(authenticated ? "authenticated" : "unauthenticated");
    };
    window.addEventListener(HTTP_AUTH_CHANGED_EVENT, handleAuthChanged);
    void verifyHttpSession().then((authenticated) => {
      if (active) {
        setHttpAuthState(authenticated ? "authenticated" : "unauthenticated");
      }
    });
    return () => {
      active = false;
      window.removeEventListener(HTTP_AUTH_CHANGED_EVENT, handleAuthChanged);
    };
  }, [isDesktopRuntime]);

  // Initialize stores
  useEffect(() => {
    const initializeApp = async () => {
      if (!hasPlatformAccess) {
        setIsLoading(false);
        return;
      }
      setIsLoading(true);

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
  if (isLoading || (!isDesktopRuntime && httpAuthState === "checking")) {
    return <LoadingIndicator />;
  }

  const requireHttpAuth = (element: React.ReactElement) => {
    if (hasPlatformAccess) return element;
    return <Navigate to="/login" replace />;
  };

  return (
    <SidebarProvider
      defaultOpen={true}
      className="!h-svh !min-h-0 max-h-svh overflow-hidden"
    >
      <Sonner />
      <AgentPluginDropInstaller
        enabled={hasPlatformAccess && !isHttpAuthRoute}
      />

      {!isHttpAuthRoute && <SidebarComponent />}
      <main className="flex h-full min-h-0 flex-1 flex-col w-full min-w-0 overflow-hidden">
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <Suspense fallback={<LoadingIndicator />}>
            <Routes>
              <Route
                path="/login"
                element={
                  hasPlatformAccess ? (
                    <Navigate to="/sessions" replace />
                  ) : (
                    <HttpLogin />
                  )
                }
              />
              <Route
                path="/setup"
                element={
                  hasPlatformAccess ? (
                    <Navigate to="/sessions" replace />
                  ) : (
                    <HttpLogin />
                  )
                }
              />
              <Route path="/" element={<Navigate to="/sessions" replace />} />
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
              <Route
                path="/sessions"
                element={requireHttpAuth(<SessionsPage />)}
              />
              <Route path="/agents" element={requireHttpAuth(<AgentsPage />)} />
              <Route
                path="/agents/:instanceId"
                element={requireHttpAuth(<AgentManagementPage />)}
              />
              <Route
                path="/gateway"
                element={requireHttpAuth(<GatewayPage />)}
              />
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

              <Route path="*" element={<Navigate to="/sessions" />} />
            </Routes>
          </Suspense>
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
