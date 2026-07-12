import React from "react";

import { cn } from "@/renderer/utils/tailwind-utils";

interface EmbeddedSkillPageProps {
  title?: React.ReactNode;
  toolbar?: React.ReactNode;
  children?: React.ReactNode;
  contentClassName?: string;
}

const EmbeddedSkillPage: React.FC<EmbeddedSkillPageProps> = ({
  toolbar,
  children,
  contentClassName,
}) => (
  <div className="flex h-full min-h-0 w-full flex-col">
    {toolbar && (
      <header className="flex items-center justify-end gap-2 border-b bg-background px-6 py-4">
        <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
          {toolbar}
        </div>
      </header>
    )}
    <div
      className={cn("min-h-0 flex-1 overflow-auto px-6 py-4", contentClassName)}
    >
      {children}
    </div>
  </div>
);

export default EmbeddedSkillPage;
