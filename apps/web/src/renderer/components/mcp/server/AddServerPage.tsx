import React from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@mcp_link/ui";
import { Compass, PencilLine } from "lucide-react";
import { useTranslation } from "react-i18next";

import PageLayout from "@/renderer/components/layout/PageLayout";
import Manual from "./Manual";
import RegistryMarket from "./RegistryMarket";

const AddServerPage: React.FC = () => {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const requestedTab = params.get("tab");
  const tab = requestedTab === "registry" ? "registry" : "manual";

  const setTab = (value: string) => {
    setParams(value === "manual" ? {} : { tab: value });
  };

  return (
    <PageLayout title={t("serverList.addServer")} contentClassName="space-y-6">
      <Breadcrumb>
        <BreadcrumbList>
          <BreadcrumbItem>
            <BreadcrumbLink asChild>
              <Link to="/servers">{t("serverList.title")}</Link>
            </BreadcrumbLink>
          </BreadcrumbItem>
          <BreadcrumbSeparator />
          <BreadcrumbItem>
            <BreadcrumbPage>{t("serverList.addServer")}</BreadcrumbPage>
          </BreadcrumbItem>
        </BreadcrumbList>
      </Breadcrumb>

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="manual">
            <PencilLine className="h-4 w-4" />
            {t("manual.createManually")}
          </TabsTrigger>
          <TabsTrigger value="registry">
            <Compass className="h-4 w-4" />
            {t("registry.title")}
          </TabsTrigger>
        </TabsList>
        <TabsContent value="manual" className="mt-4">
          <Manual />
        </TabsContent>
        <TabsContent value="registry" className="mt-4">
          <RegistryMarket />
        </TabsContent>
      </Tabs>
    </PageLayout>
  );
};

export default AddServerPage;
