import React from "react";
import type { Icon } from "@tabler/icons-react";

interface EmptyStateProps {
  icon: Icon;
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
}

const EmptyState: React.FC<EmptyStateProps> = ({
  icon: IconComponent,
  title,
  description,
  action,
}) => (
  <div className="flex min-h-[260px] items-center justify-center p-6">
    <div className="mx-auto flex max-w-sm flex-col items-center text-center">
      <IconComponent className="mb-4 h-12 w-12 text-muted-foreground/50" />
      <div className="text-base font-medium">{title}</div>
      {description && (
        <div className="mt-1 text-sm text-muted-foreground">{description}</div>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  </div>
);

export default EmptyState;
