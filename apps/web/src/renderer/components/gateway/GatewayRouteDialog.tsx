import type { Dispatch, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import {
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
  GatewayProvider,
  GatewayRoute,
  GatewayRouteDraft,
} from "@mcp_link/shared";
import { Loader2, RefreshCw } from "lucide-react";

import { protocolLabel } from "./gateway-utils";

interface GatewayRouteDialogProps {
  open: boolean;
  editingRoute: GatewayRoute | null;
  draft: GatewayRouteDraft;
  provider: GatewayProvider | undefined;
  models: string[];
  saving: boolean;
  fetchingModels: boolean;
  onOpenChange: (open: boolean) => void;
  onDraftChange: Dispatch<SetStateAction<GatewayRouteDraft>>;
  onFetchModels: () => Promise<void>;
  onSave: () => Promise<void>;
}

export function GatewayRouteDialog({
  open,
  editingRoute,
  draft,
  provider,
  models,
  saving,
  fetchingModels,
  onOpenChange,
  onDraftChange,
  onFetchModels,
  onSave,
}: GatewayRouteDialogProps) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {editingRoute ? t("gateway.editMapping") : t("gateway.addMapping")}
          </DialogTitle>
          <DialogDescription>
            {t("gateway.mappingDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-4 py-2">
          <Field label={t("gateway.modelAlias")}>
            <Input
              placeholder="my-model"
              value={draft.alias}
              onChange={(event) =>
                onDraftChange((current) => ({
                  ...current,
                  alias: event.target.value,
                }))
              }
            />
          </Field>
          <Field label={t("gateway.provider")}>
            <Input
              readOnly
              value={
                provider
                  ? `${provider.name} · ${protocolLabel(provider.protocol)}`
                  : ""
              }
            />
          </Field>
          <Field label={t("gateway.upstreamModel")}>
            <div className="flex gap-2">
              {models.length > 0 ? (
                <Select
                  value={draft.upstreamModel}
                  disabled={fetchingModels}
                  onValueChange={(upstreamModel) =>
                    onDraftChange((current) => ({
                      ...current,
                      upstreamModel,
                    }))
                  }
                >
                  <SelectTrigger className="min-w-0 flex-1 font-mono">
                    <SelectValue placeholder={t("gateway.upstreamModel")} />
                  </SelectTrigger>
                  <SelectContent>
                    {models.map((model) => (
                      <SelectItem key={model} value={model}>
                        {model}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <Input
                  className="min-w-0 flex-1 font-mono"
                  placeholder="gpt-4.1"
                  value={draft.upstreamModel}
                  disabled={fetchingModels}
                  onChange={(event) =>
                    onDraftChange((current) => ({
                      ...current,
                      upstreamModel: event.target.value,
                    }))
                  }
                />
              )}
              <Button
                type="button"
                variant="outline"
                size="icon"
                disabled={fetchingModels}
                aria-label={t("gateway.fetchModels")}
                onClick={() => void onFetchModels()}
              >
                {fetchingModels ? (
                  <Loader2 className="animate-spin" />
                ) : (
                  <RefreshCw />
                )}
              </Button>
            </div>
          </Field>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button disabled={saving} onClick={() => void onSave()}>
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
