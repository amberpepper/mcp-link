import React, { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AgentPluginDescriptor } from "@mcp_link/shared";

import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";

interface AgentAvatarProps {
  plugin: Pick<AgentPluginDescriptor, "name" | "icon"> | null;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const SIZE_CLASS = {
  sm: "h-8 w-8 rounded-md text-xs",
  md: "h-9 w-9 rounded-lg text-sm",
  lg: "h-10 w-10 rounded-lg text-base",
};

const AgentAvatar: React.FC<AgentAvatarProps> = ({
  plugin,
  size = "md",
  className = "",
}) => {
  const [imageFailed, setImageFailed] = useState(false);
  const imageSource = useMemo(
    () => resolveAgentIconSource(plugin?.icon),
    [plugin?.icon],
  );

  useEffect(() => setImageFailed(false), [imageSource]);

  return (
    <div
      className={`flex shrink-0 items-center justify-center overflow-hidden bg-primary/10 font-semibold text-primary ${SIZE_CLASS[size]} ${className}`}
    >
      {imageSource && !imageFailed ? (
        <img
          src={imageSource}
          alt={plugin?.name ?? ""}
          className="h-full w-full object-cover"
          onError={() => setImageFailed(true)}
        />
      ) : (
        (plugin?.name || "AI").slice(0, 1).toUpperCase()
      )}
    </div>
  );
};

export function resolveAgentIconSource(icon?: string | null) {
  if (!icon) return null;
  if (/^(data:|blob:|https?:\/\/)/i.test(icon)) return icon;
  return isTauriRuntime() ? convertFileSrc(icon) : null;
}

export default AgentAvatar;
