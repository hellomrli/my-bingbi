/** Extracts the stable error code from an IPC failure (`{ code, message }` or `IpcError`). */
export function errorCode(error: unknown): string | null {
  return typeof error === "object" && error !== null && "code" in error && typeof error.code === "string"
    ? error.code
    : null;
}
