import type { TFunction } from "i18next";
import {
  IconBrandGithub,
  IconDownload,
  IconExternalLink,
  IconPlus,
  IconShieldCheck,
  IconStar,
} from "@tabler/icons-react";

import {
  formatCount,
  type MarketCardItem,
  type MarketLink,
  type MarketMetaItem,
} from "@/renderer/components/common/MarketView";
import {
  registryServerToConfig,
  type RegistryServerEntry,
  type RegistrySource,
} from "@/renderer/services/registry-service";
import type { SkillMarketEntry } from "@/renderer/services/skill-market-service";

const OFFICIAL_META_KEY = "io.modelcontextprotocol.registry/official";

export function registryMarketItem(
  entry: RegistryServerEntry,
  source: RegistrySource,
  adding: boolean,
  onAdd: (entry: RegistryServerEntry) => void,
  t: TFunction,
): MarketCardItem {
  const server = entry.server;
  const config = registryServerToConfig(server);
  const transport =
    server.remotes?.[0]?.type ??
    server.packages?.[0]?.registryType ??
    "unknown";
  const official = entry._meta?.[OFFICIAL_META_KEY];
  const useCount = entry._meta?.["smithery/useCount"] as number | undefined;
  const verified = entry._meta?.["smithery/verified"] as boolean | undefined;
  const mcpsLiveCategory = entry._meta?.["mcps-live/category"] as
    | string
    | undefined;
  const mcpsLiveKeywords = entry._meta?.["mcps-live/keywords"] as
    | string[]
    | undefined;
  const mcpmCategories = entry._meta?.["mcpm/categories"] as
    | string[]
    | undefined;
  const mcpmTags = entry._meta?.["mcpm/tags"] as string[] | undefined;
  const mcpmOfficial = entry._meta?.["mcpm/official"] as boolean | undefined;

  const metadata = compact<MarketMetaItem>([
    { label: transport },
    source === "mcps-live" && { label: "MCPs Live" },
    source === "mcpm" && { label: "MCPM" },
    source === "mcps-live" && mcpsLiveCategory && { label: mcpsLiveCategory },
    source === "mcpm" && mcpmCategories?.[0] && { label: mcpmCategories[0] },
    source === "mcpm" &&
      mcpmOfficial && { label: t("registry.verified"), emphasis: "success" },
    source === "smithery" &&
      verified && { label: t("registry.verified"), emphasis: "success" },
    source === "smithery" &&
      useCount != null &&
      useCount > 0 && {
        label: t("registry.useCountShort", { value: formatCount(useCount) }),
      },
    source === "official" &&
      official?.isLatest !== false && { label: t("registry.latest") },
  ]);
  const links = compact<MarketLink>([
    server.repository?.url && {
      label: "GitHub",
      icon: IconBrandGithub,
      url: server.repository.url,
    },
    server.websiteUrl && {
      label: "Website",
      icon: IconExternalLink,
      url: server.websiteUrl,
    },
  ]);

  return {
    id: `${server.name}:${server.version ?? ""}`,
    title: server.title || server.name,
    subtitle: `${server.name}${server.version ? ` @ ${server.version}` : ""}`,
    metadata,
    description: server.description,
    tags:
      source === "mcps-live"
        ? mcpsLiveKeywords?.slice(0, 4)
        : source === "mcpm"
          ? mcpmTags?.slice(0, 4)
          : undefined,
    links,
    primaryAction: {
      label: t("common.add"),
      icon: IconPlus,
      onClick: () => onAdd(entry),
      loading: adding,
      disabled: !config,
    },
  };
}

export function skillMarketItem(
  entry: SkillMarketEntry,
  installed: boolean,
  installing: boolean,
  onInstall: (entry: SkillMarketEntry) => void,
  t: TFunction,
): MarketCardItem {
  const metadata = compact<MarketMetaItem>([
    entry.category && { label: entry.category },
    entry.language && { label: entry.language },
    entry.source === "anthropic" && { label: t("skillMarket.sourceAnthropic") },
    entry.securityStatus === "PASSED" && {
      label: t("skillMarket.verified"),
      icon: IconShieldCheck,
      emphasis: "success",
    },
    entry.stars != null &&
      entry.stars > 0 && {
        label: formatCount(entry.stars),
        icon: IconStar,
      },
  ]);
  const links = compact<MarketLink>([
    entry.repoUrl && {
      label: "Repo",
      icon: IconBrandGithub,
      url: entry.repoUrl,
    },
    entry.source === "anthropic" &&
      entry.downloadUrl && {
        label: "SKILL.md",
        icon: IconExternalLink,
        url: entry.downloadUrl,
      },
  ]);

  return {
    id: entry.id,
    title: entry.name,
    subtitle:
      entry.repoUrl?.replace(/^https?:\/\/github\.com\//, "") ??
      (entry.source === "anthropic" ? "anthropics/skills" : undefined),
    metadata,
    description: entry.description,
    tags: entry.topics,
    links,
    primaryAction: {
      label: t("skillMarket.install"),
      icon: IconDownload,
      onClick: () => onInstall(entry),
      loading: installing,
      installed,
      installedLabel: t("skillMarket.installed"),
    },
  };
}

export function safeSkillName(name: string) {
  return name.replace(/[^a-zA-Z0-9_-]/g, "-").toLowerCase();
}

function compact<T>(items: Array<T | false | "" | null | undefined>) {
  return items.filter(Boolean) as T[];
}
