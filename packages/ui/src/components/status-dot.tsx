import * as React from "react";

import { cn } from "../lib/utils";

export type StatusDotTone =
  | "running"
  | "starting"
  | "stopping"
  | "stopped"
  | "error"
  | "config";

const TONE_CLASSES: Record<StatusDotTone, string> = {
  running: "bg-green-500",
  starting: "bg-yellow-500 animate-pulse",
  stopping: "bg-yellow-500 animate-pulse",
  stopped: "bg-slate-400",
  error: "bg-red-500",
  config: "bg-amber-500",
};

interface StatusDotProps extends React.HTMLAttributes<HTMLSpanElement> {
  tone: StatusDotTone;
  size?: "sm" | "md";
}

export function StatusDot({
  tone,
  size = "sm",
  className,
  ...props
}: StatusDotProps) {
  return (
    <span
      className={cn(
        "inline-block shrink-0 rounded-full",
        size === "sm" ? "h-2 w-2" : "h-2.5 w-2.5",
        TONE_CLASSES[tone],
        className,
      )}
      aria-label={tone}
      {...props}
    />
  );
}
