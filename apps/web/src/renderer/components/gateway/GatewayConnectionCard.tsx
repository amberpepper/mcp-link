import { useTranslation } from "react-i18next";
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Label,
} from "@mcp_link/ui";
import type { GatewaySettings } from "@mcp_link/shared";
import { Copy, Eye, EyeOff, RefreshCw } from "lucide-react";

import NetworkAddressSelect from "@/renderer/components/common/NetworkAddressSelect";

interface GatewayConnectionCardProps {
  endpoint: string;
  listenerError: string | null;
  settings: GatewaySettings | null;
  isDesktopRuntime: boolean;
  listenHost: string;
  listenPort: string;
  saving: boolean;
  showAccessKey: boolean;
  onListenHostChange: (value: string) => void;
  onListenPortChange: (value: string) => void;
  onShowAccessKeyChange: (value: boolean) => void;
  onCopy: (value: string) => Promise<void>;
  onRegenerateKey: () => Promise<void>;
  onSaveSettings: () => Promise<void>;
}

export function GatewayConnectionCard({
  endpoint,
  listenerError,
  settings,
  isDesktopRuntime,
  listenHost,
  listenPort,
  saving,
  showAccessKey,
  onListenHostChange,
  onListenPortChange,
  onShowAccessKeyChange,
  onCopy,
  onRegenerateKey,
  onSaveSettings,
}: GatewayConnectionCardProps) {
  const { t } = useTranslation();
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-xl">{t("gateway.connection")}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-5">
        {listenerError && (
          <p className="text-sm text-destructive">
            {t("gateway.listenerError", { error: listenerError })}
          </p>
        )}
        <div className="grid gap-4 lg:grid-cols-2">
          <EndpointField
            label={t("gateway.openaiBaseUrl")}
            value={endpoint ? `${endpoint}/openai/v1` : ""}
            onCopy={onCopy}
          />
          <EndpointField
            label={t("gateway.anthropicBaseUrl")}
            value={endpoint ? `${endpoint}/anthropic` : ""}
            onCopy={onCopy}
          />
        </div>

        <div className="space-y-2">
          <Label>{t("gateway.accessKey")}</Label>
          <div className="flex gap-2">
            <Input
              readOnly
              type={showAccessKey ? "text" : "password"}
              value={settings?.accessKey ?? ""}
              className="hide-native-password-toggle font-mono"
            />
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={() => onShowAccessKeyChange(!showAccessKey)}
              aria-label={
                showAccessKey
                  ? t("common.hidePassword")
                  : t("common.showPassword")
              }
            >
              {showAccessKey ? <EyeOff /> : <Eye />}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={() => void onCopy(settings?.accessKey ?? "")}
              aria-label={t("gateway.copy")}
            >
              <Copy />
            </Button>
            <Button
              type="button"
              variant="outline"
              size="icon"
              disabled={saving}
              onClick={() => void onRegenerateKey()}
              aria-label={t("gateway.regenerateKey")}
            >
              <RefreshCw />
            </Button>
          </div>
        </div>

        {isDesktopRuntime && (
          <div className="grid items-end gap-4 sm:grid-cols-[1fr_160px_auto]">
            <div className="space-y-2">
              <Label>{t("gateway.listenHost")}</Label>
              <NetworkAddressSelect
                value={listenHost}
                onValueChange={onListenHostChange}
                disabled={saving}
                placeholder={t("gateway.listenHost")}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="gateway-port">{t("gateway.listenPort")}</Label>
              <Input
                id="gateway-port"
                inputMode="numeric"
                value={listenPort}
                onChange={(event) => onListenPortChange(event.target.value)}
              />
            </div>
            <Button disabled={saving} onClick={() => void onSaveSettings()}>
              {t("common.save")}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function EndpointField({
  label,
  value,
  onCopy,
}: {
  label: string;
  value: string;
  onCopy: (value: string) => Promise<void>;
}) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      <div className="flex gap-2">
        <Input readOnly value={value} className="font-mono text-xs" />
        <Button
          type="button"
          variant="outline"
          size="icon"
          onClick={() => void onCopy(value)}
        >
          <Copy />
        </Button>
      </div>
    </div>
  );
}
