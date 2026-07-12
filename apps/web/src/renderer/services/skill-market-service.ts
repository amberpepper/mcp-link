import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/renderer/platform-api/tauri-platform-api";
import { callHttpPlatform } from "@/renderer/platform-api/http-platform-api";

export type SkillMarketSource = "community" | "anthropic";

export interface SkillMarketEntry {
  id: string;
  name: string;
  description: string;
  repoUrl?: string;
  repoOwner?: string;
  repoName?: string;
  stars?: number;
  language?: string;
  topics?: string[];
  category?: string;
  securityStatus?: string;
  hasSkillMd?: boolean;
  skillMdContent?: string | null;
  source: SkillMarketSource;
  downloadUrl?: string;
}

export interface SkillMarketResult {
  skills: SkillMarketEntry[];
  total?: number;
  pages?: number;
  currentPage?: number;
  hasMore?: boolean;
}

// ─── Proxy fetch: routes through Rust (Tauri) or server backend to bypass CORS ───

async function proxyFetch(url: string): Promise<any> {
  if (isTauriRuntime()) {
    return await invoke("platform_call", {
      method: "proxyFetch",
      args: [url],
    });
  }

  return await callHttpPlatform("proxyFetch", [url]);
}

async function proxyFetchText(url: string): Promise<string | null> {
  if (isTauriRuntime()) {
    return await invoke("platform_call", {
      method: "proxyFetchText",
      args: [url],
    });
  }
  return await callHttpPlatform<string | null>("proxyFetchText", [url]);
}

// ─── skillsllm.com (Community) ───

interface SkillsLLMServer {
  id: string;
  slug: string;
  name: string;
  description: string;
  repoUrl?: string;
  repoOwner?: string;
  repoName?: string;
  stars?: number;
  forks?: number;
  topics?: string[];
  language?: string;
  hasSkillMd?: boolean;
  skillMdContent?: string | null;
  securityStatus?: string;
  category?: { name?: string; slug?: string };
}

interface SkillsLLMResponse {
  skills?: SkillsLLMServer[];
  data?: SkillsLLMServer[];
  pagination?: {
    page?: number;
    limit?: number;
    total?: number;
    pages?: number;
  };
}

export async function fetchCommunitySkills(options: {
  search?: string;
  page?: number;
  pageSize?: number;
}): Promise<SkillMarketResult> {
  const params = new URLSearchParams();
  params.set("page", String(options.page ?? 1));
  params.set("limit", String(options.pageSize ?? 24));
  if (options.search) params.set("search", options.search);

  const data: SkillsLLMResponse = await proxyFetch(
    `https://skillsllm.com/api/skills?${params.toString()}`,
  );
  const items = data.skills ?? data.data ?? [];

  const skills: SkillMarketEntry[] = items.map((item) => {
    const repo = parseGithubRepoUrl(item.repoUrl);
    return {
      id: item.slug || item.id,
      name: item.name,
      description: item.description ?? "",
      repoUrl: item.repoUrl,
      repoOwner: item.repoOwner ?? repo?.owner,
      repoName: item.repoName ?? repo?.name,
      stars: item.stars,
      language: item.language,
      topics: item.topics,
      category: item.category?.name,
      securityStatus: item.securityStatus,
      hasSkillMd: item.hasSkillMd,
      skillMdContent: item.skillMdContent,
      source: "community" as const,
      downloadUrl: item.repoUrl,
    };
  });

  return {
    skills,
    total: data.pagination?.total ?? skills.length,
    pages: data.pagination?.pages,
    currentPage: data.pagination?.page,
    hasMore:
      data.pagination?.page != null &&
      data.pagination?.pages != null &&
      data.pagination.page < data.pagination.pages,
  };
}

// ─── anthropics/skills (Official GitHub) ───

interface GitHubContentItem {
  name: string;
  type: string;
  path: string;
}

interface AnthropicSkillMeta {
  name: string;
  description: string;
}

function parseSkillMdFrontmatter(content: string): AnthropicSkillMeta | null {
  const match = content.match(/^---\n([\s\S]*?)\n---/);
  if (!match) return null;
  const frontmatter = match[1];
  const nameMatch = frontmatter.match(/^name:\s*(.+)$/m);
  const descMatch = frontmatter.match(/^description:\s*(.+)$/m);
  if (!nameMatch) return null;
  return {
    name: nameMatch[1].trim(),
    description: descMatch?.[1]?.trim() ?? "",
  };
}

export async function fetchAnthropicSkills(options: {
  search?: string;
  page?: number;
  pageSize?: number;
}): Promise<SkillMarketResult> {
  const data: GitHubContentItem[] = await proxyFetch(
    "https://api.github.com/repos/anthropics/skills/contents/skills",
  );
  const dirs = data.filter((item) => item.type === "dir");

  const search = options.search?.toLowerCase().trim();
  const filtered = search
    ? dirs.filter((d) => d.name.toLowerCase().includes(search))
    : dirs;
  const pageSize = Math.max(1, options.pageSize ?? 24);
  const currentPage = Math.max(1, options.page ?? 1);
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const pageDirs = filtered.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  );

  const skills: SkillMarketEntry[] = await Promise.all(
    pageDirs.map(async (dir) => {
      const skillUrl = `https://raw.githubusercontent.com/anthropics/skills/main/skills/${dir.name}/SKILL.md`;
      let description = "";
      let skillMdContent: string | null = null;

      try {
        skillMdContent = await proxyFetchText(skillUrl);
        if (skillMdContent) {
          const meta = parseSkillMdFrontmatter(skillMdContent);
          if (meta) {
            description = meta.description;
          }
        }
      } catch {
        // ignore fetch errors
      }

      return {
        id: `anthropic-${dir.name}`,
        name: dir.name,
        description,
        repoUrl: `https://github.com/anthropics/skills/tree/main/skills/${dir.name}`,
        repoOwner: "anthropics",
        repoName: "skills",
        stars: 0,
        topics: ["official", "anthropic"],
        category: "Official",
        hasSkillMd: !!skillMdContent,
        skillMdContent,
        source: "anthropic" as const,
        downloadUrl: skillUrl,
      };
    }),
  );

  return {
    skills,
    total: filtered.length,
    pages,
    currentPage,
    hasMore: currentPage < pages,
  };
}

// ─── Install helper: fetch SKILL.md content ───

export async function fetchSkillContent(
  entry: SkillMarketEntry,
): Promise<string | null> {
  if (entry.skillMdContent) {
    return entry.skillMdContent;
  }

  if (entry.source === "anthropic" && entry.downloadUrl) {
    const text = await proxyFetchText(entry.downloadUrl);
    if (text) return text;
  }

  const repo =
    entry.repoOwner && entry.repoName
      ? { owner: entry.repoOwner, name: entry.repoName }
      : parseGithubRepoUrl(entry.repoUrl);

  if (repo) {
    const branches = ["main", "master"];
    for (const branch of branches) {
      for (const path of ["SKILL.md", "skill.md"]) {
        const url = `https://raw.githubusercontent.com/${repo.owner}/${repo.name}/${branch}/${path}`;
        const text = await proxyFetchText(url);
        if (text) return text;
      }
    }
  }

  return null;
}

function parseGithubRepoUrl(url: string | undefined):
  | {
      owner: string;
      name: string;
    }
  | undefined {
  if (!url) return undefined;
  const match = url.match(/^https?:\/\/github\.com\/([^/]+)\/([^/#?]+)/i);
  if (!match) return undefined;
  return {
    owner: match[1],
    name: match[2].replace(/\.git$/i, ""),
  };
}
