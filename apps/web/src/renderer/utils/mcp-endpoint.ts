export function usableMcpEndpoint(endpoint: string) {
  try {
    const url = new URL(endpoint);
    if (["0.0.0.0", "::", "[::]"].includes(url.hostname)) {
      url.hostname = window.location.hostname || "127.0.0.1";
    }
    return url.toString().replace(/\/$/, "");
  } catch {
    return endpoint;
  }
}
