import React from "react";
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  StatusDot,
  Switch,
} from "@mcp_link/ui";
import { Copy, Edit2, MoreVertical, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { HookRule } from "./hook-rule-adapter";

interface HookRowProps {
  rule: HookRule;
  onClick: () => void;
  onToggle: (checked: boolean) => void | Promise<void>;
  onDuplicate: () => void | Promise<void>;
  onDelete: () => void | Promise<void>;
}

const HookRow: React.FC<HookRowProps> = ({
  rule,
  onClick,
  onToggle,
  onDuplicate,
  onDelete,
}) => {
  const { t } = useTranslation();

  return (
    <div
      role="button"
      tabIndex={0}
      className="group flex w-full cursor-pointer items-center gap-4 p-4 text-left transition-colors hover:bg-muted/50"
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick();
        }
      }}
    >
      <StatusDot tone={rule.enabled ? "running" : "stopped"} />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <div className="truncate text-sm font-medium">{rule.name}</div>
        </div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">
          {t("hooks.ruleSubtitle", {
            method: rule.method,
            timing: t(`hooks.timing.${rule.timing}`),
          })}
        </div>
      </div>
      <div
        className="flex shrink-0 items-center gap-2"
        onClick={(event) => event.stopPropagation()}
      >
        <Switch
          checked={rule.enabled}
          onCheckedChange={onToggle}
          aria-label={t("hooks.enabled")}
        />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 opacity-0 transition-opacity group-hover:opacity-100"
              title={t("common.settings")}
            >
              <MoreVertical className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              onClick={(event) => {
                event.stopPropagation();
                onClick();
              }}
            >
              <Edit2 className="h-4 w-4" />
              {t("common.edit")}
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={(event) => {
                event.stopPropagation();
                void onDuplicate();
              }}
            >
              <Copy className="h-4 w-4" />
              {t("hooks.duplicate")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={(event) => {
                event.stopPropagation();
                void onDelete();
              }}
              className="text-destructive focus:text-destructive"
            >
              <Trash2 className="h-4 w-4" />
              {t("common.delete")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
};

export default HookRow;
