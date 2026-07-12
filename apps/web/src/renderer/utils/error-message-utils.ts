import { ParsedPaymentError } from "@mcp_link/shared";

/**
 * 错误消息解析工具，用于生成更适合展示给用户的文案。
 */

/**
 * 解析错误消息，识别支付相关错误并提取可展示文案。
 */
export function parseErrorMessage(errorMessage: string): ParsedPaymentError {
  const result: ParsedPaymentError = {
    isPaymentError: false,
    displayMessage: errorMessage,
    originalMessage: errorMessage,
  };

  try {
    // 402 错误可能以 JSON 字符串形式返回。
    const parsed = JSON.parse(errorMessage);

    if (parsed.code === "insufficient_credits") {
      result.isPaymentError = false;
      result.code = parsed.code;
      result.displayMessage = parsed.message || "额度不足";
      return result;
    }

    // 兼容其他 JSON 错误格式。
    if (parsed.message) {
      result.displayMessage = parsed.message;
    }
  } catch {
    // 非 JSON 时按纯文本处理，并根据 HTTP 状态判断是否为支付错误。
    if (
      errorMessage.includes("402") ||
      errorMessage.toLowerCase().includes("payment required")
    ) {
      result.isPaymentError = false;
      result.displayMessage = "额度不足";
    }
  }

  return result;
}
