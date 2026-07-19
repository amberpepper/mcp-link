import { useTranslation } from "react-i18next";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@mcp_link/ui";
import type { GatewayRemoveTarget } from "@mcp_link/shared";

interface GatewayRemoveDialogProps {
  target: GatewayRemoveTarget | null;
  saving: boolean;
  onTargetChange: (target: GatewayRemoveTarget | null) => void;
  onConfirm: () => Promise<void>;
}

export function GatewayRemoveDialog({
  target,
  saving,
  onTargetChange,
  onConfirm,
}: GatewayRemoveDialogProps) {
  const { t } = useTranslation();
  return (
    <AlertDialog
      open={Boolean(target)}
      onOpenChange={(open) => !open && onTargetChange(null)}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("gateway.removeTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("gateway.removeDescription")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={saving} onClick={() => void onConfirm()}>
            {t("common.delete")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
