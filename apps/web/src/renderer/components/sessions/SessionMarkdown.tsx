import React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { IconCopy } from "@tabler/icons-react";
import { Button, Tooltip, TooltipContent, TooltipTrigger } from "@mcp_link/ui";

const MarkdownCodeBlock: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { t } = useTranslation();
  const copyCode = async () => {
    try {
      await navigator.clipboard.writeText(
        reactNodeText(children).replace(/\n$/, ""),
      );
      toast.success(t("sessions.codeCopied"));
    } catch {
      toast.error(t("sessions.codeCopyFailed"));
    }
  };

  return (
    <div className="group/code relative my-3 max-w-full overflow-hidden rounded-md bg-muted">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            size="icon"
            variant="ghost"
            className="absolute right-1.5 top-1.5 z-10 h-7 w-7 bg-muted/90 opacity-0 transition-opacity hover:bg-background group-hover/code:opacity-100 focus-visible:opacity-100"
            aria-label={t("sessions.copyCode")}
            onClick={() => void copyCode()}
          >
            <IconCopy className="h-3.5 w-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t("sessions.copyCode")}</TooltipContent>
      </Tooltip>
      <pre className="max-w-full overflow-x-auto p-3 pr-11 font-mono text-xs leading-5 [&_code]:break-normal [&_code]:bg-transparent [&_code]:p-0">
        {children}
      </pre>
    </div>
  );
};

const SessionMarkdown: React.FC<{ content: string }> = React.memo(
  ({ content }) => (
    <div className="min-w-0 max-w-full break-words text-sm leading-6">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        components={{
          h1: ({ children }) => (
            <h1 className="mb-3 mt-4 text-xl font-semibold first:mt-0">
              {children}
            </h1>
          ),
          h2: ({ children }) => (
            <h2 className="mb-2 mt-4 text-lg font-semibold first:mt-0">
              {children}
            </h2>
          ),
          h3: ({ children }) => (
            <h3 className="mb-2 mt-3 text-base font-semibold first:mt-0">
              {children}
            </h3>
          ),
          p: ({ children }) => (
            <p className="my-2 whitespace-pre-wrap first:mt-0 last:mb-0">
              {children}
            </p>
          ),
          ul: ({ children }) => (
            <ul className="my-2 list-disc space-y-1 pl-6">{children}</ul>
          ),
          ol: ({ children }) => (
            <ol className="my-2 list-decimal space-y-1 pl-6">{children}</ol>
          ),
          li: ({ children }) => <li className="pl-1">{children}</li>,
          blockquote: ({ children }) => (
            <blockquote className="my-3 border-l-4 border-primary/40 pl-4 text-foreground/75">
              {children}
            </blockquote>
          ),
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noreferrer"
              className="break-all text-primary underline underline-offset-2 hover:opacity-80"
            >
              {children}
            </a>
          ),
          pre: ({ children }) => (
            <MarkdownCodeBlock>{children}</MarkdownCodeBlock>
          ),
          code: ({ className, children }) => (
            <code
              className={`${className ?? ""} break-all rounded bg-muted px-1 py-0.5 font-mono text-[0.9em]`}
            >
              {children}
            </code>
          ),
          table: ({ children }) => (
            <div className="my-3 max-w-full overflow-x-auto">
              <table className="w-full border-collapse text-left text-sm">
                {children}
              </table>
            </div>
          ),
          thead: ({ children }) => (
            <thead className="bg-muted/70">{children}</thead>
          ),
          th: ({ children }) => (
            <th className="border px-3 py-2 font-semibold">{children}</th>
          ),
          td: ({ children }) => (
            <td className="border px-3 py-2 align-top">{children}</td>
          ),
          hr: () => <hr className="my-4 border-border" />,
          img: ({ src, alt }) => (
            <img
              src={src}
              alt={alt ?? ""}
              loading="lazy"
              className="my-3 max-h-[520px] max-w-full rounded-md border object-contain"
            />
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  ),
);
SessionMarkdown.displayName = "SessionMarkdown";

function reactNodeText(node: React.ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(reactNodeText).join("");
  if (React.isValidElement<{ children?: React.ReactNode }>(node))
    return reactNodeText(node.props.children);
  return "";
}

export default SessionMarkdown;
