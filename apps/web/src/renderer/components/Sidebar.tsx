import React from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import {
  IconActivity,
  IconBuildingStore,
  IconGitFork,
  IconKey,
  IconServer,
  IconSettings,
  IconSparkles,
} from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@mcp_link/ui";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@mcp_link/ui";

import { useServerStore } from "@/renderer/stores";
import iconImage from "../../../public/images/icon/icon.png";

const NAV = [
  {
    path: "/servers",
    icon: IconServer,
    labelKey: "serverList.title",
    withCount: true,
  },
  {
    path: "/logs",
    icon: IconActivity,
    labelKey: "serverDetails.requestLogs",
  },
  { path: "/keys", icon: IconKey, labelKey: "keys.title" },
  { path: "/hooks", icon: IconGitFork, labelKey: "hooks.title" },
  { path: "/skills", icon: IconSparkles, labelKey: "skills.title" },
  {
    path: "/market-sources",
    icon: IconBuildingStore,
    labelKey: "marketSources.title",
  },
];

const SidebarComponent: React.FC = () => {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const serverCount = useServerStore((state) => state.servers.length);

  const navLinkClass =
    "flex items-center gap-2.5 py-2.5 px-2.5 w-full text-left";
  const navIconClass = "h-5 w-5";
  const navTextClass = "text-sm";

  return (
    <Sidebar variant="floating">
      <div className="pt-2" />
      <SidebarHeader>
        <Link to="/" className="flex items-center px-2 py-2 no-underline">
          <img src={iconImage} className="mr-2.5 h-7 w-7" alt="Logo" />
          <h1 className="text-base font-semibold">{t("home.title")}</h1>
        </Link>
      </SidebarHeader>

      <SidebarContent className="px-2">
        <SidebarMenu>
          {NAV.map((item) => {
            const Icon = item.icon;
            const isActive =
              location.pathname === item.path ||
              (item.path !== "/servers" &&
                location.pathname.startsWith(`${item.path}/`)) ||
              (item.path === "/servers" &&
                location.pathname.startsWith("/servers"));
            return (
              <SidebarMenuItem key={item.path}>
                <SidebarMenuButton
                  asChild
                  tooltip={t(item.labelKey)}
                  isActive={isActive}
                >
                  <button
                    type="button"
                    className={navLinkClass}
                    onClick={() => {
                      if (location.pathname !== item.path) {
                        navigate(item.path);
                      }
                    }}
                  >
                    <Icon className={navIconClass} />
                    <span className={navTextClass}>{t(item.labelKey)}</span>
                    {item.withCount && (
                      <Badge variant="secondary" className="ml-auto h-5 px-1.5">
                        {serverCount}
                      </Badge>
                    )}
                  </button>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
        </SidebarMenu>
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              asChild
              tooltip={t("common.settings")}
              isActive={location.pathname === "/settings"}
            >
              <Link to="/settings" className={navLinkClass}>
                <IconSettings className={navIconClass} />
                <span className={navTextClass}>{t("common.settings")}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  );
};

export default SidebarComponent;
