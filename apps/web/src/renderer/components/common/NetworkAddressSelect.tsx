import React, { useEffect, useMemo, useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@mcp_link/ui";
import type { NetworkInterfaceAddress } from "@mcp_link/shared";

import { usePlatformAPI } from "@/renderer/platform-api";

interface NetworkAddressSelectProps {
  value: string;
  onValueChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

const NetworkAddressSelect: React.FC<NetworkAddressSelectProps> = ({
  value,
  onValueChange,
  disabled = false,
  placeholder,
}) => {
  const platformAPI = usePlatformAPI();
  const [interfaces, setInterfaces] = useState<NetworkInterfaceAddress[]>([]);

  useEffect(() => {
    let active = true;
    void platformAPI.settings
      .listNetworkInterfaces()
      .then((items) => active && setInterfaces(items))
      .catch(() => active && setInterfaces([]));
    return () => {
      active = false;
    };
  }, [platformAPI]);

  const options = useMemo(() => {
    if (!value || interfaces.some((item) => item.address === value)) {
      return interfaces;
    }
    return [
      {
        name: "Configured",
        address: value,
        family: "ipv4" as const,
        isLoopback: value === "127.0.0.1",
        label: value,
      },
      ...interfaces,
    ];
  }, [interfaces, value]);

  return (
    <Select
      value={value}
      onValueChange={onValueChange}
      disabled={disabled || options.length === 0}
    >
      <SelectTrigger>
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map((item) => (
          <SelectItem key={item.address} value={item.address}>
            {item.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default NetworkAddressSelect;
