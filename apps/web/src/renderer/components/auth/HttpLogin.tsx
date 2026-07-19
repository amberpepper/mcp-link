import React, { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button, Input } from "@mcp_link/ui";
import { Eye, EyeOff } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

import { loginHttpSession } from "@/renderer/platform-api/http-platform-api";

const HttpLogin: React.FC = () => {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const canSubmit = useMemo(() => password.trim().length > 0, [password]);

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setIsSubmitting(true);
    try {
      await loginHttpSession(password.trim());
      toast.success(t("auth.loginSuccess"));
      navigate("/servers", { replace: true });
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : t("auth.loginFailed"),
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="flex min-h-full items-center justify-center bg-background px-4">
      <div className="w-full max-w-sm rounded-md border bg-card p-5 shadow-sm">
        <h1 className="text-lg font-semibold">MCP Link</h1>
        <div className="mt-4 space-y-3">
          <div className="relative">
            <Input
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder={t("auth.serverPassword")}
              type={showPassword ? "text" : "password"}
              className="hide-native-password-toggle pr-10"
              onKeyDown={(event) => {
                if (event.key === "Enter" && canSubmit && !isSubmitting) {
                  handleSubmit();
                }
              }}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="absolute right-1 top-1/2 h-7 w-7 -translate-y-1/2"
              onClick={() => setShowPassword((value) => !value)}
              aria-label={
                showPassword
                  ? t("common.hidePassword")
                  : t("common.showPassword")
              }
            >
              {showPassword ? (
                <EyeOff className="h-4 w-4" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
            </Button>
          </div>
          <Button
            type="button"
            className="w-full"
            disabled={!canSubmit || isSubmitting}
            onClick={handleSubmit}
          >
            {t("auth.login")}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default HttpLogin;
