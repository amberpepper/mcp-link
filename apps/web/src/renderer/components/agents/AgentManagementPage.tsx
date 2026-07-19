import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
  Switch,
  Textarea,
} from "@mcp_link/ui";
import {
  IconAdjustments,
  IconAlertTriangle,
  IconArrowLeft,
  IconBraces,
  IconBrain,
  IconDatabase,
  IconEdit,
  IconKey,
  IconFileText,
  IconPlus,
  IconRefresh,
  IconServer,
  IconSettings,
  IconShield,
  IconSparkles,
  IconTrash,
} from "@tabler/icons-react";
import type {
  AgentConfigDocument,
  AgentConfigFileSummary,
  AgentInstanceEntry,
  AgentManagementMutation,
  AgentManagementSection,
  AgentManagementSectionDescriptor,
  AgentManagementSectionId,
  AgentManagementSectionRenderer,
  ManagedApiProvider,
  ManagedApiProviderSettings,
  ManagedEnvironmentSettings,
  ManagedFormField,
  ManagedFormSettings,
  ManagedLocalizedText,
  ManagedMcpServer,
  ManagedMcpSettings,
  ManagedModelSettings,
  ManagedPermissionRule,
  ManagedPermissionSettings,
} from "@mcp_link/shared";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";

import { usePlatformAPI } from "@/renderer/platform-api";
import { useThemeStore } from "@/renderer/stores";
import { cn } from "@/renderer/utils/tailwind-utils";
import { usableMcpEndpoint } from "@/renderer/utils/mcp-endpoint";
import AgentAvatar from "./AgentAvatar";
import SkillsManager from "@/renderer/components/skills/SkillsManager";
import { getHttpApiBase } from "@/renderer/platform-api/http-platform-api";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";

const sectionMeta: Record<
  AgentManagementSectionRenderer,
  { icon: React.ComponentType<{ className?: string }>; label: string | null }
> = {
  overview: { icon: IconAdjustments, label: "overview" },
  form: { icon: IconSettings, label: null },
  mcp: { icon: IconServer, label: "mcp" },
  skills: { icon: IconSparkles, label: "skills" },
  prompts: { icon: IconFileText, label: "prompts" },
  providers: { icon: IconKey, label: "providers" },
  models: { icon: IconBrain, label: "models" },
  permissions: { icon: IconShield, label: "permissions" },
  environment: { icon: IconDatabase, label: "environment" },
  "raw-config": { icon: IconBraces, label: "rawConfig" },
};

const fallbackSectionMeta = { icon: IconBraces, label: null };

function getSectionMeta(section: AgentManagementSectionDescriptor) {
  return (
    sectionMeta[section.renderer as AgentManagementSectionRenderer] ??
    fallbackSectionMeta
  );
}

const AgentManagementPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { instanceId = "" } = useParams<{ instanceId: string }>();
  const navigate = useNavigate();
  const platformAPI = usePlatformAPI();
  const [entry, setEntry] = useState<AgentInstanceEntry | null>(null);
  const [sections, setSections] = useState<AgentManagementSectionDescriptor[]>(
    [],
  );
  const [selected, setSelected] =
    useState<AgentManagementSectionId>("overview");
  const [section, setSection] = useState<AgentManagementSection | null>(null);
  const [loading, setLoading] = useState(true);
  const [sectionLoading, setSectionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectedDescriptor = useMemo(
    () => sections.find((item) => item.id === selected),
    [sections, selected],
  );
  const selectedRenderer = selectedDescriptor?.renderer ?? selected;

  const loadShell = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [plugins, descriptor] = await Promise.all([
        platformAPI.agents.list(),
        platformAPI.agents.management.describe(instanceId),
      ]);
      const found = plugins
        .flatMap((plugin) =>
          plugin.instances.map((instance) => ({ plugin, instance })),
        )
        .find((candidate) => candidate.instance.id === instanceId);
      if (!found) throw new Error(t("agents.management.instanceNotFound"));
      setEntry(found);
      setSections(descriptor.sections);
      setSelected((current) =>
        descriptor.sections.some((item) => item.id === current)
          ? current
          : (descriptor.sections[0]?.id ?? "overview"),
      );
    } catch (cause) {
      setError(errorMessage(cause, t("agents.management.loadFailed")));
    } finally {
      setLoading(false);
    }
  }, [instanceId, platformAPI, t]);

  const loadSection = useCallback(async () => {
    if (!instanceId || selectedDescriptor?.source === "host") {
      setSection(null);
      return;
    }
    setSectionLoading(true);
    try {
      setSection(
        await platformAPI.agents.management.getSection(instanceId, selected),
      );
    } catch (cause) {
      toast.error(
        errorMessage(cause, t("agents.management.sectionLoadFailed")),
      );
      setSection(null);
    } finally {
      setSectionLoading(false);
    }
  }, [instanceId, platformAPI, selected, selectedDescriptor?.source, t]);

  useEffect(() => {
    void loadShell();
  }, [loadShell]);

  useEffect(() => {
    if (!loading && entry) void loadSection();
  }, [entry, loadSection, loading]);

  if (loading) return <ManagementSkeleton />;
  if (error || !entry) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <Alert variant="destructive" className="max-w-xl">
          <IconAlertTriangle className="h-4 w-4" />
          <AlertTitle>{t("agents.management.loadFailed")}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      </div>
    );
  }

  const waitsForManagedSection =
    selectedDescriptor?.source !== "host" && section?.id !== selected;
  const selectedMeta = selectedDescriptor
    ? getSectionMeta(selectedDescriptor)
    : fallbackSectionMeta;
  const selectedTitle = selectedMeta.label
    ? t(`agents.management.sections.${selectedMeta.label}`)
    : (localizedText(selectedDescriptor?.label, i18n.resolvedLanguage) ??
      selected);
  const selectedDescription = selectedMeta.label
    ? t(`agents.management.descriptions.${selectedMeta.label}`)
    : localizedText(selectedDescriptor?.description, i18n.resolvedLanguage);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <header className="flex shrink-0 items-center gap-3 border-b px-5 py-3">
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("common.back")}
          onClick={() => navigate("/agents")}
        >
          <IconArrowLeft className="h-4 w-4" />
        </Button>
        <AgentAvatar plugin={entry.plugin} size="lg" />
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-lg font-semibold">
            {entry.instance.label}
          </h1>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={sectionLoading}
          onClick={() => void loadSection()}
        >
          <IconRefresh
            className={cn("h-4 w-4", sectionLoading && "animate-spin")}
          />
          {t("common.refresh")}
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        <nav className="w-52 shrink-0 overflow-y-auto border-r p-3">
          <div className="space-y-1">
            {sections.map((item) => {
              const meta = getSectionMeta(item);
              const Icon = meta.icon;
              return (
                <button
                  key={item.id}
                  type="button"
                  className={cn(
                    "flex h-9 w-full items-center gap-2 rounded-md px-3 text-left text-sm transition-colors",
                    selected === item.id
                      ? "bg-accent font-medium text-accent-foreground"
                      : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                  )}
                  onClick={() => setSelected(item.id)}
                >
                  <Icon className="h-4 w-4 shrink-0" />
                  <span className="truncate">
                    {meta.label
                      ? t(`agents.management.sections.${meta.label}`)
                      : (localizedText(item.label, i18n.resolvedLanguage) ??
                        item.id)}
                  </span>
                </button>
              );
            })}
          </div>
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto p-6">
          <div className="mx-auto max-w-5xl">
            <div className="mb-5 flex items-center justify-between gap-4">
              <div>
                <h2 className="text-xl font-semibold">{selectedTitle}</h2>
                {selectedDescription && (
                  <p className="mt-1 text-sm text-muted-foreground">
                    {selectedDescription}
                  </p>
                )}
              </div>
              {selectedDescriptor?.readOnly && (
                <Badge variant="outline">
                  {t("agents.management.readOnly")}
                </Badge>
              )}
            </div>
            {sectionLoading || waitsForManagedSection ? (
              <SectionSkeleton />
            ) : (
              <SectionContent
                entry={entry}
                renderer={selectedRenderer}
                section={section}
                readOnly={selectedDescriptor?.readOnly ?? true}
                reload={loadSection}
              />
            )}
          </div>
        </main>
      </div>
    </div>
  );
};

function SectionContent({
  entry,
  renderer,
  section,
  readOnly,
  reload,
}: {
  entry: AgentInstanceEntry;
  renderer: string;
  section: AgentManagementSection | null;
  readOnly: boolean;
  reload: () => Promise<void>;
}) {
  if (renderer === "skills") {
    return <SkillsManager embedded targetAgentId={entry.plugin.id} />;
  }
  if (renderer === "prompts") return <PromptSection entry={entry} />;
  if (renderer === "raw-config") return <RawConfigSection entry={entry} />;
  if (!section) return <EmptySection />;
  if (renderer === "overview") return <OverviewSection data={section.data} />;
  if (renderer === "form") {
    return (
      <DynamicFormSection
        instanceId={entry.instance.id}
        section={section as AgentManagementSection<ManagedFormSettings>}
        readOnly={readOnly}
        reload={reload}
      />
    );
  }
  if (renderer === "mcp") {
    return (
      <McpSection
        instanceId={entry.instance.id}
        section={section as AgentManagementSection<ManagedMcpSettings>}
        readOnly={readOnly}
        reload={reload}
      />
    );
  }
  if (renderer === "providers") {
    return (
      <ProvidersSection
        agentId={entry.plugin.id}
        instanceId={entry.instance.id}
        section={section as AgentManagementSection<ManagedApiProviderSettings>}
        readOnly={readOnly}
        reload={reload}
      />
    );
  }
  if (renderer === "models") {
    return (
      <ModelsSection
        instanceId={entry.instance.id}
        section={section as AgentManagementSection<ManagedModelSettings>}
        readOnly={readOnly}
        reload={reload}
      />
    );
  }
  if (renderer === "permissions") {
    return (
      <PermissionsSection
        instanceId={entry.instance.id}
        section={section as AgentManagementSection<ManagedPermissionSettings>}
        readOnly={readOnly}
        reload={reload}
      />
    );
  }
  if (renderer === "environment") {
    return (
      <EnvironmentSection data={section.data as ManagedEnvironmentSettings} />
    );
  }
  return <EmptySection />;
}

function OverviewSection({ data }: { data: unknown }) {
  const { t } = useTranslation();
  const value = data as Record<string, unknown>;
  const metrics = [
    ["defaultModel", value.defaultModel],
    ["defaultProvider", value.defaultProvider],
    ["mcpServerCount", value.mcpServerCount],
    ["providerCount", value.providerCount],
  ];
  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {metrics.map(([key, metric]) => (
          <Card key={String(key)}>
            <CardContent className="p-4">
              <p className="text-xs text-muted-foreground">
                {t(`agents.management.fields.${key}`)}
              </p>
              <p className="mt-2 truncate text-lg font-semibold">
                {metric === null || metric === undefined || metric === ""
                  ? "—"
                  : String(metric)}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            {t("agents.management.paths")}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {(["configRoot", "sessionRoot", "skillRoot"] as const).map((key) => (
            <KeyValue
              key={key}
              label={t(`agents.management.fields.${key}`)}
              value={value[key]}
              mono
            />
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function McpSection(props: EditableSectionProps<ManagedMcpSettings>) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState<Partial<ManagedMcpServer> | null>(
    null,
  );
  const servers = props.section.data.servers ?? [];
  const canDisable = props.section.data.canDisable !== false;
  return (
    <div className="space-y-3">
      {!props.readOnly && (
        <div className="flex justify-end">
          <Button
            size="sm"
            className="h-8 shadow-none"
            onClick={() =>
              setEditing({ transport: "stdio", enabled: true, args: [] })
            }
          >
            <IconPlus className="h-4 w-4" />
            {t("agents.management.addMcp")}
          </Button>
        </div>
      )}
      {servers.length === 0 ? (
        <EmptySection />
      ) : (
        servers.map((server) => (
          <Card key={server.id}>
            <CardContent className="flex items-center gap-4 p-4">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium">{server.name}</span>
                  <Badge variant="outline">{server.transport}</Badge>
                  {!server.enabled && (
                    <Badge variant="secondary">{t("common.disabled")}</Badge>
                  )}
                </div>
                <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                  {server.transport === "stdio"
                    ? [server.command, ...(server.args ?? [])]
                        .filter(Boolean)
                        .join(" ")
                    : server.url}
                </p>
              </div>
              {!props.readOnly && (
                <div className="flex shrink-0 items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 px-3 text-xs shadow-none"
                    onClick={() => setEditing(server)}
                  >
                    <IconEdit className="h-4 w-4" />
                    {t("common.edit")}
                  </Button>
                  <MutationButton
                    {...props}
                    sectionId={props.section.id}
                    action="remove"
                    entityId={server.id}
                    variant="outline"
                    size="sm"
                    className="h-8 border-destructive/30 px-3 text-xs text-destructive shadow-none hover:bg-destructive/10 hover:text-destructive"
                  >
                    <IconTrash className="h-4 w-4" />
                    {t("common.delete")}
                  </MutationButton>
                </div>
              )}
            </CardContent>
          </Card>
        ))
      )}
      <McpDialog
        open={editing !== null}
        value={editing ?? {}}
        onClose={() => setEditing(null)}
        mutationProps={props}
        canDisable={canDisable}
      />
    </div>
  );
}

function McpDialog({
  open,
  value,
  onClose,
  mutationProps,
  canDisable,
}: {
  open: boolean;
  value: Partial<ManagedMcpServer>;
  onClose: () => void;
  mutationProps: EditableSectionProps<ManagedMcpSettings>;
  canDisable: boolean;
}) {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [form, setForm] = useState(value);
  const [presetBusy, setPresetBusy] = useState(false);
  const generatedKeyPrefix = useRef<string | null>(null);
  const generatedKeyCommitted = useRef(false);
  useEffect(() => {
    setForm(value);
    generatedKeyPrefix.current = null;
    generatedKeyCommitted.current = false;
  }, [value]);
  const close = async () => {
    if (!generatedKeyCommitted.current) {
      await cleanupGeneratedKey(platformAPI, generatedKeyPrefix.current);
    }
    generatedKeyPrefix.current = null;
    onClose();
  };
  const useMcpLink = async () => {
    setPresetBusy(true);
    try {
      await cleanupGeneratedKey(platformAPI, generatedKeyPrefix.current);
      const [servers, endpoint] = await Promise.all([
        platformAPI.servers.list(),
        platformAPI.settings.getMcpEndpoint(),
      ]);
      const token = await platformAPI.accessKeys.generate({
        name: `${mutationProps.instanceId} MCP`,
        serverAccess: Object.fromEntries(
          servers.map((server) => [server.id, true]),
        ),
      });
      generatedKeyPrefix.current = token.slice(0, 12);
      setForm({
        id: "mcp-link",
        name: "MCP Link",
        transport: "http",
        url: usableMcpEndpoint(endpoint),
        headers: { Authorization: `Bearer ${token}` },
        enabled: true,
      });
    } catch (cause) {
      toast.error(errorMessage(cause, t("agents.management.saveFailed")));
    } finally {
      setPresetBusy(false);
    }
  };
  const payload = {
    id: form.id?.trim(),
    name: form.name?.trim() || form.id?.trim(),
    transport: form.transport ?? "stdio",
    command: form.command?.trim(),
    args: form.args ?? [],
    url: form.url?.trim(),
    enabled: form.enabled ?? true,
    env: form.env ?? {},
    headers: form.headers ?? {},
  };
  return (
    <Dialog open={open} onOpenChange={(next) => !next && void close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {form.id
              ? t("agents.management.editMcp")
              : t("agents.management.addMcp")}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          {!value.id && (
            <Button
              type="button"
              variant="outline"
              className="w-full justify-start"
              disabled={presetBusy}
              onClick={() => void useMcpLink()}
            >
              <IconServer className="h-4 w-4" />
              {t("agents.management.integration.useMcpLinkPreset")}
            </Button>
          )}
          <Field label="ID">
            <Input
              disabled={Boolean(value.id)}
              value={form.id ?? ""}
              onChange={(e) => setForm({ ...form, id: e.target.value })}
            />
          </Field>
          <Field label={t("agents.management.fields.name")}>
            <Input
              value={form.name ?? ""}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
          </Field>
          <Field label={t("agents.management.fields.transport")}>
            <Select
              value={form.transport ?? "stdio"}
              onValueChange={(transport: "stdio" | "http" | "sse") =>
                setForm({ ...form, transport })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="stdio">stdio</SelectItem>
                <SelectItem value="http">HTTP</SelectItem>
                <SelectItem value="sse">SSE</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          {form.transport === "stdio" || !form.transport ? (
            <>
              <Field label={t("agents.management.fields.command")}>
                <Input
                  value={form.command ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, command: e.target.value })
                  }
                />
              </Field>
              <Field label={t("agents.management.fields.arguments")}>
                <Textarea
                  className="min-h-24 resize-y font-mono text-xs"
                  value={(form.args ?? []).join("\n")}
                  onChange={(e) =>
                    setForm({
                      ...form,
                      args: e.target.value
                        .split(/\r?\n/)
                        .map((argument) => argument.trim())
                        .filter(Boolean),
                    })
                  }
                />
              </Field>
            </>
          ) : (
            <Field label="URL">
              <Input
                value={form.url ?? ""}
                onChange={(e) => setForm({ ...form, url: e.target.value })}
              />
            </Field>
          )}
          {canDisable && (
            <div className="flex items-center justify-between">
              <Label>{t("common.enabled")}</Label>
              <Switch
                checked={form.enabled ?? true}
                onCheckedChange={(enabled) => setForm({ ...form, enabled })}
              />
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => void close()}>
            {t("common.cancel")}
          </Button>
          <MutationButton
            {...mutationProps}
            sectionId={mutationProps.section.id}
            action="upsert"
            entityId={value.id}
            payload={payload}
            disabled={!payload.id}
            onApplied={() => {
              generatedKeyCommitted.current = true;
              onClose();
            }}
          >
            {t("common.save")}
          </MutationButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ProvidersSection({
  agentId,
  ...props
}: EditableSectionProps<ManagedApiProviderSettings> & { agentId: string }) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState<Partial<ManagedApiProvider> | null>(
    null,
  );
  return (
    <div className="space-y-3">
      {!props.readOnly && (
        <div className="flex justify-end">
          <Button
            size="sm"
            className="h-8 shadow-none"
            onClick={() =>
              setEditing({ protocol: "openai", enabled: true, models: [] })
            }
          >
            <IconPlus className="h-4 w-4" />
            {t("agents.management.addProvider")}
          </Button>
        </div>
      )}
      {(props.section.data.providers ?? []).length === 0 ? (
        <EmptySection />
      ) : (
        props.section.data.providers.map((provider) => (
          <Card key={provider.id}>
            <CardContent className="flex items-center gap-4 p-4">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium">{provider.name}</span>
                  <Badge variant="outline">{provider.protocol}</Badge>
                </div>
                <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                  {provider.baseUrl || "—"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {(provider.models ?? []).join(", ")}
                </p>
              </div>
              {!props.readOnly && (
                <div className="flex shrink-0 items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 px-3 text-xs shadow-none"
                    onClick={() => setEditing(provider)}
                  >
                    <IconEdit className="h-4 w-4" />
                    {t("common.edit")}
                  </Button>
                  <MutationButton
                    {...props}
                    sectionId={props.section.id}
                    action="remove"
                    entityId={provider.id}
                    variant="outline"
                    size="sm"
                    className="h-8 border-destructive/30 px-3 text-xs text-destructive shadow-none hover:bg-destructive/10 hover:text-destructive"
                  >
                    <IconTrash className="h-4 w-4" />
                    {t("common.delete")}
                  </MutationButton>
                </div>
              )}
            </CardContent>
          </Card>
        ))
      )}
      <ProviderDialog
        agentId={agentId}
        open={editing !== null}
        value={editing ?? {}}
        secretInput={props.section.data.secretInput}
        canEditModels={props.section.data.canEditModels !== false}
        onClose={() => setEditing(null)}
        mutationProps={props}
      />
    </div>
  );
}

function ProviderDialog({
  agentId,
  open,
  value,
  secretInput,
  canEditModels,
  onClose,
  mutationProps,
}: {
  agentId: string;
  open: boolean;
  value: Partial<ManagedApiProvider>;
  secretInput?: ManagedApiProviderSettings["secretInput"];
  canEditModels: boolean;
  onClose: () => void;
  mutationProps: EditableSectionProps<ManagedApiProviderSettings>;
}) {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [form, setForm] = useState(value);
  const [presetBusy, setPresetBusy] = useState(false);
  const [presetApiKey, setPresetApiKey] = useState("");
  const [apiKeyValue, setApiKeyValue] = useState("");
  const [apiKeyEnvironmentVariable, setApiKeyEnvironmentVariable] = useState(
    value.apiKey?.environmentVariable ??
      secretInput?.defaultEnvironmentVariable ??
      "",
  );
  const secretMode = secretInput?.mode ?? "value";
  useEffect(() => {
    setForm(value);
    setApiKeyValue("");
    setPresetApiKey("");
    setApiKeyEnvironmentVariable(
      value.apiKey?.environmentVariable ??
        secretInput?.defaultEnvironmentVariable ??
        "",
    );
  }, [secretInput?.defaultEnvironmentVariable, value]);
  const useGateway = async () => {
    setPresetBusy(true);
    try {
      const settings = await platformAPI.gateway.getSettings();
      const accessKey =
        settings.accessKey || (await platformAPI.gateway.regenerateAccessKey());
      const endpoint = resolveGatewayEndpoint(settings).replace(/\/+$/, "");
      const anthropic = agentId === "claude-code";
      setForm({
        id: "mcp-link",
        name: "MCP Link Gateway",
        protocol: anthropic ? "anthropic" : "openai",
        baseUrl: `${endpoint}/${anthropic ? "anthropic" : "openai/v1"}`,
        models: [],
        enabled: true,
      });
      setPresetApiKey(accessKey);
      setApiKeyValue(accessKey);
    } catch (cause) {
      toast.error(errorMessage(cause, t("agents.management.saveFailed")));
    } finally {
      setPresetBusy(false);
    }
  };
  const payload = {
    id: form.id?.trim(),
    name: form.name?.trim() || form.id?.trim(),
    protocol: form.protocol ?? "openai",
    baseUrl: form.baseUrl?.trim(),
    apiKeyValue:
      presetApiKey || (secretMode === "value" ? apiKeyValue : undefined),
    apiKeyEnvironmentVariable:
      secretMode === "environment-variable"
        ? apiKeyEnvironmentVariable.trim()
        : undefined,
    models: form.models ?? [],
    enabled: form.enabled ?? true,
  };
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {value.id
              ? t("agents.management.editProvider")
              : t("agents.management.addProvider")}
          </DialogTitle>
          <DialogDescription>
            {t("agents.management.secretHint")}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          {!value.id && (
            <Button
              type="button"
              variant="outline"
              className="w-full justify-start"
              disabled={presetBusy}
              onClick={() => void useGateway()}
            >
              <IconBrain className="h-4 w-4" />
              {t("agents.management.integration.useGatewayPreset")}
            </Button>
          )}
          <Field label="ID">
            <Input
              disabled={Boolean(value.id)}
              value={form.id ?? ""}
              onChange={(e) => setForm({ ...form, id: e.target.value })}
            />
          </Field>
          <Field label={t("agents.management.fields.name")}>
            <Input
              value={form.name ?? ""}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
          </Field>
          <Field label={t("agents.management.fields.protocol")}>
            <Select
              value={form.protocol ?? "openai"}
              onValueChange={(protocol: ManagedApiProvider["protocol"]) =>
                setForm({ ...form, protocol })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="openai">OpenAI compatible</SelectItem>
                <SelectItem value="anthropic">Anthropic</SelectItem>
                <SelectItem value="gemini">Gemini</SelectItem>
                <SelectItem value="custom">Custom</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          <Field label="Base URL">
            <Input
              value={form.baseUrl ?? ""}
              onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
            />
          </Field>
          {secretMode === "environment-variable" ? (
            <Field
              label={t("agents.management.fields.apiKeyEnvironmentVariable")}
            >
              <Input
                className="font-mono"
                value={apiKeyEnvironmentVariable}
                placeholder="OPENAI_API_KEY"
                onChange={(e) => setApiKeyEnvironmentVariable(e.target.value)}
              />
            </Field>
          ) : (
            <Field label="API Key">
              <Input
                type="password"
                autoComplete="new-password"
                className="font-mono placeholder:text-foreground/70"
                value={apiKeyValue}
                placeholder={
                  value.apiKey?.configured
                    ? (value.apiKey.masked ?? "••••••••")
                    : ""
                }
                onChange={(e) => setApiKeyValue(e.target.value)}
              />
            </Field>
          )}
          {canEditModels && (
            <Field label={t("agents.management.fields.models")}>
              <Input
                value={(form.models ?? []).join(", ")}
                onChange={(e) =>
                  setForm({
                    ...form,
                    models: e.target.value
                      .split(",")
                      .map((item) => item.trim())
                      .filter(Boolean),
                  })
                }
              />
            </Field>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <MutationButton
            {...mutationProps}
            sectionId={mutationProps.section.id}
            action="upsert"
            entityId={value.id}
            payload={payload}
            disabled={!payload.id}
            onApplied={onClose}
          >
            {t("common.save")}
          </MutationButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DynamicFormSection(props: EditableSectionProps<ManagedFormSettings>) {
  const { t, i18n } = useTranslation();
  const data = normalizeFormSettings(props.section.data);
  const [values, setValues] = useState(data.values);
  useEffect(() => setValues(data.values), [props.section.data]);
  const language = i18n.resolvedLanguage;
  return (
    <div className="space-y-4">
      {data.groups.map((group) => (
        <Card key={group.id}>
          {(group.title || group.description) && (
            <CardHeader>
              {group.title && (
                <CardTitle className="text-base">
                  {localizedText(group.title, language)}
                </CardTitle>
              )}
              {group.description && (
                <p className="text-sm text-muted-foreground">
                  {localizedText(group.description, language)}
                </p>
              )}
            </CardHeader>
          )}
          <CardContent
            className={cn(
              "grid gap-5 p-5",
              group.columns === 2 && "sm:grid-cols-2",
              (group.title || group.description) && "pt-0",
            )}
          >
            {group.fields.map((field) => (
              <DynamicFormField
                key={field.key}
                field={field}
                value={values[field.key]}
                language={language}
                disabled={props.readOnly}
                onChange={(value) =>
                  setValues((current) => ({
                    ...current,
                    [field.key]: value,
                  }))
                }
              />
            ))}
          </CardContent>
        </Card>
      ))}
      {!props.readOnly && (
        <div className="flex justify-end">
          <MutationButton
            {...props}
            sectionId={props.section.id}
            action="update"
            payload={{ values }}
          >
            {t("common.save")}
          </MutationButton>
        </div>
      )}
    </div>
  );
}

function normalizeFormSettings(
  data: ManagedFormSettings | null | undefined,
): ManagedFormSettings {
  return {
    schemaVersion: 1,
    groups: Array.isArray(data?.groups) ? data.groups : [],
    values: data?.values && typeof data.values === "object" ? data.values : {},
  };
}

function DynamicFormField({
  field,
  value,
  language,
  disabled,
  onChange,
}: {
  field: ManagedFormField;
  value: unknown;
  language?: string;
  disabled: boolean;
  onChange: (value: unknown) => void;
}) {
  const { t } = useTranslation();
  const label = localizedText(field.label, language) ?? field.key;
  const description = localizedText(field.description, language);
  if (field.control === "switch") {
    return (
      <div className="flex items-center justify-between gap-4">
        <div>
          <Label>{label}</Label>
          {description && (
            <p className="mt-1 text-xs text-muted-foreground">{description}</p>
          )}
        </div>
        <Switch
          disabled={disabled}
          checked={
            value === undefined ? field.defaultValue === true : value === true
          }
          onCheckedChange={onChange}
        />
      </div>
    );
  }
  return (
    <Field label={label}>
      {field.control === "select" ? (
        <Select
          disabled={disabled}
          value={typeof value === "string" ? value : "__default"}
          onValueChange={(next) =>
            onChange(next === "__default" ? undefined : next)
          }
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {!field.required && (
              <SelectItem value="__default">
                {t("agents.management.useDefault")}
              </SelectItem>
            )}
            {(field.options ?? []).map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {localizedText(option.label, language) ?? option.value}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : field.control === "textarea" ? (
        <Textarea
          disabled={disabled}
          rows={field.rows ?? 4}
          className={cn("resize-y", field.mono && "font-mono text-xs")}
          placeholder={localizedText(field.placeholder, language)}
          value={
            field.valueType === "string-array"
              ? Array.isArray(value)
                ? value.join("\n")
                : ""
              : typeof value === "string"
                ? value
                : ""
          }
          onChange={(event) =>
            onChange(
              field.valueType === "string-array"
                ? event.target.value
                    .split(/\r?\n/)
                    .map((item) => item.trim())
                    .filter(Boolean)
                : event.target.value,
            )
          }
        />
      ) : (
        <Input
          type={field.control === "password" ? "password" : "text"}
          autoComplete={
            field.control === "password" ? "new-password" : undefined
          }
          disabled={disabled}
          className={cn(field.mono && "font-mono text-xs")}
          placeholder={localizedText(field.placeholder, language)}
          value={typeof value === "string" ? value : ""}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
      {description && (
        <p className="text-xs text-muted-foreground">{description}</p>
      )}
    </Field>
  );
}

function ModelsSection(props: EditableSectionProps<ManagedModelSettings>) {
  const { t } = useTranslation();
  const [form, setForm] = useState(props.section.data);
  useEffect(() => setForm(props.section.data), [props.section.data]);
  return (
    <Card>
      <CardContent className="space-y-4 p-5">
        <Field label={t("agents.management.fields.defaultModel")}>
          <Input
            disabled={props.readOnly}
            value={form.defaultModel ?? ""}
            onChange={(event) =>
              setForm({ ...form, defaultModel: event.target.value })
            }
          />
        </Field>
        <Field label={t("agents.management.fields.smallModel")}>
          <Input
            disabled={props.readOnly}
            value={form.smallModel ?? ""}
            onChange={(event) =>
              setForm({ ...form, smallModel: event.target.value })
            }
          />
        </Field>
        <Field label={t("agents.management.fields.reasoningEffort")}>
          <Input
            disabled={props.readOnly}
            value={form.reasoningEffort ?? ""}
            onChange={(event) =>
              setForm({ ...form, reasoningEffort: event.target.value })
            }
          />
        </Field>
        {!props.readOnly && (
          <div className="flex justify-end">
            <MutationButton
              {...props}
              sectionId={props.section.id}
              action="update"
              payload={form}
            >
              {t("common.save")}
            </MutationButton>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function PermissionsSection(
  props: EditableSectionProps<ManagedPermissionSettings>,
) {
  const { t } = useTranslation();
  const [form, setForm] = useState(() =>
    normalizePermissionSettings(props.section.data),
  );
  useEffect(
    () => setForm(normalizePermissionSettings(props.section.data)),
    [props.section.data],
  );
  const updateRule = (index: number, patch: Partial<ManagedPermissionRule>) =>
    setForm({
      ...form,
      rules: form.rules.map((rule, current) =>
        current === index ? { ...rule, ...patch } : rule,
      ),
    });
  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="grid gap-4 p-5 sm:grid-cols-2">
          <Field label={t("agents.management.fields.approvalMode")}>
            <Input
              disabled={props.readOnly}
              value={form.approvalMode ?? ""}
              onChange={(e) =>
                setForm({ ...form, approvalMode: e.target.value })
              }
            />
          </Field>
          <Field label={t("agents.management.fields.sandboxMode")}>
            <Input
              disabled={props.readOnly}
              value={form.sandboxMode ?? ""}
              onChange={(e) =>
                setForm({ ...form, sandboxMode: e.target.value })
              }
            />
          </Field>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            {t("agents.management.permissionRules")}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          {form.rules.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              {t("agents.management.empty")}
            </p>
          ) : (
            form.rules.map((rule, index) => (
              <div
                key={`${rule.id}-${index}`}
                className="grid grid-cols-[1fr_120px_auto] items-center gap-2"
              >
                <Input
                  disabled={props.readOnly}
                  value={rule.target}
                  onChange={(e) =>
                    updateRule(index, {
                      target: e.target.value,
                      id: e.target.value,
                    })
                  }
                />
                <Select
                  disabled={props.readOnly}
                  value={rule.decision}
                  onValueChange={(
                    decision: ManagedPermissionRule["decision"],
                  ) => updateRule(index, { decision })}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="allow">allow</SelectItem>
                    <SelectItem value="ask">ask</SelectItem>
                    <SelectItem value="deny">deny</SelectItem>
                  </SelectContent>
                </Select>
                {!props.readOnly && (
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() =>
                      setForm({
                        ...form,
                        rules: form.rules.filter(
                          (_, current) => current !== index,
                        ),
                      })
                    }
                  >
                    <IconTrash className="h-4 w-4" />
                  </Button>
                )}
              </div>
            ))
          )}
        </CardContent>
      </Card>
      {!props.readOnly && (
        <div className="flex justify-between">
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              setForm({
                ...form,
                rules: [
                  ...form.rules,
                  {
                    id: `rule-${Date.now()}`,
                    target: "",
                    decision: "ask",
                    kind: "tool",
                  },
                ],
              })
            }
          >
            <IconPlus className="h-4 w-4" />
            {t("common.add")}
          </Button>
          <MutationButton
            {...props}
            sectionId={props.section.id}
            action="update"
            payload={form}
          >
            {t("common.save")}
          </MutationButton>
        </div>
      )}
    </div>
  );
}

function normalizePermissionSettings(
  data: ManagedPermissionSettings | null | undefined,
): ManagedPermissionSettings {
  return {
    approvalMode: data?.approvalMode,
    sandboxMode: data?.sandboxMode,
    rules: Array.isArray(data?.rules) ? data.rules : [],
  };
}

function EnvironmentSection({ data }: { data: ManagedEnvironmentSettings }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            {t("agents.management.configFiles")}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {(data.configFiles ?? []).map((file) => (
            <div key={file.id} className="flex items-center gap-3">
              <Badge variant={file.exists ? "secondary" : "outline"}>
                {file.exists
                  ? t("agents.configFileExists")
                  : t("agents.configFileNotCreated")}
              </Badge>
              <span className="font-medium">{file.label}</span>
              <span className="min-w-0 flex-1 truncate text-right font-mono text-xs text-muted-foreground">
                {file.path}
              </span>
            </div>
          ))}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            {t("agents.management.paths")}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <KeyValue
            label={t("agents.management.fields.configRoot")}
            value={data.cliRoot}
            mono
          />
          <KeyValue
            label={t("agents.management.fields.sessionRoot")}
            value={data.sessionRoot}
            mono
          />
          <KeyValue
            label={t("agents.management.fields.skillRoot")}
            value={data.skillRoot}
            mono
          />
        </CardContent>
      </Card>
    </div>
  );
}

async function cleanupGeneratedKey(
  platformAPI: ReturnType<typeof usePlatformAPI>,
  prefix: string | null,
) {
  if (!prefix) return;
  try {
    const generated = (await platformAPI.accessKeys.list()).find(
      (key) => key.keyPrefix === prefix,
    );
    if (generated) await platformAPI.accessKeys.revoke(generated.id);
  } catch {
    // Keep the original installation error when best-effort cleanup fails.
  }
}

function resolveGatewayEndpoint(settings: {
  endpoint?: string | null;
  listenHost: string;
  listenPort: number;
}) {
  if (!isTauriRuntime()) return getHttpApiBase();
  if (settings.endpoint) return usableMcpEndpoint(settings.endpoint);
  const host =
    settings.listenHost === "0.0.0.0" ? "127.0.0.1" : settings.listenHost;
  return `http://${host}:${settings.listenPort}`;
}

function PromptSection({ entry }: { entry: AgentInstanceEntry }) {
  return <ConfigEditorSection entry={entry} kind="prompt" />;
}

function RawConfigSection({ entry }: { entry: AgentInstanceEntry }) {
  return <ConfigEditorSection entry={entry} kind="config" />;
}

function ConfigEditorSection({
  entry,
  kind,
}: {
  entry: AgentInstanceEntry;
  kind: "config" | "prompt";
}) {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const theme = useThemeStore((state) => state.theme);
  const [files, setFiles] = useState<AgentConfigFileSummary[]>([]);
  const [document, setDocument] = useState<AgentConfigDocument | null>(null);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const darkEditor =
    theme === "dark" ||
    (theme === "system" &&
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  const editorExtensions = useMemo(() => {
    const extensions = [EditorView.lineWrapping];
    if (document?.language === "json" || document?.language === "jsonc") {
      extensions.unshift(javascript());
    }
    return extensions;
  }, [document?.language]);
  const loadFile = useCallback(
    async (fileId: string) => {
      setLoading(true);
      try {
        const next = await platformAPI.agents.configs.read(
          entry.instance.id,
          fileId,
        );
        setDocument(next);
        setContent(next.content);
      } catch (cause) {
        toast.error(errorMessage(cause, t("agents.configLoadFailed")));
      } finally {
        setLoading(false);
      }
    },
    [entry.instance.id, platformAPI, t],
  );
  useEffect(() => {
    void (async () => {
      try {
        const listed = await platformAPI.agents.configs.list(entry.instance.id);
        const next = listed.filter((file) =>
          kind === "prompt" ? file.kind === "prompt" : file.kind !== "prompt",
        );
        setFiles(next);
        if (next[0]) await loadFile(next[0].id);
        else setLoading(false);
      } catch (cause) {
        setLoading(false);
        toast.error(errorMessage(cause, t("agents.configLoadFailed")));
      }
    })();
  }, [entry.instance.id, kind, loadFile, platformAPI, t]);
  if (loading) return <SectionSkeleton />;
  if (!document) return <EmptySection />;
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        {files.length > 1 && (
          <Select
            value={document.id}
            disabled={content !== document.content}
            onValueChange={(id) => void loadFile(id)}
          >
            <SelectTrigger className="w-56">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {files.map((file) => (
                <SelectItem key={file.id} value={file.id}>
                  {file.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
          {document.path}
        </span>
      </div>
      <div className="h-[min(64vh,680px)] min-h-[460px] overflow-hidden rounded-md border bg-background">
        <CodeMirror
          className="h-full [&_.cm-editor]:h-full [&_.cm-scroller]:overflow-auto"
          height="100%"
          value={content}
          theme={darkEditor ? oneDark : "light"}
          extensions={editorExtensions}
          onChange={setContent}
          basicSetup={{
            lineNumbers: true,
            foldGutter: true,
            dropCursor: true,
            indentOnInput: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: true,
            highlightSelectionMatches: true,
            searchKeymap: true,
          }}
        />
      </div>
      <div className="flex justify-end">
        <Button
          disabled={content === document.content}
          onClick={async () => {
            try {
              let nextContent = content;
              if (document.language === "json")
                nextContent = `${JSON.stringify(JSON.parse(content), null, 2)}\n`;
              const saved = await platformAPI.agents.configs.save(
                entry.instance.id,
                document.id,
                nextContent,
                document.revision,
              );
              setDocument(saved);
              setContent(saved.content);
              toast.success(t("agents.configSaved"));
            } catch (cause) {
              toast.error(errorMessage(cause, t("agents.configSaveFailed")));
            }
          }}
        >
          {t("common.save")}
        </Button>
      </div>
    </div>
  );
}

type EditableSectionProps<T> = {
  instanceId: string;
  section: AgentManagementSection<T>;
  readOnly: boolean;
  reload: () => Promise<void>;
};

function MutationButton<T>({
  instanceId,
  section,
  readOnly: _readOnly,
  sectionId,
  action,
  entityId,
  payload,
  reload,
  onApplied,
  children,
  ...buttonProps
}: EditableSectionProps<T> & {
  sectionId: AgentManagementSectionId;
  action: string;
  entityId?: string;
  payload?: unknown;
  onApplied?: () => void;
} & React.ComponentProps<typeof Button>) {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [busy, setBusy] = useState(false);
  const apply = async () => {
    const next: AgentManagementMutation = {
      section: sectionId,
      action,
      entityId,
      payload,
      expectedRevision: section.revision,
    };
    setBusy(true);
    try {
      await platformAPI.agents.management.apply(instanceId, next);
      toast.success(t("agents.management.saved"));
      onApplied?.();
      await reload();
    } catch (cause) {
      toast.error(errorMessage(cause, t("agents.management.saveFailed")));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Button
      {...buttonProps}
      disabled={buttonProps.disabled || busy}
      onClick={() => void apply()}
    >
      {children}
    </Button>
  );
}

function Field({
  label,
  children,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      {children}
    </div>
  );
}

function localizedText(
  value: ManagedLocalizedText | undefined,
  language = "en",
): string | undefined {
  if (typeof value === "string") return value;
  if (!value) return undefined;
  const shortLanguage = language.split("-")[0];
  return (
    value[language] ??
    value[shortLanguage] ??
    value.en ??
    Object.values(value)[0]
  );
}

function KeyValue({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: unknown;
  mono?: boolean;
}) {
  return (
    <div className="grid grid-cols-[150px_minmax(0,1fr)] gap-4 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className={cn("min-w-0 truncate", mono && "font-mono text-xs")}>
        {value === null || value === undefined || value === ""
          ? "—"
          : String(value)}
      </span>
    </div>
  );
}
function EmptySection() {
  const { t } = useTranslation();
  return (
    <div className="flex min-h-52 items-center justify-center rounded-lg border border-dashed text-sm text-muted-foreground">
      {t("agents.management.empty")}
    </div>
  );
}
function SectionSkeleton() {
  return (
    <div className="space-y-3">
      <Skeleton className="h-24 w-full" />
      <Skeleton className="h-24 w-full" />
      <Skeleton className="h-40 w-full" />
    </div>
  );
}
function ManagementSkeleton() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-20 items-center gap-3 border-b px-6">
        <Skeleton className="h-10 w-10 rounded-md" />
        <Skeleton className="h-5 w-44" />
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="w-52 border-r p-4">
          <SectionSkeleton />
        </div>
        <div className="min-w-0 flex-1 p-6">
          <SectionSkeleton />
        </div>
      </div>
    </div>
  );
}
function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error
    ? error.message
    : typeof error === "string"
      ? error
      : fallback;
}

export default AgentManagementPage;
