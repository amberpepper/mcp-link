import React, { useEffect, useState } from "react";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@mcp_link/ui";
import { IconPackageImport, IconUpload } from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { usePlatformAPI } from "@/renderer/platform-api";
const MAX_PLUGIN_BYTES = 128 * 1024 * 1024;

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? error.message : fallback;
}

function pluginFiles(transfer: DataTransfer | null): File[] {
  return Array.from(transfer?.files ?? []).filter((file) =>
    /\.(mclagent|zip)$/i.test(file.name),
  );
}

export default function AgentPluginDropInstaller({
  enabled,
}: {
  enabled: boolean;
}) {
  const { t } = useTranslation();
  const platformAPI = usePlatformAPI();
  const [dragging, setDragging] = useState(false);
  const [pending, setPending] = useState<File[]>([]);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let depth = 0;
    const enter = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      depth += 1;
      setDragging(true);
    };
    const over = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    };
    const leave = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      depth = Math.max(0, depth - 1);
      if (depth === 0) setDragging(false);
    };
    const drop = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      depth = 0;
      setDragging(false);
      const files = pluginFiles(event.dataTransfer);
      if (files.length === 0) {
        toast.error(t("agents.dropInvalid"));
        return;
      }
      setPending(files);
    };
    window.addEventListener("dragenter", enter);
    window.addEventListener("dragover", over);
    window.addEventListener("dragleave", leave);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragenter", enter);
      window.removeEventListener("dragover", over);
      window.removeEventListener("dragleave", leave);
      window.removeEventListener("drop", drop);
    };
  }, [enabled, t]);

  const install = async () => {
    setInstalling(true);
    let installed = 0;
    try {
      for (const file of pending) {
        if (file.size > MAX_PLUGIN_BYTES) {
          throw new Error(t("agents.dropTooLarge", { name: file.name }));
        }
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
        await platformAPI.agents.plugins.install(bytes);
        installed += 1;
      }
      toast.success(t("agents.dropInstalled", { count: installed }));
      setPending([]);
      window.dispatchEvent(new CustomEvent("agent-plugins-changed"));
    } catch (cause) {
      toast.error(errorMessage(cause, t("agents.importFailed")));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <>
      {dragging && (
        <div className="pointer-events-none fixed inset-0 z-[100] flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="flex min-w-80 flex-col items-center gap-4 rounded-2xl border-2 border-dashed border-primary bg-card p-10 shadow-2xl">
            <IconUpload className="h-12 w-12 text-primary" />
            <div className="text-lg font-semibold">{t("agents.dropHere")}</div>
            <div className="text-sm text-muted-foreground">.mclagent</div>
          </div>
        </div>
      )}
      <Dialog
        open={pending.length > 0}
        onOpenChange={(open) => !open && !installing && setPending([])}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <IconPackageImport className="h-5 w-5" />
              {t("agents.dropConfirmTitle")}
            </DialogTitle>
            <DialogDescription>
              {t("agents.dropConfirmDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-52 space-y-2 overflow-y-auto rounded-md border p-3">
            {pending.map((file) => (
              <div
                key={`${file.name}-${file.size}`}
                className="flex gap-3 text-sm"
              >
                <span className="min-w-0 flex-1 truncate font-medium">
                  {file.name}
                </span>
                <span className="text-muted-foreground">
                  {(file.size / 1024).toFixed(1)} KB
                </span>
              </div>
            ))}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={installing}
              onClick={() => setPending([])}
            >
              {t("common.cancel")}
            </Button>
            <Button disabled={installing} onClick={() => void install()}>
              {installing ? t("common.loading") : t("agents.install")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
