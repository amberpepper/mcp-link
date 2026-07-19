import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import type {
  GatewayProvider,
  GatewayProviderDraft,
  GatewayRemoveTarget,
  GatewayRoute,
  GatewayRouteDraft,
  GatewaySettings,
} from "@mcp_link/shared";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@mcp_link/ui";
import { toast } from "sonner";

import PageLayout from "@/renderer/components/layout/PageLayout";
import { usePlatformAPI } from "@/renderer/platform-api";
import { getHttpApiBase } from "@/renderer/platform-api/http-platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import { usableMcpEndpoint } from "@/renderer/utils/mcp-endpoint";

import { GatewayConnectionCard } from "./GatewayConnectionCard";
import { GatewayCallLogs } from "./GatewayCallLogs";
import { GatewayProviderDialog } from "./GatewayProviderDialog";
import { GatewayProviderList } from "./GatewayProviderList";
import { GatewayRemoveDialog } from "./GatewayRemoveDialog";
import { GatewayRouteDialog } from "./GatewayRouteDialog";
import {
  emptyProviderDraft,
  emptyRouteDraft,
  errorMessage,
} from "./gateway-utils";

const GatewayPage: React.FC = () => {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [settings, setSettings] = useState<GatewaySettings | null>(null);
  const [listenHost, setListenHost] = useState("127.0.0.1");
  const [listenPort, setListenPort] = useState("3285");
  const [providers, setProviders] = useState<GatewayProvider[]>([]);
  const [routes, setRoutes] = useState<GatewayRoute[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [showAccessKey, setShowAccessKey] = useState(false);

  const [providerOpen, setProviderOpen] = useState(false);
  const [editingProvider, setEditingProvider] =
    useState<GatewayProvider | null>(null);
  const [providerDraft, setProviderDraft] =
    useState<GatewayProviderDraft>(emptyProviderDraft);
  const [showProviderKey, setShowProviderKey] = useState(false);
  const [fetchingModels, setFetchingModels] = useState(false);
  const modelFetchRequestRef = useRef(0);

  const [routeOpen, setRouteOpen] = useState(false);
  const [editingRoute, setEditingRoute] = useState<GatewayRoute | null>(null);
  const [routeDraft, setRouteDraft] =
    useState<GatewayRouteDraft>(emptyRouteDraft);
  const [removeTarget, setRemoveTarget] = useState<GatewayRemoveTarget | null>(
    null,
  );
  const isDesktopRuntime = isTauriRuntime();

  const loadGateway = useCallback(async () => {
    try {
      const [nextSettings, nextProviders, nextRoutes] = await Promise.all([
        platformAPI.gateway.getSettings(),
        platformAPI.gateway.listProviders(),
        platformAPI.gateway.listRoutes(),
      ]);
      setSettings(nextSettings);
      setListenHost(nextSettings.listenHost);
      setListenPort(String(nextSettings.listenPort));
      setProviders(nextProviders);
      setRoutes(nextRoutes);
    } catch (error) {
      toast.error(errorMessage(error, t("gateway.loadFailed")));
    } finally {
      setLoading(false);
    }
  }, [platformAPI, t]);

  useEffect(() => {
    void loadGateway();
  }, [loadGateway]);

  const providerById = useMemo(
    () => new Map(providers.map((provider) => [provider.id, provider])),
    [providers],
  );
  const routeProviderModels = useMemo(() => {
    const savedModels = providerById.get(routeDraft.providerId)?.models ?? [];
    const models =
      editingProvider?.id === routeDraft.providerId &&
      providerDraft.models.length > 0
        ? providerDraft.models
        : savedModels;
    const current = routeDraft.upstreamModel.trim();
    return current && !models.includes(current) ? [current, ...models] : models;
  }, [editingProvider?.id, providerById, providerDraft.models, routeDraft]);
  const endpoint = useMemo(() => {
    if (!isDesktopRuntime) return getHttpApiBase();
    if (settings?.endpoint) return usableMcpEndpoint(settings.endpoint);
    if (settings?.listenerError) return "";
    const host = listenHost === "0.0.0.0" ? "127.0.0.1" : listenHost;
    return `http://${host || "127.0.0.1"}:${listenPort || "3285"}`;
  }, [
    isDesktopRuntime,
    listenHost,
    listenPort,
    settings?.endpoint,
    settings?.listenerError,
  ]);

  const copyText = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(t("gateway.copied"));
    } catch {
      toast.error(t("gateway.copyFailed"));
    }
  };

  const saveSettings = async () => {
    const port = Number(listenPort);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      toast.error(t("gateway.invalidPort"));
      return;
    }
    setSaving(true);
    try {
      const next = await platformAPI.gateway.saveSettings({
        listenHost: listenHost.trim(),
        listenPort: port,
      });
      setSettings(next);
      toast.success(t("gateway.settingsSaved"));
    } catch (error) {
      const current = await platformAPI.gateway.getSettings().catch(() => null);
      if (current) setSettings(current);
      toast.error(errorMessage(error, t("gateway.saveFailed")));
    } finally {
      setSaving(false);
    }
  };

  const regenerateAccessKey = async () => {
    setSaving(true);
    try {
      const accessKey = await platformAPI.gateway.regenerateAccessKey();
      setSettings((current) => (current ? { ...current, accessKey } : current));
      setShowAccessKey(true);
      toast.success(t("gateway.keyRegenerated"));
    } catch (error) {
      toast.error(errorMessage(error, t("gateway.saveFailed")));
    } finally {
      setSaving(false);
    }
  };

  const openCreateProvider = () => {
    modelFetchRequestRef.current += 1;
    setFetchingModels(false);
    setEditingProvider(null);
    setProviderDraft(emptyProviderDraft());
    setShowProviderKey(false);
    setProviderOpen(true);
  };

  const openEditProvider = (provider: GatewayProvider) => {
    modelFetchRequestRef.current += 1;
    setFetchingModels(false);
    setEditingProvider(provider);
    setProviderDraft({
      name: provider.name,
      protocol: provider.protocol,
      baseUrl: provider.baseUrl,
      apiKey: provider.apiKey,
      models: provider.models ?? [],
      enabled: provider.enabled,
    });
    setShowProviderKey(false);
    setProviderOpen(true);
  };

  const fetchProviderModels = async () => {
    if (!providerDraft.baseUrl.trim()) {
      toast.error(t("gateway.baseUrlRequired"));
      return;
    }
    const requestId = modelFetchRequestRef.current + 1;
    modelFetchRequestRef.current = requestId;
    setFetchingModels(true);
    try {
      const models = await platformAPI.gateway.fetchProviderModels({
        protocol: providerDraft.protocol,
        baseUrl: providerDraft.baseUrl.trim(),
        apiKey: providerDraft.apiKey.trim(),
      });
      if (modelFetchRequestRef.current !== requestId) return;
      setProviderDraft((draft) => ({ ...draft, models }));
      toast.success(t("gateway.modelsFetched", { count: models.length }));
    } catch (error) {
      if (modelFetchRequestRef.current !== requestId) return;
      toast.error(errorMessage(error, t("gateway.modelsFetchFailed")));
    } finally {
      if (modelFetchRequestRef.current === requestId) {
        setFetchingModels(false);
      }
    }
  };

  const handleProviderOpenChange = (open: boolean) => {
    if (!open) {
      modelFetchRequestRef.current += 1;
      setFetchingModels(false);
    }
    setProviderOpen(open);
  };

  const saveProvider = async () => {
    if (!providerDraft.name.trim() || !providerDraft.baseUrl.trim()) {
      toast.error(t("gateway.requiredFields"));
      return;
    }
    setSaving(true);
    try {
      const input = {
        ...providerDraft,
        name: providerDraft.name.trim(),
        baseUrl: providerDraft.baseUrl.trim(),
        apiKey: providerDraft.apiKey.trim(),
      };
      if (editingProvider) {
        await platformAPI.gateway.updateProvider(editingProvider.id, input);
      } else {
        await platformAPI.gateway.createProvider(input);
      }
      setProviderOpen(false);
      await loadGateway();
      toast.success(t("gateway.providerSaved"));
    } catch (error) {
      toast.error(errorMessage(error, t("gateway.saveFailed")));
    } finally {
      setSaving(false);
    }
  };

  const setActiveProvider = async (provider: GatewayProvider) => {
    if (settings?.activeProviderId === provider.id) return;
    setSaving(true);
    try {
      if (!provider.enabled) {
        await platformAPI.gateway.updateProvider(provider.id, {
          enabled: true,
        });
      }
      await platformAPI.gateway.setActiveProvider(provider.id);
      await Promise.all(
        providers
          .filter((item) => item.id !== provider.id && item.enabled)
          .map((item) =>
            platformAPI.gateway.updateProvider(item.id, { enabled: false }),
          ),
      );
      setProviders((current) =>
        current.map((item) => ({ ...item, enabled: item.id === provider.id })),
      );
      setSettings((current) =>
        current ? { ...current, activeProviderId: provider.id } : current,
      );
      setRouteOpen(false);
      toast.success(
        t("gateway.activeProviderChanged", { name: provider.name }),
      );
    } catch (error) {
      toast.error(errorMessage(error, t("gateway.activeProviderChangeFailed")));
    } finally {
      setSaving(false);
    }
  };

  const openCreateRoute = (providerId = editingProvider?.id ?? "") => {
    const models =
      editingProvider?.id === providerId && providerDraft.models.length > 0
        ? providerDraft.models
        : (providerById.get(providerId)?.models ?? []);
    setEditingRoute(null);
    setRouteDraft({
      ...emptyRouteDraft(providerId),
      upstreamModel: models[0] ?? "",
    });
    setRouteOpen(true);
    if (models.length === 0 && editingProvider?.id === providerId) {
      void fetchProviderModels();
    }
  };

  const openEditRoute = (route: GatewayRoute) => {
    setEditingRoute(route);
    setRouteDraft({
      alias: route.alias,
      providerId: route.providerId,
      upstreamModel: route.upstreamModel,
    });
    setRouteOpen(true);
    const models =
      editingProvider?.id === route.providerId &&
      providerDraft.models.length > 0
        ? providerDraft.models
        : (providerById.get(route.providerId)?.models ?? []);
    if (models.length === 0 && editingProvider?.id === route.providerId) {
      void fetchProviderModels();
    }
  };

  useEffect(() => {
    if (
      routeOpen &&
      !routeDraft.upstreamModel &&
      routeProviderModels.length > 0
    ) {
      setRouteDraft((draft) => ({
        ...draft,
        upstreamModel: routeProviderModels[0],
      }));
    }
  }, [routeOpen, routeDraft.upstreamModel, routeProviderModels]);

  const saveRoute = async () => {
    if (
      !routeDraft.alias.trim() ||
      !routeDraft.providerId ||
      !routeDraft.upstreamModel.trim()
    ) {
      toast.error(t("gateway.requiredFields"));
      return;
    }
    setSaving(true);
    try {
      const input = {
        alias: routeDraft.alias.trim(),
        providerId: routeDraft.providerId,
        upstreamModel: routeDraft.upstreamModel.trim(),
      };
      if (editingRoute) {
        await platformAPI.gateway.updateRoute(editingRoute.id, input);
      } else {
        await platformAPI.gateway.createRoute(input);
      }
      setRouteOpen(false);
      await loadGateway();
      toast.success(t("gateway.routeSaved"));
    } catch (error) {
      toast.error(errorMessage(error, t("gateway.saveFailed")));
    } finally {
      setSaving(false);
    }
  };

  const removeItem = async () => {
    if (!removeTarget) return;
    setSaving(true);
    try {
      if (removeTarget.type === "provider") {
        await platformAPI.gateway.removeProvider(removeTarget.item.id);
      } else {
        await platformAPI.gateway.removeRoute(removeTarget.item.id);
      }
      setRemoveTarget(null);
      await loadGateway();
      toast.success(t("gateway.removed"));
    } catch (error) {
      toast.error(errorMessage(error, t("gateway.removeFailed")));
    } finally {
      setSaving(false);
    }
  };

  const providerProtocolLocked = Boolean(
    editingProvider &&
    routes.some((route) => route.providerId === editingProvider.id),
  );

  return (
    <PageLayout
      title={t("gateway.title")}
      contentClassName="flex flex-col gap-6"
    >
      <GatewayConnectionCard
        endpoint={endpoint}
        listenerError={settings?.listenerError ?? null}
        settings={settings}
        isDesktopRuntime={isDesktopRuntime}
        listenHost={listenHost}
        listenPort={listenPort}
        saving={saving}
        showAccessKey={showAccessKey}
        onListenHostChange={setListenHost}
        onListenPortChange={setListenPort}
        onShowAccessKeyChange={setShowAccessKey}
        onCopy={copyText}
        onRegenerateKey={regenerateAccessKey}
        onSaveSettings={saveSettings}
      />
      <Tabs defaultValue="providers">
        <TabsList>
          <TabsTrigger value="providers">{t("gateway.providers")}</TabsTrigger>
          <TabsTrigger value="logs">{t("gateway.logs.title")}</TabsTrigger>
        </TabsList>
        <TabsContent value="providers" className="mt-4">
          <GatewayProviderList
            loading={loading}
            saving={saving}
            providers={providers}
            routes={routes}
            settings={settings}
            onCreate={openCreateProvider}
            onEdit={openEditProvider}
            onActivate={setActiveProvider}
            onRemove={setRemoveTarget}
          />
        </TabsContent>
        <TabsContent value="logs" className="mt-4">
          <GatewayCallLogs providers={providers} />
        </TabsContent>
      </Tabs>
      <GatewayProviderDialog
        open={providerOpen}
        editingProvider={editingProvider}
        draft={providerDraft}
        routes={routes}
        saving={saving}
        fetchingModels={fetchingModels}
        protocolLocked={providerProtocolLocked}
        showApiKey={showProviderKey}
        onOpenChange={handleProviderOpenChange}
        onDraftChange={setProviderDraft}
        onShowApiKeyChange={setShowProviderKey}
        onFetchModels={fetchProviderModels}
        onSave={saveProvider}
        onCreateRoute={openCreateRoute}
        onEditRoute={openEditRoute}
        onRemove={setRemoveTarget}
      />
      <GatewayRouteDialog
        open={routeOpen}
        editingRoute={editingRoute}
        draft={routeDraft}
        provider={providerById.get(routeDraft.providerId)}
        models={routeProviderModels}
        saving={saving}
        fetchingModels={fetchingModels}
        onOpenChange={setRouteOpen}
        onDraftChange={setRouteDraft}
        onFetchModels={fetchProviderModels}
        onSave={saveRoute}
      />
      <GatewayRemoveDialog
        target={removeTarget}
        saving={saving}
        onTargetChange={setRemoveTarget}
        onConfirm={removeItem}
      />
    </PageLayout>
  );
};

export default GatewayPage;
