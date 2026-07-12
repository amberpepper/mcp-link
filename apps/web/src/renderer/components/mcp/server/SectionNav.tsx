import React from "react";
import { Button } from "@mcp_link/ui";
import { cn } from "@/renderer/utils/tailwind-utils";

interface DetailSection {
  id: string;
  label: string;
}

interface SectionNavProps {
  sections: DetailSection[];
  activeSection: string;
  onSelect: (section: string) => void;
}

const SectionNav: React.FC<SectionNavProps> = ({
  sections,
  activeSection,
  onSelect,
}) => (
  <nav className="flex gap-2 lg:w-56 lg:flex-col">
    {sections.map((section) => (
      <Button
        key={section.id}
        type="button"
        variant="ghost"
        className={cn(
          "justify-start",
          activeSection === section.id && "bg-muted text-primary",
        )}
        onClick={() => onSelect(section.id)}
      >
        {section.label}
      </Button>
    ))}
  </nav>
);

export default SectionNav;
