import type { Dispatch, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import {
  Badge,
  Button,
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
} from "@mcp_link/ui";
import type {
  GatewayProtocol,
  GatewayProvider,
  GatewayProviderDraft,
  GatewayRemoveTarget,
  GatewayRoute,
} from "@mcp_link/shared";
import {
  Eye,
  EyeOff,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";

interface GatewayProviderDialogProps {
  open: boolean;
  editingProvider: GatewayProvider | null;
  draft: GatewayProviderDraft;
  routes: GatewayRoute[];
  saving: boolean;
  fetchingModels: boolean;
  protocolLocked: boolean;
  showApiKey: boolean;
  onOpenChange: (open: boolean) => void;
  onDraftChange: Dispatch<SetStateAction<GatewayProviderDraft>>;
  onShowApiKeyChange: (show: boolean) => void;
  onFetchModels: () => Promise<void>;
  onSave: () => Promise<void>;
  onCreateRoute: (providerId: string) => void;
  onEditRoute: (route: GatewayRoute) => void;
  onRemove: (target: GatewayRemoveTarget) => void;
}

export function GatewayProviderDialog({
  open,
  editingProvider,
  draft,
  routes,
  saving,
  fetchingModels,
  protocolLocked,
  showApiKey,
  onOpenChange,
  onDraftChange,
  onShowApiKeyChange,
  onFetchModels,
  onSave,
  onCreateRoute,
  onEditRoute,
  onRemove,
}: GatewayProviderDialogProps) {
  const { t } = useTranslation();
  const providerRoutes = editingProvider
    ? routes.filter((route) => route.providerId === editingProvider.id)
    : [];
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] max-w-lg overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {editingProvider
              ? t("gateway.editProvider")
              : t("gateway.addProvider")}
          </DialogTitle>
          <DialogDescription>
            {t("gateway.providerDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-4 py-2">
          <Field label={t("gateway.name")}>
            <Input
              value={draft.name}
              onChange={(event) =>
                onDraftChange((current) => ({
                  ...current,
                  name: event.target.value,
                }))
              }
            />
          </Field>
          <Field label={t("gateway.protocol")}>
            <Select
              value={draft.protocol}
              disabled={protocolLocked}
              onValueChange={(protocol) =>
                onDraftChange((current) => ({
                  ...current,
                  protocol: protocol as GatewayProtocol,
                  models: [],
                }))
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="openai-compatible">
                  OpenAI Compatible
                </SelectItem>
                <SelectItem value="openai-responses">
                  OpenAI Responses
                </SelectItem>
                <SelectItem value="anthropic">Anthropic</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          <Field label={t("gateway.baseUrl")}>
            <Input
              placeholder="https://api.openai.com/v1"
              value={draft.baseUrl}
              onChange={(event) =>
                onDraftChange((current) => ({
                  ...current,
                  baseUrl: event.target.value,
                  models: [],
                }))
              }
            />
          </Field>
          <Field label={t("gateway.providerApiKey")}>
            <div className="flex gap-2">
              <Input
                type={showApiKey ? "text" : "password"}
                className="hide-native-password-toggle"
                value={draft.apiKey}
                onChange={(event) =>
                  onDraftChange((current) => ({
                    ...current,
                    apiKey: event.target.value,
                    models: [],
                  }))
                }
              />
              <Button
                type="button"
                variant="outline"
                size="icon"
                onClick={() => onShowApiKeyChange(!showApiKey)}
              >
                {showApiKey ? <EyeOff /> : <Eye />}
              </Button>
            </div>
          </Field>
          <Field label={t("gateway.availableModels")}>
            <div className="space-y-2">
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs text-muted-foreground">
                  {draft.models.length > 0
                    ? t("gateway.modelsCount", { count: draft.models.length })
                    : t("gateway.noFetchedModels")}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={fetchingModels || !draft.baseUrl.trim()}
                  onClick={() => void onFetchModels()}
                >
                  {fetchingModels ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <RefreshCw className="mr-2 h-4 w-4" />
                  )}
                  {t("gateway.fetchModels")}
                </Button>
              </div>
              {draft.models.length > 0 && (
                <div className="max-h-36 overflow-y-auto rounded-md border bg-muted/20 p-2">
                  <div className="flex flex-wrap gap-1.5">
                    {draft.models.map((model) => (
                      <Badge
                        key={model}
                        variant="secondary"
                        className="font-mono text-[10px]"
                      >
                        {model}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </Field>
          {editingProvider && (
            <Field label={t("gateway.modelMappings")}>
              <div className="space-y-2 rounded-md border p-3">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs text-muted-foreground">
                    {t("gateway.mappingDescription")}
                  </span>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => onCreateRoute(editingProvider.id)}
                  >
                    <Plus className="mr-2 h-4 w-4" />
                    {t("gateway.addMapping")}
                  </Button>
                </div>
                {providerRoutes.length === 0 ? (
                  <p className="text-xs text-muted-foreground">
                    {t("gateway.noMappings")}
                  </p>
                ) : (
                  <div className="divide-y">
                    {providerRoutes.map((route) => (
                      <div
                        key={route.id}
                        className="flex items-center gap-2 py-2 first:pt-0 last:pb-0"
                      >
                        <span className="min-w-0 flex-1 truncate font-mono text-xs">
                          {route.alias}
                        </span>
                        <span className="text-xs text-muted-foreground">→</span>
                        <span className="min-w-0 max-w-[45%] truncate font-mono text-xs text-muted-foreground">
                          {route.upstreamModel}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          onClick={() => onEditRoute(route)}
                        >
                          <Pencil />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          onClick={() =>
                            onRemove({ type: "route", item: route })
                          }
                        >
                          <Trash2 />
                        </Button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </Field>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button
            disabled={saving || fetchingModels}
            onClick={() => void onSave()}
          >
            {t("common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      {children}
    </div>
  );
}
