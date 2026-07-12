import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router-dom";
import type { MCPInputParam, MCPServer } from "@mcp_link/shared";
import { Button, StatusDot, Switch } from "@mcp_link/ui";
import { ArrowLeft, Check, RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { useServerEditingStore, useServerStore } from "@/renderer/stores";
import { showServerError } from "@/renderer/components/common";
import ServerDetailsGeneralSettings from "./server-details/ServerDetailsGeneralSettings";
import ServerDetailsInputParams from "./server-details/ServerDetailsInputParams";
import ToolPermissions from "./server-details/ToolPermissions";
import SectionNav from "./SectionNav";

const ServerDetailPage: React.FC = () => {
  const { t } = useTranslation();
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const {
    servers,
    startServer,
    stopServer,
    refreshServers,
    updateServerConfig,
    updateServerToolPermissions,
  } = useServerStore();
  const server = servers.find((item) => item.id === id) ?? null;

  const {
    editedName,
    editedCommand,
    editedArgs,
    editedBearerToken,
    editedAutoStart,
    editedStartupTimeoutSec,
    editedCapabilityTimeoutSec,
    envPairs,
    editedToolPermissions,
    initializeFromServer,
    setEditedName,
    setEditedCommand,
    setEditedBearerToken,
    setEditedAutoStart,
    setEditedStartupTimeoutSec,
    setEditedCapabilityTimeoutSec,
    setEditedToolPermissions,
    updateArg,
    removeArg,
    addArg,
    updateEnvPair,
    removeEnvPair,
    addEnvPair,
    reset,
  } = useServerEditingStore();

  const [activeSection, setActiveSection] = useState("general");
  const initializedServerIdRef = useRef<string | null>(null);
  const [inputParamValues, setInputParamValues] = useState<
    Record<string, string>
  >({});
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (!server) {
      initializedServerIdRef.current = null;
      return;
    }
    if (initializedServerIdRef.current === server.id) return;
    initializedServerIdRef.current = server.id;
    initializeFromServer(server);
    setInputParamValues(initialInputValues(server));
    setEditedToolPermissions({ ...(server.toolPermissions ?? {}) });
  }, [initializeFromServer, server, setEditedToolPermissions]);

  const sections = useMemo(
    () => [
      { id: "general", label: t("serverDetails.generalSettings") },
      { id: "input", label: t("serverDetails.inputParameters") },
      { id: "tools", label: t("serverDetails.tools") },
    ],
    [t],
  );

  const handleToggleServer = async (checked: boolean) => {
    if (!server) return;
    try {
      if (checked) {
        await startServer(server.id);
        toast.success(t("serverList.serverStarted"));
      } else {
        await stopServer(server.id);
        toast.success(t("serverList.serverStopped"));
      }
    } catch (error) {
      showServerError(
        error instanceof Error ? error : new Error(String(error)),
        server.name,
      );
    }
  };

  const updateInputParam = (key: string, value: string) => {
    setInputParamValues((current) => ({ ...current, [key]: value }));
  };

  const prepareInputParamsForSave = useCallback(():
    | Record<string, MCPInputParam>
    | undefined => {
    if (!server?.inputParams) return server?.inputParams;
    const updated: Record<string, MCPInputParam> = { ...server.inputParams };
    for (const [key, value] of Object.entries(inputParamValues)) {
      if (updated[key]) {
        updated[key] = { ...updated[key], default: value };
      }
    }
    return updated;
  }, [inputParamValues, server]);

  const handleSave = async () => {
    if (!server) return;
    setIsSaving(true);
    try {
      const env: Record<string, string> = {};
      for (const pair of envPairs) {
        if (pair.key.trim()) {
          env[pair.key.trim()] = pair.value;
        }
      }

      const inputParams = prepareInputParamsForSave();
      if (inputParams) {
        Object.entries(inputParams).forEach(([key, param]) => {
          if (
            !env[key] &&
            param.default !== undefined &&
            param.default !== null &&
            String(param.default).trim() !== ""
          ) {
            env[key] = String(param.default);
          }
        });
      }

      const config: Partial<MCPServer> = {
        name: editedName || server.name,
        command: editedCommand,
        args: editedArgs,
        env,
        autoStart: editedAutoStart,
        startupTimeoutSec: editedStartupTimeoutSec,
        capabilityTimeoutSec: editedCapabilityTimeoutSec,
        inputParams,
      };

      if (server.serverType !== "local") {
        config.bearerToken = editedBearerToken;
      }

      await updateServerConfig(server.id, config);
      await updateServerToolPermissions(server.id, editedToolPermissions);
      await refreshServers();
      toast.success(t("serverDetails.updateSuccess"));
    } catch (error) {
      console.error("Failed to update server:", error);
      toast.error(t("serverDetails.updateFailed"));
    } finally {
      setIsSaving(false);
    }
  };

  if (!id) {
    return <Navigate to="/servers" replace />;
  }

  if (!server) {
    return (
      <PageLayout title={t("serverList.title")}>
        <div className="text-sm text-muted-foreground">
          {t("serverDetails.notFound")}{" "}
          <Link to="/servers" className="text-primary">
            {t("serverDetails.backToServers")}
          </Link>
        </div>
      </PageLayout>
    );
  }

  return (
    <PageLayout
      title={
        <span className="flex min-w-0 items-center gap-3">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => navigate("/servers")}
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <StatusDot tone={server.status} size="md" />
          <span className="truncate">{server.name}</span>
        </span>
      }
      toolbar={
        <>
          <Switch
            checked={server.status === "running"}
            disabled={
              server.status === "starting" || server.status === "stopping"
            }
            onCheckedChange={handleToggleServer}
          />
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              reset();
              navigate("/servers");
            }}
          >
            <X className="h-4 w-4" />
            {t("common.cancel")}
          </Button>
          <Button type="button" onClick={handleSave} disabled={isSaving}>
            {isSaving ? (
              <RefreshCw className="h-4 w-4 animate-spin" />
            ) : (
              <Check className="h-4 w-4" />
            )}
            {t("common.save")}
          </Button>
        </>
      }
      contentClassName="flex flex-col gap-6 lg:flex-row"
    >
      <SectionNav
        sections={sections}
        activeSection={activeSection}
        onSelect={setActiveSection}
      />
      <div className="min-w-0 flex-1 lg:max-w-3xl">
        {activeSection === "general" && (
          <ServerDetailsGeneralSettings
            server={server}
            editedName={editedName}
            setEditedName={setEditedName}
            editedCommand={editedCommand}
            setEditedCommand={setEditedCommand}
            editedArgs={editedArgs}
            updateArg={updateArg}
            removeArg={removeArg}
            addArg={addArg}
            editedBearerToken={editedBearerToken}
            setEditedBearerToken={setEditedBearerToken}
            editedAutoStart={editedAutoStart}
            setEditedAutoStart={setEditedAutoStart}
            editedStartupTimeoutSec={editedStartupTimeoutSec}
            setEditedStartupTimeoutSec={setEditedStartupTimeoutSec}
            editedCapabilityTimeoutSec={editedCapabilityTimeoutSec}
            setEditedCapabilityTimeoutSec={setEditedCapabilityTimeoutSec}
            envPairs={envPairs}
            updateEnvPair={updateEnvPair}
            removeEnvPair={removeEnvPair}
            addEnvPair={addEnvPair}
            inputParamValues={inputParamValues}
          />
        )}
        {activeSection === "input" && (
          <ServerDetailsInputParams
            server={server}
            inputParamValues={inputParamValues}
            updateInputParam={updateInputParam}
          />
        )}
        {activeSection === "tools" && (
          <ToolPermissions
            server={server}
            permissions={editedToolPermissions}
            onChange={setEditedToolPermissions}
          />
        )}
      </div>
    </PageLayout>
  );
};

function initialInputValues(server: MCPServer): Record<string, string> {
  const values: Record<string, string> = {};
  if (!server.inputParams) return values;
  for (const [key, param] of Object.entries(server.inputParams)) {
    values[key] = param.default !== undefined ? String(param.default) : "";
  }
  return values;
}

export default ServerDetailPage;
