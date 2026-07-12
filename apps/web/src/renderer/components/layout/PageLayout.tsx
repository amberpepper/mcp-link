import React from "react";
import { Outlet } from "react-router-dom";
import { cn } from "@/renderer/utils/tailwind-utils";

interface PageLayoutProps {
  title?: React.ReactNode;
  toolbar?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
  headerClassName?: string;
  contentClassName?: string;
}

const PageLayout: React.FC<PageLayoutProps> = ({
  title,
  toolbar,
  children,
  className,
  headerClassName,
  contentClassName,
}) => {
  const hasHeader = Boolean(title || toolbar);

  return (
    <div className={cn("flex h-full min-h-0 w-full flex-col", className)}>
      {hasHeader && (
        <header
          className={cn(
            "sticky top-0 z-10 flex items-center justify-between gap-4 border-b bg-background px-6 py-4",
            headerClassName,
          )}
        >
          {title && (
            <h1 className="min-w-0 text-xl font-semibold leading-7">{title}</h1>
          )}
          {toolbar && (
            <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
              {toolbar}
            </div>
          )}
        </header>
      )}
      <div
        className={cn(
          "flex-1 overflow-auto px-6 py-4 min-h-0",
          contentClassName,
        )}
      >
        {children ?? <Outlet />}
      </div>
    </div>
  );
};

export default PageLayout;
