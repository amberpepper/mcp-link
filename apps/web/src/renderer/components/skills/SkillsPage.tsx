import React from "react";
import { Link } from "react-router-dom";
import { Button } from "@mcp_link/ui";
import { PackageSearch } from "lucide-react";
import { useTranslation } from "react-i18next";

import SkillsManager from "./SkillsManager";

const SkillsPage: React.FC = () => {
  const { t } = useTranslation();

  return (
    <SkillsManager
      toolbarEnd={
        <Button asChild variant="outline" size="sm" className="h-8 gap-2">
          <Link to="/skills/market">
            <PackageSearch className="h-4 w-4" />
            {t("skillMarket.title")}
          </Link>
        </Button>
      }
    />
  );
};

export default SkillsPage;
