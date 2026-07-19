import { useTranslation } from "react-i18next";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Switch,
} from "@mcp_link/ui";
import type {
  GatewayProvider,
  GatewayRemoveTarget,
  GatewayRoute,
  GatewaySettings,
} from "@mcp_link/shared";
import { Pencil, Plus, Trash2 } from "lucide-react";

import { protocolLabel } from "./gateway-utils";

interface GatewayProviderListProps {
  loading: boolean;
  saving: boolean;
  providers: GatewayProvider[];
  routes: GatewayRoute[];
  settings: GatewaySettings | null;
  onCreate: () => void;
  onEdit: (provider: GatewayProvider) => void;
  onActivate: (provider: GatewayProvider) => Promise<void>;
  onRemove: (target: GatewayRemoveTarget) => void;
}

export function GatewayProviderList({
  loading,
  saving,
  providers,
  routes,
  settings,
  onCreate,
  onEdit,
  onActivate,
  onRemove,
}: GatewayProviderListProps) {
  const { t } = useTranslation();
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between gap-4">
        <CardTitle className="text-xl">{t("gateway.providers")}</CardTitle>
        <Button size="sm" onClick={onCreate}>
          <Plus className="mr-2 h-4 w-4" />
          {t("gateway.addProvider")}
        </Button>
      </CardHeader>
      <CardContent>
        {loading ? (
          <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
        ) : providers.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("gateway.noProviders")}
          </p>
        ) : (
          <div className="divide-y">
            {providers.map((provider) => (
              <div
                key={provider.id}
                className="flex items-center gap-4 py-4 first:pt-0 last:pb-0"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">
                      {provider.name}
                    </span>
                    <Badge variant="secondary">
                      {protocolLabel(provider.protocol)}
                    </Badge>
                  </div>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    {provider.baseUrl}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t("gateway.mappingCount", {
                      count: routes.filter(
                        (route) => route.providerId === provider.id,
                      ).length,
                    })}
                  </p>
                </div>
                <Switch
                  checked={settings?.activeProviderId === provider.id}
                  disabled={saving}
                  onCheckedChange={(checked) =>
                    checked && void onActivate(provider)
                  }
                />
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => onEdit(provider)}
                >
                  <Pencil />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => onRemove({ type: "provider", item: provider })}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
