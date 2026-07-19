import type { TFunction } from "i18next";
import type {
  AgentPluginDescriptor,
  AgentSessionGroup,
  AgentSessionMessage,
  AgentSessionMessageGroup,
  AgentSessionSummary,
  UserMessageNavItem,
  VisibleAgentSessionMessage,
} from "@mcp_link/shared";

const MAX_MESSAGES_PER_GROUP = 4;

export function formatSessionDate(value?: number | null) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const now = new Date();
  if (date.getFullYear() === now.getFullYear()) {
    return new Intl.DateTimeFormat(undefined, {
      month: "2-digit",
      day: "2-digit",
    }).format(date);
  }
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
}

export function buildAgentGroups(
  sessions: AgentSessionSummary[],
  plugins: AgentPluginDescriptor[],
  noWorkspaceLabel: string,
): AgentSessionGroup[] {
  const pluginMap = new Map(plugins.map((plugin) => [plugin.id, plugin]));
  const agents = new Map<string, AgentSessionGroup>();
  for (const plugin of plugins) {
    if (
      plugin.enabled &&
      plugin.instances.some((instance) => instance.enabled) &&
      plugin.capabilities.includes("sessions.list")
    ) {
      agents.set(plugin.id, { key: plugin.id, plugin, groups: [] });
    }
  }
  for (const session of sessions) {
    const plugin = pluginMap.get(session.agentId);
    if (!plugin) continue;
    let agent = agents.get(session.agentId);
    if (!agent) {
      agent = { key: session.agentId, plugin, groups: [] };
      agents.set(session.agentId, agent);
    }
    const path = session.cwd ?? session.repository ?? null;
    const label = path
      ? (path.split(/[\\/]/).filter(Boolean).at(-1) ?? path)
      : noWorkspaceLabel;
    const key = path ? workspaceGroupKey(path) : "__no_workspace__";
    let group = agent.groups.find((item) => item.key === key);
    if (!group) {
      group = { key, label, path, sessions: [] };
      agent.groups.push(group);
    }
    group.sessions.push(session);
  }
  for (const agent of agents.values()) {
    for (const group of agent.groups) {
      group.sessions.sort(
        (left, right) =>
          (right.updatedAt ?? right.createdAt ?? 0) -
          (left.updatedAt ?? left.createdAt ?? 0),
      );
    }
    agent.groups.sort((left, right) => left.label.localeCompare(right.label));
  }
  return Array.from(agents.values()).sort((left, right) =>
    left.plugin.name.localeCompare(right.plugin.name),
  );
}

export function sessionSourceName(
  plugins: AgentPluginDescriptor[],
  session: AgentSessionSummary,
) {
  const plugin = plugins.find((item) => item.id === session.agentId);
  const instance = plugin?.instances.find(
    (item) => item.id === session.sourceInstanceId,
  );
  return combinedSourceName(
    plugin?.name ?? session.agentId,
    instance?.label ?? session.sourceLabel,
  );
}

export function isToolMessage(item: AgentSessionMessage) {
  return item.kind === "tool-call" || item.kind === "tool-result";
}

export function isInternalContextText(value: string) {
  const text = value.trimStart();
  return [
    "<environment_context",
    "<permissions",
    "<collaboration_mode",
    "<multi_agent_mode",
    "<codex_internal_context",
    "<model_switch",
  ].some((prefix) => text.startsWith(prefix));
}

function workspaceGroupKey(path: string) {
  const normalized = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const isWindowsDrive = /^[a-z]:\//i.test(normalized);
  const isWindowsMountedDrive = /^\/mnt\/[a-z]\//i.test(normalized);
  return isWindowsDrive || isWindowsMountedDrive
    ? normalized.toLocaleLowerCase("en-US")
    : normalized;
}

export function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

export function warningMessage(t: TFunction, warning: string) {
  if (warning.startsWith("openAfterImportFailed::")) {
    return t("sessions.warnings.openAfterImportFailed", {
      error: warning.slice("openAfterImportFailed::".length),
    });
  }
  return t(`sessions.warnings.${warning}`, { defaultValue: warning });
}

export function prettyJson(value: unknown) {
  if (typeof value === "string") return value;
  if (value == null) return "";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function groupVisibleMessages(
  messages: VisibleAgentSessionMessage[],
): AgentSessionMessageGroup[] {
  const groups: AgentSessionMessageGroup[] = [];
  for (const message of messages) {
    const role = messageGroupRole(message.item);
    const current = groups.at(-1);
    const messageModel = message.item.model ?? null;
    const currentModel = current?.messages[0]?.item.model ?? null;
    if (
      current?.role === role &&
      currentModel === messageModel &&
      current.messages.length < MAX_MESSAGES_PER_GROUP
    ) {
      current.messages.push(message);
      continue;
    }
    groups.push({
      key: `${role}-${message.item.id}-${message.originalIndex}`,
      role,
      messages: [message],
      continuation: current?.role === role && currentModel === messageModel,
    });
  }
  return groups;
}

export function sliceMessageGroups(
  groups: AgentSessionMessageGroup[],
  messageStart: number,
) {
  if (messageStart <= 0) return groups;
  let remaining = messageStart;
  const visible: AgentSessionMessageGroup[] = [];
  for (const group of groups) {
    if (remaining >= group.messages.length) {
      remaining -= group.messages.length;
      continue;
    }
    if (remaining > 0) {
      const messages = group.messages.slice(remaining);
      const first = messages[0];
      visible.push({
        ...group,
        key: `${group.role}-${first.item.id}-${first.originalIndex}`,
        messages,
        continuation: false,
      });
      remaining = 0;
      continue;
    }
    visible.push(
      visible.length === 0 && group.continuation
        ? { ...group, continuation: false }
        : group,
    );
  }
  return visible;
}

export function roleClass(role: string) {
  if (role === "user") return "bg-primary/5";
  if (role === "assistant") return "bg-muted/30";
  if (role === "tool") return "bg-amber-500/5";
  return "bg-destructive/5";
}

export function messageAlignmentClass(role: AgentSessionMessage["role"]) {
  if (role === "user") return "items-end";
  if (role === "system") return "items-center";
  return "items-start";
}

export function formatMessageTimestamp(value?: number | null) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  const sameYear = date.getFullYear() === now.getFullYear();
  const label = new Intl.DateTimeFormat(undefined, {
    ...(!sameDay && { month: "2-digit", day: "2-digit" }),
    ...(!sameYear && { year: "numeric" }),
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
  return { label, dateTime: date.toISOString(), title: date.toLocaleString() };
}

export function chatMessageShellClass(
  role: AgentSessionMessage["role"],
  toolContent = false,
) {
  if (toolContent) return "w-full max-w-[94%] xl:max-w-[86%]";
  if (role === "user") return "w-fit max-w-[90%] xl:max-w-[78%]";
  return "w-fit max-w-[94%] xl:max-w-[86%]";
}

export function sampleNavItems(
  items: UserMessageNavItem[],
  activeMessageId: string | null,
  limit: number,
) {
  if (items.length <= limit) return items;
  const positions = new Set<number>();
  const sampleCount = Math.max(1, limit - 1);
  for (let index = 0; index < sampleCount; index += 1) {
    positions.add(
      Math.round((index * (items.length - 1)) / Math.max(1, sampleCount - 1)),
    );
  }
  const activePosition = items.findIndex(
    (item) => item.messageId === activeMessageId,
  );
  if (activePosition >= 0) positions.add(activePosition);
  return Array.from(positions)
    .sort((left, right) => left - right)
    .slice(0, limit)
    .map((position) => items[position]);
}

export function downloadResult(
  fileName: string,
  mimeType: string,
  content: string,
  encoding: "utf8" | "base64",
) {
  const blob =
    encoding === "base64"
      ? new Blob([base64Bytes(content)], { type: mimeType })
      : new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function combinedSourceName(plugin: string, source?: string | null) {
  if (!source || source === plugin || source.startsWith(`${plugin} · `)) {
    return source || plugin;
  }
  return `${plugin} · ${source}`;
}

function messageGroupRole(
  item: AgentSessionMessage,
): AgentSessionMessageGroup["role"] {
  if (item.role === "user") return "user";
  if (item.role === "system") return "system";
  return "assistant";
}

function base64Bytes(content: string) {
  const binary = window.atob(content);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}
