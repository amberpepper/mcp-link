import React, { useState } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import {
  IconActivity,
  IconBuildingStore,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
  IconGitFork,
  IconKey,
  IconMessages,
  IconPlug,
  IconRoute,
  IconServer,
  IconSparkles,
  IconSettings,
} from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import {
  Badge,
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarResizer,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  ScrollArea,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  useSidebar,
} from "@mcp_link/ui";
import SessionSidebarTree from "./sessions/SessionSidebarTree";

import { useServerStore } from "@/renderer/stores";
import { cn } from "@/renderer/utils/tailwind-utils";
import iconImage from "/images/icon/icon.png";

const NAV = [
  {
    path: "/agents",
    icon: IconPlug,
    labelKey: "agents.title",
    activePaths: ["/agents"],
  },
  {
    path: "/logs",
    icon: IconActivity,
    labelKey: "navigation.activity",
    activePaths: ["/logs"],
  },
];

const INTEGRATION_NAV = [
  { path: "/servers", icon: IconServer, labelKey: "integrationNav.mcp" },
  { path: "/gateway", icon: IconRoute, labelKey: "integrationNav.gateway" },
  { path: "/skills", icon: IconSparkles, labelKey: "integrationNav.skills" },
  { path: "/hooks", icon: IconGitFork, labelKey: "integrationNav.hooks" },
  { path: "/keys", icon: IconKey, labelKey: "integrationNav.keys" },
  {
    path: "/market-sources",
    icon: IconBuildingStore,
    labelKey: "integrationNav.marketSources",
  },
];

const SidebarComponent: React.FC = () => {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const serverCount = useServerStore((state) => state.servers.length);
  const { open, toggleSidebar } = useSidebar();
  const isSessionsRoute =
    location.pathname === "/sessions" ||
    location.pathname.startsWith("/sessions/");
  const isIntegrationRoute = INTEGRATION_NAV.some(
    (item) =>
      location.pathname === item.path ||
      location.pathname.startsWith(`${item.path}/`),
  );
  const [sessionsOpen, setSessionsOpen] = useState(isSessionsRoute);
  const [integrationsOpen, setIntegrationsOpen] = useState(isIntegrationRoute);

  const navLinkClass =
    "flex items-center gap-2.5 py-2.5 px-2.5 w-full text-left group-data-[collapsible=icon]:mx-auto";
  const navIconClass = "h-5 w-5";
  const navTextClass = "text-sm";

  return (
    <Sidebar variant="sidebar" collapsible="icon">
      <div className="pt-2" />
      <SidebarHeader>
        <Link
          to="/"
          className="flex items-center px-2 py-2 no-underline group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
        >
          <img
            src={iconImage}
            className="mr-2.5 h-7 w-7 shrink-0 group-data-[collapsible=icon]:mr-0"
            alt="Logo"
          />
          <h1 className="text-base font-semibold group-data-[collapsible=icon]:hidden">
            {t("home.title")}
          </h1>
        </Link>
      </SidebarHeader>

      <SidebarContent className="overflow-hidden px-0">
        <ScrollArea
          horizontal={false}
          className="min-h-0 min-w-0 flex-1 [&>div]:max-w-full [&>div]:overflow-x-hidden"
        >
          <div className="flex min-w-0 max-w-full flex-col gap-2 overflow-x-hidden pl-2 pr-4 group-data-[collapsible=icon]:pr-2">
            <SidebarMenu>
              {/* AI 会话 — multi-level session tree */}
              <SidebarMenuItem>
                <Collapsible open={sessionsOpen} onOpenChange={setSessionsOpen}>
                  <SidebarMenuButton
                    asChild
                    tooltip={t("sessions.title")}
                    isActive={isSessionsRoute}
                  >
                    <button
                      type="button"
                      className={navLinkClass}
                      onClick={() => setSessionsOpen((prev) => !prev)}
                    >
                      <IconMessages className={navIconClass} />
                      <span className={navTextClass}>
                        {t("sessions.title")}
                      </span>
                    </button>
                  </SidebarMenuButton>
                  <CollapsibleTrigger asChild>
                    <SidebarMenuAction>
                      <IconChevronDown
                        className={cn(
                          "h-4 w-4 transition-transform",
                          sessionsOpen && "rotate-180",
                        )}
                      />
                    </SidebarMenuAction>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <SessionSidebarTree />
                  </CollapsibleContent>
                </Collapsible>
              </SidebarMenuItem>
            </SidebarMenu>

            <SidebarMenu>
              <SidebarMenuItem>
                <Collapsible
                  open={integrationsOpen}
                  onOpenChange={setIntegrationsOpen}
                >
                  <SidebarMenuButton
                    asChild
                    tooltip={t("navigation.integrations")}
                    isActive={isIntegrationRoute}
                  >
                    <button
                      type="button"
                      className={navLinkClass}
                      onClick={() => setIntegrationsOpen((prev) => !prev)}
                    >
                      <IconBuildingStore className={navIconClass} />
                      <span className={navTextClass}>
                        {t("navigation.integrations")}
                      </span>
                    </button>
                  </SidebarMenuButton>
                  <CollapsibleTrigger asChild>
                    <SidebarMenuAction>
                      <IconChevronDown
                        className={cn(
                          "h-4 w-4 transition-transform",
                          integrationsOpen && "rotate-180",
                        )}
                      />
                    </SidebarMenuAction>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <SidebarMenuSub className="ml-3 mr-0 min-w-0 max-w-full pr-0">
                      {INTEGRATION_NAV.map((item) => {
                        const Icon = item.icon;
                        const active =
                          location.pathname === item.path ||
                          location.pathname.startsWith(`${item.path}/`);
                        return (
                          <SidebarMenuSubItem key={item.path}>
                            <SidebarMenuSubButton asChild isActive={active}>
                              <Link to={item.path}>
                                <Icon className="h-4 w-4" />
                                <span>{t(item.labelKey)}</span>
                                {item.path === "/servers" && (
                                  <Badge
                                    variant="secondary"
                                    className="ml-auto h-5 px-1.5"
                                  >
                                    {serverCount}
                                  </Badge>
                                )}
                              </Link>
                            </SidebarMenuSubButton>
                          </SidebarMenuSubItem>
                        );
                      })}
                    </SidebarMenuSub>
                  </CollapsibleContent>
                </Collapsible>
              </SidebarMenuItem>

              {NAV.map((item) => {
                const Icon = item.icon;
                const isActive = item.activePaths.some(
                  (path) =>
                    location.pathname === path ||
                    location.pathname.startsWith(`${path}/`),
                );
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
                      </button>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                );
              })}
            </SidebarMenu>
          </div>
        </ScrollArea>
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
      <SidebarResizer />
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={toggleSidebar}
            aria-label={
              open ? t("common.collapseSidebar") : t("common.expandSidebar")
            }
            className="absolute right-0 top-1/2 z-40 flex h-7 w-7 -translate-y-1/2 translate-x-1/2 items-center justify-center rounded-full border border-sidebar-border bg-background text-sidebar-foreground shadow-sm hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
          >
            {open ? (
              <IconChevronLeft className="h-4 w-4" />
            ) : (
              <IconChevronRight className="h-4 w-4" />
            )}
          </button>
        </TooltipTrigger>
        <TooltipContent>
          {open ? t("common.collapseSidebar") : t("common.expandSidebar")}
        </TooltipContent>
      </Tooltip>
    </Sidebar>
  );
};

export default SidebarComponent;
