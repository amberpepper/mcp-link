import React from "react";
import type { MCPServer } from "@mcp_link/shared";
import { ScrollArea } from "@mcp_link/ui";

import { ServerCard } from "./ServerCard";
import { ServerRow } from "./ServerRow";

interface ServerListViewProps {
  servers: MCPServer[];
  view: "list" | "grid";
  onToggle: (server: MCPServer, checked: boolean) => void | Promise<void>;
  onClick: (server: MCPServer) => void;
  onDelete: (server: MCPServer) => void;
  onError: (server: MCPServer) => void;
  onDuplicate: (server: MCPServer) => void;
  onExport: (server: MCPServer) => void;
}

const ServerListView: React.FC<ServerListViewProps> = ({
  servers,
  view,
  onToggle,
  onClick,
  onDelete,
  onError,
  onDuplicate,
  onExport,
}) => {
  if (view === "grid") {
    return (
      <ScrollArea className="h-full">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3 p-3">
          {servers.map((server) => (
            <ServerCard
              key={server.id}
              server={server}
              onClick={() => onClick(server)}
              onToggle={(checked) => onToggle(server, checked)}
              onDelete={() => onDelete(server)}
              onError={() => onError(server)}
              onDuplicate={() => onDuplicate(server)}
              onExport={() => onExport(server)}
            />
          ))}
        </div>
      </ScrollArea>
    );
  }

  return (
    <ScrollArea className="h-full">
      <div className="divide-y divide-border">
        {servers.map((server) => (
          <ServerRow
            key={server.id}
            server={server}
            onClick={() => onClick(server)}
            onToggle={(checked) => onToggle(server, checked)}
            onDelete={() => onDelete(server)}
            onError={() => onError(server)}
            onDuplicate={() => onDuplicate(server)}
            onExport={() => onExport(server)}
          />
        ))}
      </div>
    </ScrollArea>
  );
};

export default ServerListView;
